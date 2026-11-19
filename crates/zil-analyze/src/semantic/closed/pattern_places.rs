//! Bind original pattern names to actual storage through exact subject SSA,
//! tag/field selection and destination Store operations. Names and equal
//! physical types do not choose a destination.
use super::*;
use zil_base::{NodeId, Span};
use crate::semantic::value::Class;

impl Check<'_, '_> {
    fn pattern_address_projection(
        &self, instance: &SemanticInstance, function: &TypedFunction,
        origin: &original::pattern_places::PatternOrigin, mut address: ValueId, b: &mut Budget,
    ) -> CompileResult<bool> {
        let mut path = origin.path.as_slice();
        while let Some((PlaceProjection::Field(index), parents)) = path.split_last() {
            b.spend(1, WORK)?;
            let (_, _, producer) = self.defining(function, address, b)?;
            let InstructionKind::AddressField { address: parent, index: actual } = producer.kind else {
                return Ok(false);
            };
            if *index != actual { return Ok(false); }
            address = parent;
            path = parents;
        }
        if origin.subject.indirect {
            if !path.is_empty() { return Ok(false); }
            let root = self.original_subject_value(instance, function, origin.subject.node, b)?
                .ok_or_else(|| fail("borrowed pattern lacks the actual original input address"))?;
            return self.operation_operand_matches(function, address, root, b);
        }
        let (_, _, producer) = self.defining(function, address, b)?;
        let InstructionKind::BorrowVariantLocal { local, path: actual, .. } = &producer.kind else {
            return Ok(false);
        };
        b.spend(path.len().saturating_add(actual.len()), WORK)?;
        if !origin.subject.path.is_empty()
            || actual.iter().copied().map(PlaceProjection::Variant).ne(path.iter().copied())
        {
            return Ok(false);
        }
        let anchor = origin.subject.anchor.ok_or_else(|| fail("borrowed pattern has no original variant owner"))?;
        Ok(self.anchored_local_at(instance, function, anchor, b, 0)? == *local)
    }

    pub(super) fn original_pattern_reborrow_class(
        &self, instance: &SemanticInstance, function: &TypedFunction,
        address: ValueId, mutable: bool, span: Span, b: &mut Budget,
    ) -> CompileResult<Option<Class>> {
        let Some(origin) = self.original.indirect_pattern_borrow(instance.definition, span, b)? else {
            return Ok(None);
        };
        let borrow = origin.borrow.ok_or_else(|| fail("original reborrow loses its pattern occurrence"))?;
        if borrow.mutable != mutable || !self.pattern_address_projection(instance, function, origin, address, b)? {
            return Err(fail("actual pattern reborrow changes its original input address, field, or mode"));
        }
        Ok(Some(Class::Implicit(instance.id, borrow.ordinal)))
    }

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
        for (ordinal, origin) in self.original.pattern_origins().iter().enumerate() {
            b.spend(1, WORK)?;
            if origin.record {
                self.check_record_pattern_binding(ordinal, origin, b)?;
            }
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

    fn check_record_pattern_binding(
        &self, ordinal: usize, origin: &original::pattern_places::PatternOrigin, b: &mut Budget,
    ) -> CompileResult<()> {
        let ordinal = u32::try_from(ordinal).map_err(|_| fail("record binder occurrence ordinal overflow"))?;
        let (span, name) = origin.binding.as_ref()
            .ok_or_else(|| fail("record pattern path lacks its original binding name"))?;
        for instance in &self.program.instances {
            b.spend(1, WORK)?;
            if instance.definition != origin.owner { continue; }
            let SemanticInstanceTarget::Function(target) = &instance.target else { continue; };
            let function = self.inventory.function(target, b)?;
            let local = self.pattern_local(instance, function, ordinal, b, 0)?;
            let actual = function.locals.get(local.0 as usize)
                .filter(|declaration| declaration.id == local)
                .ok_or_else(|| fail("record pattern lacks its actual binding declaration"))?;
            b.spend(name.len().saturating_add(1), WORK)?;
            if actual.span != *span || actual.name != *name {
                return Err(fail("actual record pattern field initializes another original binder"));
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

    // The source binds a Copy payload. The compiler reads it through the
    // selected shared address while the actual Sum kernel checks its tag.
    fn copied_pattern_local(
        &self, function: &TypedFunction, local: LocalId, path: &[PlaceProjection],
        address: ValueId, b: &mut Budget,
    ) -> CompileResult<Option<LocalId>> {
        let mut child = &function.locals.get(local.0 as usize)
            .filter(|entry| entry.id == local)
            .ok_or_else(|| fail("actual Copy binding has no whole pattern local"))?.ty;
        for selector in path {
            b.spend(1, WORK)?;
            child = Self::pattern_child_type(child, *selector)
                .ok_or_else(|| fail("actual Copy binding leaves its complete original pattern layout"))?;
        }
        let Some(Type::Address { mutable: false, inner }) = function.values.get(address.0 as usize) else {
            return Err(fail("actual Copy pattern projection lacks its shared address result"));
        };
        if !geometry::same_type(child, inner, b)? {
            return Err(fail("actual Copy pattern projection changes its exact Copy payload"));
        }
        if !child.is_copy_value() {
            return Ok(None);
        }
        let mut destination = None;
        for block in &function.blocks {
            for (index, instruction) in block.instructions.iter().enumerate() {
                b.spend(1, WORK)?;
                if !matches!(instruction.kind, InstructionKind::ReadAddress { address: source } if source == address) {
                    continue;
                }
                operations::available_before(function, (block.id, index, SemanticSiteEdge::Before), address, b)?;
                let result = instruction.result.ok_or_else(|| fail("actual Copy pattern read has no value result"))?;
                let output = function.values.get(result.0 as usize)
                    .ok_or_else(|| fail("actual Copy pattern read result has no complete type"))?;
                if !geometry::same_type(child, output, b)? {
                    return Err(fail("actual Copy pattern read changes its complete payload"));
                }
                if let Some(local) = self.stored_pattern_value(function, result, b)? {
                    let binding = function.locals.get(local.0 as usize)
                        .filter(|entry| entry.id == local)
                        .ok_or_else(|| fail("actual Copy pattern store has no binding local"))?;
                    if !geometry::same_type(child, &binding.ty, b)? {
                        return Err(fail("actual Copy pattern store changes its binding type"));
                    }
                    if destination.is_some_and(|previous| previous != local) {
                        return Err(fail("actual Copy pattern payload has several binding destinations"));
                    }
                    destination = Some(local);
                }
            }
        }
        Ok(destination)
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
        if self.task_domain(child, b)?.is_some()
            || selector == PlaceProjection::ErrorArm(true)
                && value::task_failure_domain(child, b)?.is_some()
        {
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
            (Type::Array { element, length }, PlaceProjection::Field(index)) if index < *length => Some(element),
            (
                Type::Record { fields, .. } | Type::Package { fields, .. },
                PlaceProjection::Field(index),
            ) => fields.get(index as usize).map(|(_, ty)| ty),
            _ => None,
        }
    }
    fn pattern_subject_root(
        &self,
        instance: &SemanticInstance,
        function: &TypedFunction,
        node: zil_base::NodeId,
        value: ValueId,
        anchor: LocalId,
        path: &[u32],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        if depth > b.partial_depth {
            return Err(unknown(
                "original pattern subject exceeds its finite field depth",
            ));
        }
        let (block, index, producer) = self.defining(function, value, b)?;
        let direct = match &producer.kind {
            InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                path.is_empty() && *local == anchor
            }
            InstructionKind::TakePackageField { local, index }
            | InstructionKind::ReadPackageField { local, index } => {
                *local == anchor && path == [*index]
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
                    && selected
                        .iter()
                        .copied()
                        .eq(path.iter().copied().map(PlaceProjection::Field))
            }
            _ => false,
        };
        if direct {
            return Ok(true);
        }
        let (local, field) = match producer.kind {
            InstructionKind::TakePackageField { local, index }
            | InstructionKind::ReadPackageField { local, index } => (local, index),
            _ => return Ok(false),
        };
        let Some((&last, prefix)) = path.split_last() else {
            return Ok(false);
        };
        if last != field {
            return Ok(false);
        }

        // A consuming field access can move its original receiver into a
        // temporary first. Reconstruct each actual receiver at its exact read.
        let original = self.inventory.original_index(instance.definition, b)?;
        let CoreExprKind::Member { object, .. } = &original.get_expr(node, b)?.kind else {
            return Ok(false);
        };
        let parent = match self.original_subject_value(instance, function, object.id, b)? {
            Some(value) => value,
            None if prefix.is_empty() && matches!(object.kind, CoreExprKind::Name(_)) => {
                // A plain moved name need not have a separate semantic site.
                // Its latest actual use here must consume this very binding.
                let mut moved = None;
                for instruction in function.blocks[block.0 as usize].instructions[..index].iter().rev() {
                    b.spend(1, WORK)?;
                    if let InstructionKind::TakeLocal { local } = instruction.kind
                        && local == anchor
                    {
                        moved = instruction.result;
                        break;
                    }
                    let touches = match crate::incremental::Operation::of(&instruction.kind) {
                        crate::incremental::Operation::Read(local)
                        | crate::incremental::Operation::Store(local)
                        | crate::incremental::Operation::Take(local)
                        | crate::incremental::Operation::Kill(local) => local == anchor,
                        crate::incremental::Operation::Unpack { source, fields } => {
                            b.spend(fields.len(), WORK)?;
                            source == anchor || fields.contains(&anchor)
                        }
                        crate::incremental::Operation::Other => false,
                    };
                    if touches { break; }
                }
                let Some(value) = moved else { return Ok(false) };
                value
            }
            None => return Ok(false),
        };
        let actual = function
            .locals
            .get(local.0 as usize)
            .filter(|entry| entry.id == local)
            .ok_or_else(|| fail("pattern field has no actual receiver local"))?;
        let ty = function
            .values
            .get(parent.0 as usize)
            .ok_or_else(|| fail("pattern receiver has no emitted SSA type"))?;
        if !geometry::same_type(&actual.ty, ty, b)?
            || !self.local_holds_operand_at(function, local, block, index, parent, b)?
        {
            return Ok(false);
        }
        self.pattern_subject_root(
            instance,
            function,
            object.id,
            parent,
            anchor,
            prefix,
            b,
            depth + 1,
        )
    }

    /// An unpack can initialize the lexical binding directly or first place an
    /// affine field in a staging local. Follow only the actual transfer from
    /// this unpack to the original declaration in the same activation.
    fn unpack_binding_local(
        &self, function: &TypedFunction, block: &BasicBlock, unpack: usize, field: LocalId,
        binding: Option<&(Span, String)>, b: &mut Budget,
    ) -> CompileResult<LocalId> {
        let Some((span, name)) = binding else { return Ok(field); };
        let local = function.locals.get(field.0 as usize)
            .filter(|row| row.id == field).ok_or_else(|| fail("unpack field has no actual local"))?;
        b.spend(local.name.len().saturating_add(name.len()), WORK)?;
        if local.span == *span && local.name == *name { return Ok(field); }
        let mut moved = None;
        for (at, instruction) in block.instructions.iter().enumerate().skip(unpack + 1) {
            b.spend(1, WORK)?;
            if let Some(source) = moved {
                if let InstructionKind::Store { local: destination, value } = instruction.kind
                    && value == source
                {
                    let dest = function.locals.get(destination.0 as usize)
                        .filter(|row| row.id == destination).ok_or_else(|| fail("unpack transfer has no destination"))?;
                    b.spend(dest.name.len().saturating_add(name.len()), WORK)?;
                    if dest.name != *name || dest.span != *span || instruction.span != *span
                        || !matches!(block.instructions[at-1].kind, InstructionKind::StorageLive(actual) if actual == destination)
                        || !geometry::same_type(&local.ty, &dest.ty, b)?
                    { return Err(fail("unpacked value is transferred to another original declaration")); }
                    operations::available_before(function, (block.id, at, SemanticSiteEdge::Before), source, b)?;
                    return Ok(destination);
                }
            } else {
                if matches!(instruction.kind, InstructionKind::TakeLocal { local } if local == field)
                    && instruction.span == *span
                {
                    moved = Some(instruction.result.ok_or_else(|| fail("unpack transfer has no SSA result"))?);
                    continue;
                }
                self.head_projection_barrier(function, field, &[], &instruction.kind, b)?;
                match crate::incremental::Operation::of(&instruction.kind) {
                    crate::incremental::Operation::Read(id) | crate::incremental::Operation::Store(id)
                    | crate::incremental::Operation::Take(id) | crate::incremental::Operation::Kill(id)
                        if id == field => break,
                    crate::incremental::Operation::Unpack { source, fields } => {
                        b.spend(fields.len(), WORK)?;
                        if source == field || fields.contains(&field) { break; }
                    }
                    _ => {}
                }
            }
        }
        Ok(field)
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
        let variant_record = origin.record && matches!(origin.path.first(), Some(PlaceProjection::Variant(_)));
        if origin.subject.indirect || variant_record {
            let (span, name) = origin.binding.as_ref().ok_or_else(|| fail("indirect pattern has no original binder"))?;
            let mut selected = None;
            for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
                b.spend(1, WORK)?;
                let InstructionKind::Store { local, value } = instruction.kind else { continue; };
                let declaration = function.locals.get(local.0 as usize)
                    .ok_or_else(|| fail("indirect pattern store has no actual local"))?;
                if declaration.span != *span { continue; }
                b.spend(name.len(), WORK)?;
                if declaration.name != *name { continue; }
                let (_, _, producer) = self.defining(function, value, b)?;
                let address = match (&producer.kind, origin.borrow) {
                    (InstructionKind::ReborrowAddress { address, mutable }, Some(borrow))
                        if *mutable == borrow.mutable && producer.span == borrow.span => *address,
                    (InstructionKind::ReadAddress { address }, None) => *address,
                    _ if !origin.subject.indirect => continue,
                    _ => return Err(fail("actual pattern binding changes its original read or borrow")),
                };
                if !self.pattern_address_projection(instance, function, origin, address, b)?
                    || selected.replace(local).is_some()
                {
                    return Err(fail("indirect pattern binding changes its exact original field selection"));
                }
            }
            if origin.subject.indirect || selected.is_some() {
                return selected.ok_or_else(|| fail("indirect pattern lacks its actual binding transfer"));
            }
        }
        if let Some(range) = self.original.range_source(ordinal, b)? {
            return self.range_binding_local(instance, function, range.statement, range.start, range.end, depth, b);
        }
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
        let mut emitted = self.projected_pattern_subject(instance, function, origin.subject.node,
            origin.subject.success_projections, b, depth + 1)?;
        if emitted.is_none() && origin.record && origin.borrow.is_none()
            && let Some(root) = anchored
        {
            let original = self.inventory.original_index(instance.definition, b)?;
            let span = original.get_expr(origin.subject.node, b)?.span;
            for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
                b.spend(1, WORK)?;
                if instruction.span == span && let Some(value) = instruction.result
                    && self.pattern_subject_root(instance, function, origin.subject.node, value,
                        root, &origin.subject.path, b, depth + 1)?
                    && emitted.replace(value).is_some()
                {
                    return Err(fail("record pattern has several actual observations of its original subject"));
                }
            }
        }
        let root = if let Some(value) = emitted {
            if let Some(anchor) = anchored {
                let valid = self.pattern_subject_root(instance, function, origin.subject.node, value,
                    anchor, &origin.subject.path, b, depth + 1)?;
                if !valid {
                    let (block, at, producer) = self.defining(function, value, b)?;
                    b.spend(origin.subject.path.len().saturating_mul(12).saturating_add(128), WORK)?;
                    let mut errors = fail("pattern subject reads another original root or field path");
                    errors[0].span = Some(producer.span);
                    errors[0].notes.push(format!(
                        "Original expression {} requires local {} with fields {:?}. Actual SSA {} is produced in block {} at instruction {}.",
                        origin.subject.node.0, anchor.0, origin.subject.path, value.0, block.0, at));
                    return Err(errors);
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
                    if origin.record && let Some((span, name)) = &origin.binding {
                        let destination = function.locals.get(local.0 as usize)
                            .filter(|item| item.id == local)
                            .ok_or_else(|| fail("record pattern store has no actual destination"))?;
                        if destination.span != *span { continue; }
                        b.spend(name.len(), WORK)?;
                        if destination.name != *name { continue; }
                    }
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
        let mut accept = |local: LocalId, b: &mut Budget| -> CompileResult<()> {
            if origin.record
                && let Some((span, name)) = &origin.binding
            {
                let actual = function.locals.get(local.0 as usize)
                    .ok_or_else(|| fail("named pattern has no actual destination local"))?;
                b.spend(name.len(), WORK)?;
                if actual.span != *span || actual.name != *name {
                    return Ok(());
                }
            }
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
            accept(local, b)?;
        }
        for block in &function.blocks {
            for (at, instruction) in block.instructions.iter().enumerate() {
                b.spend(1, WORK)?;
                match &instruction.kind {
                    InstructionKind::BorrowFieldLocal { local, path: actual, mutable }
                        if *local == root && origin.borrow.map_or(!*mutable, |borrow|
                            borrow.mutable == *mutable && borrow.span == instruction.span) =>
                    {
                        b.spend(actual.len().saturating_add(path.len() + 1), WORK)?;
                        if actual.iter().copied().map(PlaceProjection::Field).eq(path.iter().copied()) {
                            let value = instruction.result.ok_or_else(|| fail("actual field borrow has no SSA result"))?;
                            let local = if origin.borrow.is_some() {
                                self.stored_pattern_value(function, value, b)?
                            } else {
                                self.copied_pattern_local(function, root, &path, value, b)?
                            };
                            if let Some(local) = local { accept(local, b)?; }
                        }
                    }
                    InstructionKind::BorrowPlaceLocal { local, path: actual, mutable: false }
                        if *local == root && origin.borrow.is_none() => {
                        b.spend(actual.len().saturating_add(path.len() + 1), WORK)?;
                        if *actual == path {
                            let address = instruction.result.ok_or_else(|| fail("actual Copy pattern avoids its shared address result"))?;
                            if let Some(local) = self.copied_pattern_local(function, root, &path, address, b)? {
                                accept(local, b)?;
                            }
                        }
                    }
                    InstructionKind::BorrowPlaceLocal { local, path: actual, mutable }
                        if *local == root && origin.borrow.is_some_and(|borrow| borrow.mutable == *mutable) => {
                        b.spend(actual.len().saturating_add(path.len() + 1), WORK)?;
                        if *actual == path {
                            let value = instruction.result.ok_or_else(|| fail("actual pattern borrow has no SSA result"))?;
                            if let Some(local) = self.stored_pattern_value(function, value, b)? {
                                accept(local, b)?;
                            }
                        }
                    }
                    InstructionKind::BorrowLocal { local } | InstructionKind::BorrowLocalMut { local }
                        if *local == root && path.is_empty() && origin.borrow.is_some_and(|borrow|
                            borrow.mutable == matches!(instruction.kind, InstructionKind::BorrowLocalMut { .. })) => {
                        let value = instruction.result.ok_or_else(|| fail("actual pattern borrow has no SSA result"))?;
                        if let Some(local) = self.stored_pattern_value(function, value, b)? {
                            accept(local, b)?;
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
                                accept(local, b)?;
                            }
                        }
                    }
                    InstructionKind::UnpackTuple { local, fields } if *local == root && origin.borrow.is_none() => {
                        if let [PlaceProjection::Field(index)] = path.as_slice()
                            && let Some(local) = fields.get(*index as usize)
                        {
                            b.spend(fields.len(), WORK)?;
                            accept(self.unpack_binding_local(function, block, at, *local, origin.binding.as_ref(), b)?, b)?;
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
                            accept(self.unpack_binding_local(function, block, at, *local, origin.binding.as_ref(), b)?, b)?;
                        }
                    }
                    _ => {}
                }
            }
        }
        selected.ok_or_else(|| {
            if origin.record {
                fail("record pattern binding loses its exact original field selection")
            } else {
                unknown("pattern binding lacks its exact actual selection and destination store")
            }
        })
    }
}
