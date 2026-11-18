//! Bind original pattern names to actual storage through exact subject SSA,
//! tag/field selection and destination Store operations. Names and equal
//! physical types do not choose a destination.
use super::*;
use crate::semantic::value::Class;

impl Check<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn original_pattern_borrow_class(
        &self, instance: &SemanticInstance, function: &TypedFunction, local: LocalId,
        path: &[PlaceProjection], mutable: bool, span: zil_base::Span, b: &mut Budget, depth: usize,
    ) -> CompileResult<Option<Class>> {
        let mut selected = None;
        for origin in self.original.pattern_origins() {
            b.spend(1, WORK)?;
            let Some(borrow) = origin.borrow.filter(|borrow| borrow.span == span && origin.owner == instance.definition) else { continue; };
            b.spend(origin.subject.path.len().saturating_add(origin.path.len()).saturating_add(path.len()), WORK)?;
            let root = if let Some(anchor) = origin.subject.anchor {
                self.anchored_local_at(instance, function, anchor, b, depth + 1)?
            } else {
                let value = self.projected_pattern_subject(instance, function, origin.subject.node,
                    origin.subject.success_projections, b, depth + 1)?
                    .ok_or_else(|| fail("pattern borrow temporary lacks its exact original source"))?;
                self.stored_pattern_value(function, value, b)?
                    .ok_or_else(|| fail("pattern borrow temporary lacks its original holding slot"))?
            };
            let expected = origin.subject.path.iter().copied().map(PlaceProjection::Field).chain(origin.path.iter().copied());
            if root != local || borrow.mutable != mutable || !path.iter().copied().eq(expected) {
                return Err(fail("actual reference pattern changes its original root, active variant, field, or mode"));
            }
            if selected.replace(Class::Implicit(instance.id, borrow.ordinal)).is_some() {
                return Err(fail("actual reference pattern has competing original borrow occurrences"));
            }
        }
        Ok(selected)
    }

    pub(super) fn check_pattern_borrows(&self, b: &mut Budget) -> CompileResult<()> {
        let mut owners = BTreeSet::new();
        for origin in self.original.pattern_origins() {
            b.spend(1, WORK)?;
            if origin.borrow.is_some() {
                b.lookup(owners.len(), WORK)?;
                owners.insert(origin.owner);
            }
        }
        if owners.is_empty() { return Ok(()); }
        for instance in &self.program.instances {
            b.lookup(owners.len(), WORK)?;
            if !owners.contains(&instance.definition) { continue; }
            let SemanticInstanceTarget::Function(target) = &instance.target else { continue; };
            let function = self.inventory.function(target, b)?;
            for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
                b.spend(1, WORK)?;
                let (local, path, mutable): (_, &[PlaceProjection], _) = match &instruction.kind {
                    InstructionKind::BorrowPlaceLocal { local, path, mutable } => (*local, path, *mutable),
                    InstructionKind::BorrowLocal { local } => (*local, &[], false),
                    InstructionKind::BorrowLocalMut { local } => (*local, &[], true),
                    _ => continue,
                };
                self.original_pattern_borrow_class(instance, function, local, path, mutable, instruction.span, b, 0)?;
            }
        }
        Ok(())
    }

    pub(super) fn original_subject_value(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        node: zil_base::NodeId,
        b: &mut Budget,
    ) -> CompileResult<Option<ValueId>> {
        let mut result = None;
        for site in self.inventory.sites_for_expression(self.program,instance,node,b)? {
            let SemanticSiteAddress::Typed {block,instruction,edge}=site.at else {
                return Err(fail("original operation retains a Core occurrence in an actual function"));
            };
            let actual=function.blocks.get(block.0 as usize).filter(|row|row.id==block)
                .and_then(|row|row.instructions.get(instruction as usize));
            let selected=match &site.action {
                SemanticSiteAction::TypeUse {subject:SemanticOperand::Value(value),..}=>Some(*value),
                SemanticSiteAction::TypeUse {subject,..}=>match (subject,actual.map(|row|&row.kind)) {
                    (SemanticOperand::Local(expected),Some(InstructionKind::Load(local)
                        |InstructionKind::TakeLocal {local})) if expected==local=>actual.and_then(|row|row.result),
                    (SemanticOperand::Capture(expected),Some(InstructionKind::ReadCapture {index}
                        |InstructionKind::TakeCapture {index})) if expected==index=>actual.and_then(|row|row.result),
                    _=>None,
                },
                SemanticSiteAction::Construct {result,..}|SemanticSiteAction::Closure {result,..}=>{
                    let actual=actual.ok_or_else(||fail("original constructor has no actual operation"))?;
                    let SemanticOperand::Value(value)=result else {
                        return Err(fail("actual constructor output is not an SSA occurrence"));
                    };
                    if actual.result!=Some(*value) {return Err(fail("constructor occurrence selects another actual output"));}
                    Some(*value)
                }
                SemanticSiteAction::Call {..}|SemanticSiteAction::BodyCall {..}
                    |SemanticSiteAction::CallableCall {..} if edge==SemanticSiteEdge::After=>
                    actual.ok_or_else(||fail("original invocation has no actual operation"))?.result,
                SemanticSiteAction::Call {..}|SemanticSiteAction::BodyCall {..}
                    |SemanticSiteAction::CallableCall {..}|SemanticSiteAction::DeclareLocal {..}=>None,
                SemanticSiteAction::CoreBodyCall=>return Err(fail("actual operation retains an unfinished Core call")),
            };
            if let Some(value)=selected {
                if function.values.get(value.0 as usize).is_none() {return Err(fail("original operation selects no actual SSA"));}
                if result.is_some_and(|old|old!=value) {return Err(fail("original operation has competing actual outputs"));}
                result=Some(value);
            }
        }
        Ok(result)
    }

    fn projected_pattern_subject(
        &self, instance: &SemanticInstance, function: &TypedFunction,
        node: zil_base::NodeId, success_projections: usize, b: &mut Budget, depth: usize,
    ) -> CompileResult<Option<ValueId>> {
        let Some(mut current) = self.original_subject_value(instance, function, node, b)? else {
            return Ok(None);
        };
        for _ in 0..success_projections {
            let mut selected = None;
            for block in &function.blocks {
                for instruction in &block.instructions {
                    b.spend(1, WORK)?;
                    if !matches!(instruction.kind, InstructionKind::ReadErrorUnion { is_error: false, .. }
                        | InstructionKind::TakeErrorUnion { is_error: false, .. }) {
                        continue;
                    }
                    let output = instruction.result.ok_or_else(|| fail("actual success projection has no SSA result"))?;
                    if self.pattern_ssa_path(function, output, current, &[PlaceProjection::ErrorArm(false)], b, depth + 1)?
                        && selected.replace(output).is_some()
                    {
                        return Err(fail("original try subject selects several actual success projections"));
                    }
                }
            }
            let Some(output) = selected else {
                return Err(fail("original try subject loses its exact actual success projection"));
            };
            current = output;
        }
        Ok(Some(current))
    }

    pub(super) fn stored_pattern_value(
        &self,
        function: &TypedFunction,
        value: ValueId,
        b: &mut Budget,
    ) -> CompileResult<Option<LocalId>> {
        let mut result = None;
        for block in &function.blocks {
            for instruction in &block.instructions {
                b.spend(1, WORK)?;
                if let InstructionKind::Store {
                    local,
                    value: stored,
                } = instruction.kind
                    && stored == value
                {
                    if result.is_some_and(|old| old != local) {
                        return Err(unknown(
                            "pattern SSA is stored into several distinct actual heads",
                        ));
                    }
                    result = Some(local);
                }
            }
        }
        Ok(result)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn projected_shared_value(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        result: ValueId,
        parent: ValueId,
        selector: PlaceProjection,
        at: (BlockId, usize),
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<value::Value> {
        operations::available_before(function, (at.0, at.1, SemanticSiteEdge::Before), parent, b)?;
        let parent = self.ssa_value(instance, function, parent, b, depth + 1)?;
        self.projected_child(function, result, &parent, selector, b, depth)
    }

    pub(super) fn projected_child(
        &self,
        function: &TypedFunction,
        result: ValueId,
        parent: &value::Value,
        selector: PlaceProjection,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<value::Value> {
        let child = Self::pattern_child_type(&parent.ty, selector)
            .ok_or_else(|| fail("actual shared projection leaves its complete parent layout"))?;
        let output = function
            .values
            .get(result.0 as usize)
            .ok_or_else(|| fail("actual shared projection omits its complete result type"))?;
        if !geometry::same_type(child, output, b)? {
            return Err(fail(
                "actual shared projection changes its complete child type",
            ));
        }
        let storage = match &parent.ty {
            Type::BorrowedStorage { storage, .. } => storage.as_ref(),
            ty => ty,
        };
        if let PlaceProjection::Field(field) = selector
            && matches!(storage, Type::Record { .. } | Type::Package { .. })
        {
            // A stored child has its own complete intrinsic domain. Select it
            // from the declaration's full mixed application, preserving its
            // Pointee/phantom rows rather than borrowing the parent's head.
            let selected = self.actual_nominal_field(parent, field, b, depth + 1)?;
            if !geometry::same_type(&selected.ty, output, b)? {
                return Err(fail("actual nominal projection changes its complete original child"));
            }
            return Ok(selected);
        }
        if matches!(storage, Type::Tuple(_)) && !empty(&parent.life) {
            return self.closure_child_value(parent, selector, b);
        }
        if self.task_domain(child, b)?.is_some() {
            return self.task_projection(parent, child, selector, b);
        }
        let Type::Address {
            mutable: false,
            inner,
        } = child
        else {
            return Err(unknown(
                "actual projection needs its complete child class transport",
            ));
        };
        // Sealed shared projections retain one precise outer occurrence and
        // independently prove the complete pointee domain is zero. No tag,
        // access, live loan or initialized prefix is established by this map.
        let mut selected = None;
        for slot in &parent.life.slots {
            b.spend(slot.path.len().saturating_add(1), WORK)?;
            if slot.path.as_slice() == [selector] {
                if slot.mode != StoredBorrowMode::SharedAddress
                    || selected.replace(slot.binder).is_some()
                {
                    return Err(fail(
                        "actual shared projection has a competing mode or outer class",
                    ));
                }
            }
        }
        if selected.is_none() && original::strings::is_byte_type(child)
            && self.has_original_static_byte_path(&parent,
                &[ContractProjection::Place(selector)], b)?
        {
            crate::retained::check_dyn_static_return(self.module, function, result, &[], b)?;
            crate::dyn_metadata::ty(child, b)?;
            return Ok(value::scalar(child.clone()));
        }
        self.zero_data(inner, b, depth + 1)?;
        let binder = selected.ok_or_else(|| {
            fail("actual shared projection has no complete parent class occurrence")
        })?;
        let class = parent
            .classes
            .get(binder as usize)
            .cloned()
            .ok_or_else(|| fail("actual shared projection class leaves its parent domain"))?;
        crate::dyn_metadata::ty(inner, b)?;
        value::address(false, class, value::scalar(inner.as_ref().clone()), b)
    }

    pub(super) fn pattern_ssa_path(
        &self,
        function: &TypedFunction,
        value: ValueId,
        root: ValueId,
        path: &[PlaceProjection],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown("actual pattern SSA path exceeds its finite depth"));
        }
        let Some((last, prefix)) = path.split_last() else {
            return self.operation_operand_matches(function, value, root, b);
        };
        let (block, index, producer) = self.defining(function, value, b)?;
        let source = match (&producer.kind, last) {
            (
                InstructionKind::ReadVariant {
                    value,
                    variant,
                    index,
                },
                PlaceProjection::Variant(step),
            ) if *variant == step.variant && *index == step.index => *value,
            (
                InstructionKind::ReadErrorUnion { value, is_error }
                | InstructionKind::TakeErrorUnion { value, is_error },
                PlaceProjection::ErrorArm(error),
            ) if is_error == error => *value,
            (InstructionKind::ExtractField { aggregate, index }, PlaceProjection::Field(field))
                if index == field =>
            {
                *aggregate
            }
            _ => return Ok(false),
        };
        if !self.pattern_ssa_path(function, source, root, prefix, b, depth + 1)? {
            return Ok(false);
        }
        operations::available_before(
            function,
            (block, index, SemanticSiteEdge::Before),
            source,
            b,
        )?;
        let parent = function
            .values
            .get(source.0 as usize)
            .ok_or_else(|| fail("pattern SSA source has no complete type"))?;
        let child = Self::pattern_child_type(parent, *last)
            .ok_or_else(|| fail("pattern SSA path leaves its complete actual layout"))?;
        let output = function
            .values
            .get(value.0 as usize)
            .ok_or_else(|| fail("pattern SSA destination has no complete type"))?;
        if !geometry::same_type(child, output, b)? {
            return Err(fail(
                "pattern SSA extraction changes its complete child type",
            ));
        }
        Ok(true)
    }
    pub(super) fn pattern_child_type(ty: &Type, selector: PlaceProjection) -> Option<&Type> {
        let storage = match ty {
            Type::BorrowedStorage { storage, .. } => storage.as_ref(),
            ty => ty,
        };
        match (storage, selector) {
            (Type::Enum { variants, .. }, PlaceProjection::Variant(step)) => variants
                .get(step.variant as usize)
                .and_then(|(_, fields)| fields.get(step.index as usize)),
            (Type::ErrorUnion { error, result }, PlaceProjection::ErrorArm(is_error)) => {
                Some(if is_error {
                    error.as_ref()
                } else {
                    result.as_ref()
                })
            }
            (Type::Tuple(fields), PlaceProjection::Field(index)) => fields.get(index as usize),
            (
                Type::Record { fields, .. } | Type::Package { fields, .. },
                PlaceProjection::Field(index),
            ) => fields.get(index as usize).map(|(_, ty)| ty),
            _ => None,
        }
    }
    pub(super) fn pattern_local(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        ordinal: u32,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<LocalId> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(unknown("actual pattern origin exceeds its finite ancestry"));
        }
        let origin = self
            .original
            .pattern_origin(ordinal, instance.definition, b)?;
        b.spend(
            origin
                .subject
                .path
                .len()
                .saturating_add(origin.path.len() + 1),
            WORK,
        )?;
        let mut path = Vec::new();
        path.try_reserve(origin.subject.path.len().saturating_add(origin.path.len()))
            .map_err(|_| fail("actual pattern path allocation failed"))?;
        for field in &origin.subject.path {
            path.push(PlaceProjection::Field(*field));
        }
        for step in &origin.path {
            path.push(*step);
        }
        let anchored = origin
            .subject
            .anchor
            .map(|anchor| self.anchored_local_at(instance, function, anchor, b, depth + 1))
            .transpose()?;
        let emitted = self.projected_pattern_subject(instance, function, origin.subject.node,
            origin.subject.success_projections, b, depth + 1)?;
        let root = if let Some(value) = emitted {
            let (_, _, producer) = self.defining(function, value, b)?;
            if let Some(anchor) = anchored {
                // A source name/field still has to read the original root;
                // a forged same-typed SSA cannot provide another subject.
                let valid = match &producer.kind {
                    InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                        origin.subject.path.is_empty() && *local == anchor
                    }
                    InstructionKind::TakePackageField { local, index }
                    | InstructionKind::ReadPackageField { local, index } => {
                        *local == anchor && origin.subject.path.as_slice() == [*index]
                    }
                    InstructionKind::TakeTupleField {
                        local,
                        path: selected,
                    }
                    | InstructionKind::TakeVariantField {
                        local,
                        path: selected,
                    } => {
                        *local == anchor
                            && selected.iter().copied().eq(origin
                                .subject
                                .path
                                .iter()
                                .copied()
                                .map(PlaceProjection::Field))
                    }
                    _ => false,
                };
                if !valid {
                    return Err(fail(
                        "pattern subject reads another original root or field path",
                    ));
                }
            }
            // A borrowed/Copy pattern can select an SSA subject directly.
            // Match its exact original emission and complete selector chain;
            // no whole-subject temporary or same-typed head is invented.
            let mut direct = None;
            for block in &function.blocks {
                for (index, instruction) in block.instructions.iter().enumerate() {
                    b.spend(1, WORK)?;
                    let InstructionKind::Store {
                        local,
                        value: child,
                    } = instruction.kind
                    else {
                        continue;
                    };
                    if self.pattern_ssa_path(function, child, value, &origin.path, b, depth + 1)? {
                        operations::available_before(
                            function,
                            (block.id, index, SemanticSiteEdge::Before),
                            child,
                            b,
                        )?;
                        let destination = function
                            .locals
                            .get(local.0 as usize)
                            .filter(|item| item.id == local)
                            .ok_or_else(|| {
                                fail("pattern SSA store has no exact destination local")
                            })?;
                        let source = function
                            .values
                            .get(child.0 as usize)
                            .ok_or_else(|| fail("pattern SSA store has no exact selected value"))?;
                        if !geometry::same_type(&destination.ty, source, b)? {
                            return Err(fail(
                                "pattern SSA store changes its complete binding type",
                            ));
                        }
                        if direct.is_some_and(|old| old != local) {
                            return Err(unknown(
                                "original pattern SSA has several distinct destination heads",
                            ));
                        }
                        direct = Some(local);
                    }
                }
            }
            if let Some(local) = direct {
                return Ok(local);
            }
            if let Some(local) = self.stored_pattern_value(function, value, b)? {
                // A stored field-value is a new whole temporary. Its source
                // field prefix has already been checked above.
                if !origin.subject.path.is_empty() {
                    b.spend(origin.subject.path.len(), WORK)?;
                    drop(path.drain(..origin.subject.path.len()));
                }
                local
            } else {
                anchored
                    .ok_or_else(|| unknown("pattern value lacks its actual storage temporary"))?
            }
        } else {
            anchored.ok_or_else(|| unknown("pattern subject has no actual root or emission")
                .into_iter().map(|diagnostic| diagnostic.with_note(format!(
                    "Original {} expression {:?}, pattern path {:?}, actual function {:?}",
                    function.name, origin.subject.node, origin.path, function.id,
                ))).collect::<Vec<_>>())?
        };
        let mut selected = None;
        let mut accept = |local| -> CompileResult<()> {
            if selected.is_some_and(|old| old != local) {
                return Err(unknown(
                    "original pattern has several distinct extraction destinations",
                ));
            }
            selected = Some(local);
            Ok(())
        };
        if path.is_empty()
            && let Some(value) = emitted
            && let Some(local) = self.stored_pattern_value(function, value, b)?
        {
            accept(local)?;
        }
        for block in &function.blocks {
            for instruction in &block.instructions {
                b.spend(1, WORK)?;
                match &instruction.kind {
                    InstructionKind::BorrowPlaceLocal { local, path: actual, mutable }
                        if *local == root && origin.borrow.is_some_and(|borrow| borrow.mutable == *mutable) => {
                        b.spend(actual.len().saturating_add(path.len() + 1), WORK)?;
                        if *actual == path {
                            let value = instruction.result.ok_or_else(|| fail("actual pattern borrow has no SSA result"))?;
                            if let Some(local) = self.stored_pattern_value(function, value, b)? {
                                accept(local)?;
                            }
                        }
                    }
                    InstructionKind::BorrowLocal { local } | InstructionKind::BorrowLocalMut { local }
                        if *local == root && path.is_empty() && origin.borrow.is_some_and(|borrow|
                            borrow.mutable == matches!(instruction.kind, InstructionKind::BorrowLocalMut { .. })) => {
                        let value = instruction.result.ok_or_else(|| fail("actual pattern borrow has no SSA result"))?;
                        if let Some(local) = self.stored_pattern_value(function, value, b)? {
                            accept(local)?;
                        }
                    }
                    InstructionKind::TakeVariantField {
                        local,
                        path: actual,
                    }
                    | InstructionKind::TakeTupleField {
                        local,
                        path: actual,
                    } if *local == root && origin.borrow.is_none() => {
                        b.spend(actual.len().saturating_add(path.len() + 1), WORK)?;
                        if *actual == path {
                            let value = instruction.result.ok_or_else(|| {
                                fail("actual pattern extraction has no SSA result")
                            })?;
                            if let Some(local) = self.stored_pattern_value(function, value, b)? {
                                accept(local)?;
                            }
                        }
                    }
                    InstructionKind::UnpackTuple { local, fields } if *local == root && origin.borrow.is_none() => {
                        if let [PlaceProjection::Field(index)] = path.as_slice()
                            && let Some(local) = fields.get(*index as usize)
                        {
                            b.spend(fields.len(), WORK)?;
                            accept(*local)?;
                        }
                    }
                    InstructionKind::UnpackVariant {
                        local,
                        variant,
                        fields,
                    } if *local == root && origin.borrow.is_none() => {
                        let index = match path.as_slice() {
                            [PlaceProjection::Variant(step)] if step.variant == *variant => {
                                Some(step.index)
                            }
                            [PlaceProjection::ErrorArm(error)] if u32::from(*error) == *variant => {
                                Some(0)
                            }
                            _ => None,
                        };
                        if let Some(index) = index
                            && let Some(local) = fields.get(index as usize)
                        {
                            b.spend(fields.len(), WORK)?;
                            accept(*local)?;
                        }
                    }
                    _ => {}
                }
            }
        }
        selected.ok_or_else(|| {
            unknown("pattern binding lacks its exact actual selection and destination store")
        })
    }
}
