//! Interpret a complete source write place before its RHS. Observing a field
//! may peel an owning/address wrapper; writing additionally needs the actual
//! root's mutability and each traversed reference's exclusive qualification.
use super::*;

struct WritePlace {
    selected: Term,
    local: Option<usize>,
    writable: bool,
    description: bool,
    projected: bool,
    path: Vec<u32>,
    indirect: bool,
}
impl Solver<'_, '_> {
    pub(super) fn copy_vacancies(
        &self,
        paths: &BTreeSet<Vec<u32>>,
        b: &mut Budget,
    ) -> CompileResult<BTreeSet<Vec<u32>>> {
        let mut output = BTreeSet::new();
        for path in paths {
            b.spend(path.len().saturating_add(output.len() + 1), WORK)?;
            output.insert(path.clone());
        }
        Ok(output)
    }
    pub(super) fn local_available(
        &self,
        local: &Local,
        path: &[u32],
        b: &mut Budget,
    ) -> CompileResult<()> {
        for hole in &local.maybe_vacant {
            b.spend(path.len().saturating_add(hole.len() + 1), WORK)?;
            if path.starts_with(hole) || hole.starts_with(path) {
                return Err(fail(
                    "original selected place overlaps a consumed field that has not been restored",
                ));
            }
        }
        Ok(())
    }
    fn availability_local(
        &self,
        frame: &Frame,
        anchor: Option<LocalAnchor>,
        b: &mut Budget,
    ) -> CompileResult<Option<usize>> {
        let Some(anchor) = anchor else {
            return Ok(None);
        };
        if let LocalAnchor::Environment(ordinal) = anchor {
            let definition = self.inventory.definition(frame.definition, b)?;
            let source = definition
                .environment
                .get(ordinal as usize)
                .ok_or_else(|| fail("original availability environment ordinal is absent"))?;
            let SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType {
                source: SemanticOriginalTypeSource::NameAt { name, .. },
                ..
            }) = &source.source
            else {
                return Err(fail(
                    "original availability environment has no lexical NameAt source",
                ));
            };
            for (index, local) in frame.locals[..frame.inherited].iter().enumerate().rev() {
                b.spend(local.name.len().saturating_add(name.len() + 1), WORK)?;
                if local.name == *name {
                    return Ok(Some(index));
                }
            }
            return Err(fail(
                "original availability environment loses its original lexical binding",
            ));
        }
        let mut selected = None;
        for (index, local) in frame.locals.iter().enumerate() {
            b.spend(1, WORK)?;
            let actual = local.anchor;
            if actual == Some(anchor) && selected.replace(index).is_some() {
                return Err(fail(
                    "original place has competing lexical availability roots",
                ));
            }
        }
        selected
            .ok_or_else(|| fail("original place loses its lexical availability root"))
            .map(Some)
    }
    pub(super) fn place_available(
        &self,
        frame: &Frame,
        anchor: Option<LocalAnchor>,
        path: &[u32],
        indirect: bool,
        b: &mut Budget,
    ) -> CompileResult<()> {
        if !frame.alive {
            return Ok(());
        }
        if let Some(index) = self.availability_local(frame, anchor, b)? {
            self.local_available(&frame.locals[index], if indirect { &[] } else { path }, b)?;
        }
        Ok(())
    }
    pub(super) fn consume_place(
        &mut self,
        frame: &mut Frame,
        place: &borrow_places::Place,
        b: &mut Budget,
    ) -> CompileResult<()> {
        self.place_available(frame, place.anchor, &place.path, place.indirect, b)?;
        if !frame.alive || self.satisfies(place.inner, Predicate::Copy, b, 0)? {
            return Ok(());
        }
        if place.indirect || place.index.is_some() {
            return Err(fail(
                "original consuming entry cannot take through an indirect or indexed place",
            ));
        }
        self.retire_lexical_place(frame, place.anchor, &place.path, b)
    }
    pub(super) fn retire_lexical_place(
        &mut self,
        frame: &mut Frame,
        anchor: Option<LocalAnchor>,
        path: &[u32],
        b: &mut Budget,
    ) -> CompileResult<()> {
        let index = self
            .availability_local(frame, anchor, b)?
            .ok_or_else(|| fail("original consuming entry has no exact owning place"))?;
        if frame.locals[index].borrowed {
            return Err(fail(
                "original consuming entry cannot take from a borrowed lexical owner",
            ));
        }
        if path.is_empty() {
            frame.locals[index].used = true;
            frame.locals[index].live = false;
        } else {
            b.spend(
                path.len()
                    .saturating_add(frame.locals[index].maybe_vacant.len() + 1),
                WORK,
            )?;
            frame.locals[index].vacant.insert(path.to_vec());
            frame.locals[index].maybe_vacant.insert(path.to_vec());
        }
        Ok(())
    }
    pub(super) fn join_vacancies(
        &self,
        to: &mut Local,
        from: &Local,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let mut certain = BTreeSet::new();
        if !to.live {
            certain = self.copy_vacancies(&from.vacant, b)?;
        } else if !from.live {
            certain = self.copy_vacancies(&to.vacant, b)?;
        } else {
            for left in &to.vacant {
                for right in &from.vacant {
                    b.spend(left.len().saturating_add(right.len() + 1), WORK)?;
                    if left.starts_with(right) {
                        certain.insert(left.clone());
                    } else if right.starts_with(left) {
                        certain.insert(right.clone());
                    }
                }
            }
        }
        for path in &from.maybe_vacant {
            b.spend(path.len().saturating_add(to.maybe_vacant.len() + 1), WORK)?;
            to.maybe_vacant.insert(path.clone());
        }
        to.vacant = certain;
        to.live |= from.live;
        Ok(())
    }
    pub(super) fn cleanup_partial(
        &mut self,
        term: Term,
        path: &[u32],
        vacant: &BTreeSet<Vec<u32>>,
        origin: &cleanup::Origin,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        let mut partial = false;
        for hole in vacant {
            b.spend(path.len().saturating_add(hole.len() + 1), WORK)?;
            if path.starts_with(hole) {
                return Ok(());
            }
            partial |= hole.starts_with(path);
        }
        if !partial {
            return self.record_cleanup(term, origin, path, b);
        }
        let fields = match &self.state.arena[self.type_root(term, b)?] {
            Kind::Tuple(fields) => {
                b.spend(fields.len(), WORK)?;
                fields.clone()
            }
            Kind::Nominal {
                definition, fields, ..
            } => {
                let declaration = self.inventory.definition(*definition, b)?;
                if !matches!(
                    declaration.body,
                    SemanticDefinitionBody::Nominal {
                        kind: LifeNominalKind::Record | LifeNominalKind::Package,
                        body: SemanticNominalBody::Product { .. },
                        ..
                    }
                ) {
                    return Err(fail("original partial cleanup has no plain product owner"));
                }
                resources::ordinary_drop_absent(*definition, self.inventory, b)?;
                b.spend(fields.len(), WORK)?;
                fields.clone()
            }
            _ => {
                return Err(fail(
                    "original partial cleanup loses its complete product owner",
                ));
            }
        };
        for (ordinal, field) in fields.iter().enumerate() {
            let mut child = path.to_vec();
            Self::push(
                &mut child,
                u32::try_from(ordinal)
                    .map_err(|_| resource("original cleanup field ordinal overflow"))?,
                b,
            )?;
            self.cleanup_partial(*field, &child, vacant, origin, b, depth + 1)?;
        }
        Ok(())
    }
    pub(super) fn cleanup_reintroduced(
        &mut self,
        frame: &Frame,
        statement: NodeId,
        index: usize,
        old: &Local,
        next: &Local,
        b: &mut Budget,
    ) -> CompileResult<()> {
        // Inherited lexical entries include names which this closure never
        // captures. An unchanged live place has no backedge cleanup debt and
        // must not be asked for a nonexistent environment ordinal.
        if !next.live || (old.live && old.vacant.is_empty()) {
            return Ok(());
        }
        let anchor = if index < frame.inherited { Some(self.environment_anchor(frame, &old.name, b)?) }
            else { old.anchor };
        let origin = cleanup::Origin::local(frame.definition, anchor, cleanup::Event::Backedge(statement));
        if !old.live && next.live {
            return self.cleanup_partial(old.ty, &[], &next.vacant, &origin, b, 0);
        }
        // The body was checked assuming these fields empty. If a back edge
        // makes any live again, their next overwrite must independently drop
        // them; the first iteration's vacancy cannot waive that obligation.
        for path in &old.vacant {
            let mut term = old.ty;
            for ordinal in path {
                b.spend(1, WORK)?;
                term = match &self.state.arena[self.type_root(term, b)?] {
                    Kind::Tuple(fields) | Kind::Nominal { fields, .. } => *fields
                        .get(*ordinal as usize)
                        .ok_or_else(|| fail("original loop vacancy field is absent"))?,
                    _ => return Err(fail("original loop vacancy has no exact product path")),
                };
            }
            self.cleanup_partial(term, path, &next.vacant, &origin, b, 0)?;
        }
        Ok(())
    }
    fn write_record(
        &self,
        frame: &mut Frame,
        node: NodeId,
        term: Term,
        b: &mut Budget,
    ) -> CompileResult<()> {
        b.lookup(frame.expressions.len(), WORK)?;
        if frame.expressions.insert(node, term).is_some() {
            return Err(fail(
                "original write place reuses a complete source expression node",
            ));
        }
        Ok(())
    }
    fn write_local(
        &mut self,
        frame: &mut Frame,
        name: &str,
        node: NodeId,
        initialized: bool,
        b: &mut Budget,
    ) -> CompileResult<Option<WritePlace>> {
        let mut selected = None;
        for (index, local) in frame.locals.iter().enumerate().rev() {
            b.spend(local.name.len().saturating_add(name.len() + 1), WORK)?;
            if local.name == name {
                selected = Some(index);
                break;
            }
        }
        let Some(index) = selected else {
            return Ok(None);
        };
        let local = &frame.locals[index];
        if initialized && !local.initialized {
            return Err(fail(
                "original projected write has no initialized root occupant",
            ));
        }
        let (selected, writable, used) = (local.ty, local.mutable, local.used);
        if initialized && used {
            Self::push(
                &mut self.needs,
                Need {
                    term: selected,
                    predicate: Predicate::Copy,
                },
                b,
            )?;
        }
        if initialized {
            self.record_environment_read(frame, index, node, b)?;
        }
        if index < frame.inherited {
            b.lookup(frame.captured.len(), WORK)?;
            frame.captured.insert(index, selected);
        }
        self.write_record(frame, node, selected, b)?;
        Ok(Some(WritePlace {
            selected,
            local: Some(index),
            writable,
            description: false,
            projected: false,
            path: Vec::new(),
            indirect: false,
        }))
    }
    fn write_object(
        &mut self,
        frame: &mut Frame,
        object: &SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<WritePlace> {
        if let Some(place) = self.write_place(frame, object, true, b, depth + 1)? {
            if let Some(index) = place.local.filter(|index| *index < frame.inherited)
                && place.path.is_empty()
                && matches!(self.state.arena[self.type_root(place.selected, b)?], Kind::Wrapped(Wrapper::Managed, _))
            {
                let anchor = self.environment_anchor(frame, &frame.locals[index].name, b)?;
                self.observe_resource_capture(frame, object, Some(anchor), place.selected, b, depth + 1)?;
            }
            return Ok(place);
        }
        let selected = self.expression(frame, object, b, depth + 1)?;
        match self.state.arena[self.type_root(selected, b)?] {
            Kind::Address { .. } | Kind::Desc { .. } => Ok(WritePlace {
                selected,
                local: None,
                writable: false,
                description: false,
                projected: true,
                path: Vec::new(),
                indirect: true,
            }),
            _ => Err(unknown(
                "original projected write needs its complete initialized owning root or returned address role",
            )),
        }
    }
    fn write_payload(
        &self,
        place: &mut WritePlace,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        for _ in depth..=b.partial_depth {
            let term = self.type_root(place.selected, b)?;
            match self.state.arena[term] {
                Kind::Address { mutable, inner, .. } => {
                    if !mutable {
                        return Err(fail(
                            "original field write cannot strengthen a shared address into exclusive access",
                        ));
                    }
                    place.selected = inner;
                    place.writable = true;
                    place.projected = true;
                    place.indirect = true;
                }
                Kind::Desc { inner, .. } => {
                    // Desc supplies writable-description access, never an
                    // owning slot. The final selected value must remain Copy.
                    place.selected = inner;
                    place.writable = true;
                    place.description = true;
                    place.projected = true;
                    place.indirect = true;
                }
                Kind::Wrapped(Wrapper::Managed, inner) => {
                    // A resource field assignment observes the owner's
                    // current description. Its Copy field can change even
                    // when the lexical owner binding cannot be reassigned.
                    if self.resource_payload(inner, b)? {
                        place.writable = true;
                        place.description = true;
                    }
                    place.selected = inner;
                    place.projected = true;
                    place.indirect = true;
                }
                _ => return Ok(()),
            }
        }
        Err(resource(
            "original write place exceeds its complete wrapper depth",
        ))
    }
    fn write_place(
        &mut self,
        frame: &mut Frame,
        target: &SemanticOriginalExpr,
        initialized: bool,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<WritePlace>> {
        self.step(depth, b)?;
        let place = match &target.kind {
            CoreExprKind::Name(name) => {
                return self.write_local(frame, name, target.id, initialized, b);
            }
            CoreExprKind::Group(inner) => {
                let Some(place) = self.write_place(frame, inner, initialized, b, depth + 1)? else {
                    return Ok(None);
                };
                place
            }
            CoreExprKind::Ascribed { ty, value } => {
                let Some(place) = self.write_place(frame, value, initialized, b, depth + 1)? else {
                    return Ok(None);
                };
                let expected = self.recipe(frame, ty, b, depth + 1)?;
                self.unify(place.selected, expected, depth + 1, b)?;
                place
            }
            CoreExprKind::Member {
                object,
                member,
                owner,
            } => {
                let mut place = self.write_object(frame, object, b, depth + 1)?;
                self.write_payload(&mut place, b, depth + 1)?;
                if let Some(owner) = owner {
                    let expected = self.recipe(frame, owner, b, depth + 1)?;
                    self.unify(place.selected, expected, depth + 1, b)?;
                }
                // write_payload already checked every implicit wrapper; field
                // is used only for nominal member identity and substitution.
                let (ordinal, selected) = self.field(place.selected, member, b, depth + 1)?;
                Self::push(&mut place.path, ordinal, b)?;
                place.selected = selected;
                place.projected = true;
                place
            }
            CoreExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => {
                let source = self.write_place(frame, operand, true, b, depth + 1)?;
                let (term, local, description) = if let Some(source) = source {
                    (source.selected, source.local, source.description)
                } else {
                    (self.expression(frame, operand, b, depth + 1)?, None, false)
                };
                let Kind::Address {
                    mutable: true,
                    inner,
                    ..
                } = self.state.arena[self.type_root(term, b)?]
                else {
                    return Err(fail(
                        "original dereference write requires its actual exclusive address",
                    ));
                };
                WritePlace {
                    selected: inner,
                    local,
                    writable: true,
                    description,
                    projected: true,
                    path: Vec::new(),
                    indirect: true,
                }
            }
            CoreExprKind::Index { object, index } => {
                let mut place = self.write_object(frame, object, b, depth + 1)?;
                self.write_payload(&mut place, b, depth + 1)?;
                let element = match self.state.arena[self.type_root(place.selected, b)?] {
                    Kind::Array { element, .. } | Kind::Slice(element) => element,
                    _ => {
                        return Err(fail(
                            "original indexed write selects another complete sequence constructor",
                        ));
                    }
                };
                let index = self.expression(frame, index, b, depth + 1)?;
                Self::push(&mut self.numeric, index, b)?;
                Self::push(&mut self.integers, index, b)?;
                place.selected = element;
                place.projected = true;
                place.indirect = true;
                place
            }
            _ => return Ok(None),
        };
        self.write_record(frame, target.id, place.selected, b)?;
        Ok(Some(place))
    }
    fn write_access(
        &mut self,
        frame: &mut Frame,
        place: &WritePlace,
        initialized: bool,
        b: &mut Budget,
    ) -> CompileResult<()> {
        if !place.writable {
            return Err(fail(
                "original projected write targets an immutable owned root",
            ));
        }
        if place.description {
            Self::push(
                &mut self.needs,
                Need {
                    term: place.selected,
                    predicate: Predicate::Copy,
                },
                b,
            )?;
        }
        if let Some(index) = place.local {
            let local = frame
                .locals
                .get(index)
                .ok_or_else(|| fail("original projected write loses its exact lexical root"))?;
            if initialized && !local.initialized {
                return Err(fail(
                    "original projected write loses its initialized root occupant",
                ));
            }
            if frame.alive {
                for hole in &local.maybe_vacant {
                    b.spend(hole.len().saturating_add(place.path.len() + 1), WORK)?;
                    if place.indirect || (place.path.starts_with(hole) && place.path != *hole) {
                        return Err(fail("original write traverses a consumed parent place"));
                    }
                }
            }
            let external_description = place.description
                && match self.state.arena[self.type_root(local.ty, b)?] {
                    Kind::Desc { .. } => true,
                    Kind::Wrapped(Wrapper::Managed, _) => !self.owns_captures(frame.definition, b)?,
                    _ => false,
                };
            if index < frame.inherited && !external_description {
                b.lookup(self.mutating_closures.len(), WORK)?;
                self.mutating_closures.insert(frame.definition);
            }
        }
        Ok(())
    }
    fn write_complete(
        &mut self,
        frame: &mut Frame,
        place: &WritePlace,
        b: &mut Budget,
    ) -> CompileResult<()> {
        if !frame.alive {
            return Ok(());
        }
        if let Some(index) = place.local {
            let local = frame
                .locals
                .get(index)
                .ok_or_else(|| fail("original write RHS loses its complete source root"))?;
            if place.projected && !local.initialized {
                return Err(fail(
                    "original write RHS leaves its projected root uninitialized",
                ));
            }
            // A source RHS may read the same root by value. That is legal only
            // when this entire original root is independently Copy. Replacing
            // one field cannot silently reinitialize a moved owning aggregate.
            if place.projected && local.used {
                Self::push(
                    &mut self.needs,
                    Need {
                        term: local.ty,
                        predicate: Predicate::Copy,
                    },
                    b,
                )?;
            }
            if !place.indirect {
                b.spend(
                    local
                        .vacant
                        .len()
                        .saturating_add(local.maybe_vacant.len())
                        .saturating_mul(place.path.len().saturating_add(1)),
                    WORK,
                )?;
                frame.locals[index]
                    .vacant
                    .retain(|hole| !hole.starts_with(&place.path));
                frame.locals[index]
                    .maybe_vacant
                    .retain(|hole| !hole.starts_with(&place.path));
            }
            frame.locals[index].used = false;
            frame.locals[index].live = true;
            if !place.projected {
                frame.locals[index].initialized = true;
            }
        }
        Ok(())
    }
    pub(super) fn projected_assign(
        &mut self,
        frame: &mut Frame,
        statement: NodeId,
        target: &SemanticOriginalExpr,
        value: &SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        if matches!(target.kind, CoreExprKind::Name(_)) {
            return Ok(false);
        }
        let Some(place) = self.write_place(frame, target, false, b, depth + 1)? else {
            return Ok(false);
        };
        let anchor = if let Some(index) = place.local.filter(|_| !place.indirect) {
            if index < frame.inherited { Some(self.environment_anchor(frame, &frame.locals[index].name, b)?) }
            else { frame.locals[index].anchor }
        } else { None };
        self.record_cleanup_write(frame.definition, statement, anchor, &place.path, value.id, b)?;
        self.write_access(frame, &place, place.projected, b)?;
        let actual = self.expression_expected(frame, value, place.selected, b, depth + 1)?;
        self.unify(actual, place.selected, depth + 1, b)?;
        self.write_access(frame, &place, place.projected, b)?;
        if frame.alive {
            if let Some(index) = place.local.filter(|_| !place.indirect) {
                let local = &frame.locals[index];
                if local.live {
                    let vacant = self.copy_vacancies(&local.vacant, b)?;
                    let origin = cleanup::Origin::local(frame.definition, anchor, cleanup::Event::Replace(statement));
                    self.cleanup_partial(place.selected, &place.path, &vacant, &origin, b, depth + 1)?;
                }
            } else {
                let mut origin = cleanup::Origin::expression(frame.definition, target.id);
                origin.event = cleanup::Event::Replace(statement);
                self.record_cleanup(place.selected, &origin, &place.path, b)?;
            }
        }
        self.write_complete(frame, &place, b)?;
        Ok(true)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn projected_compound(
        &mut self,
        frame: &mut Frame,
        target: &SemanticOriginalExpr,
        op: BinaryOp,
        value: &SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        if matches!(target.kind, CoreExprKind::Name(_)) {
            return Ok(false);
        }
        if matches!(
            op,
            BinaryOp::And
                | BinaryOp::Or
                | BinaryOp::Equal
                | BinaryOp::NotEqual
                | BinaryOp::Less
                | BinaryOp::LessEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterEqual
        ) {
            return Err(fail(
                "original projected compound write has a nonassignment operator",
            ));
        }
        let Some(place) = self.write_place(frame, target, true, b, depth + 1)? else {
            return Ok(false);
        };
        self.write_access(frame, &place, true, b)?;
        if let Some(index) = place.local {
            self.local_available(
                &frame.locals[index],
                if place.indirect { &[] } else { &place.path },
                b,
            )?;
        }
        Self::push(
            &mut self.needs,
            Need {
                term: place.selected,
                predicate: Predicate::Copy,
            },
            b,
        )?;
        let actual = self.expression_expected(frame, value, place.selected, b, depth + 1)?;
        self.unify(actual, place.selected, depth + 1, b)?;
        Self::push(&mut self.numeric, place.selected, b)?;
        if matches!(
            op,
            BinaryOp::AddWrapping
                | BinaryOp::SubtractWrapping
                | BinaryOp::MultiplyWrapping
                | BinaryOp::Remainder
                | BinaryOp::BitAnd
                | BinaryOp::BitOr
                | BinaryOp::BitXor
                | BinaryOp::ShiftLeft
                | BinaryOp::ShiftRight
        ) {
            Self::push(&mut self.integers, place.selected, b)?;
        }
        self.write_complete(frame, &place, b)?;
        Ok(true)
    }
}
