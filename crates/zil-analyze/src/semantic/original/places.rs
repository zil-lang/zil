//! Original lexical places separate observing a Copy field from consuming its
//! enclosing value. Environment ordinals name original sealed slots, never
//! physical capture positions inferred from a matching field type.
use super::*;

pub(super) struct Place {
    pub(super) anchor: Option<LocalAnchor>,
    pub(super) root: Term,
    pub(super) path: Vec<u32>,
    pub(super) selected: Term,
}
impl Solver<'_, '_> {
    pub(super) fn environment_anchor(
        &self,
        frame: &Frame,
        name: &str,
        b: &mut Budget,
    ) -> CompileResult<LocalAnchor> {
        let definition = self.inventory.definition(frame.definition, b)?;
        let mut selected = None;
        for (ordinal, environment) in definition.environment.iter().enumerate() {
            crate::dyn_metadata::recipe(&environment.source, b)?;
            let SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source: SemanticOriginalTypeSource::NameAt { name: actual, .. },
            }) = &environment.source
            else {
                continue;
            };
            if actual != name {
                continue;
            }
            let ordinal = u32::try_from(ordinal)
                .map_err(|_| resource("original environment place ordinal overflow"))?;
            if selected
                .replace(LocalAnchor::Environment(ordinal))
                .is_some()
            {
                return Err(fail(
                    "original captured place has competing environment origins",
                ));
            }
        }
        selected.ok_or_else(|| {
            unknown("original captured place lacks its complete parent-domain name anchor")
        })
    }
    pub(super) fn place(
        &mut self,
        frame: &mut Frame,
        value: &SemanticOriginalExpr,
        record: bool,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<Place>> {
        let place = self.place_inner(frame, value, record, b, depth)?;
        if let Some(place) = &place {
            self.place_available(frame, place.anchor, &place.path, false, b)?;
        }
        Ok(place)
    }
    fn place_inner(
        &mut self,
        frame: &mut Frame,
        value: &SemanticOriginalExpr,
        record: bool,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<Place>> {
        self.step(depth, b)?;
        let place = match &value.kind {
            CoreExprKind::Name(name) => {
                let mut selected = None;
                for (index, local) in frame.locals.iter().enumerate().rev() {
                    b.spend(local.name.len().saturating_add(name.len() + 1), WORK)?;
                    if local.name == *name {
                        selected = Some(index);
                        break;
                    }
                }
                let Some(index) = selected else {
                    return Ok(None);
                };
                let local = &frame.locals[index];
                if !local.initialized {
                    return Err(fail(
                        "original place observation has no initialized occupant",
                    ));
                }
                let (term, anchor, used) = (local.ty, local.anchor, local.used);
                if record {
                    self.record_environment_read(frame, index, value.id, b)?;
                    if used {
                        Self::push(
                            &mut self.needs,
                            Need {
                                term,
                                predicate: Predicate::Copy,
                            },
                            b,
                        )?;
                    }
                    if index < frame.inherited {
                        b.lookup(frame.captured.len(), WORK)?;
                        frame.captured.insert(index, term);
                    }
                }
                let anchor = match anchor {
                    Some(anchor) => Some(anchor),
                    None if index < frame.inherited => {
                        Some(self.environment_anchor(frame, name, b)?)
                    }
                    None => None,
                };
                Place {
                    anchor,
                    root: term,
                    path: Vec::new(),
                    selected: term,
                }
            }
            CoreExprKind::Group(inner) | CoreExprKind::Ascribed { value: inner, .. } => {
                let Some(place) = self.place_inner(frame, inner, record, b, depth + 1)? else {
                    return Ok(None);
                };
                if let CoreExprKind::Ascribed { ty, .. } = &value.kind {
                    let expected = self.recipe(frame, ty, b, depth + 1)?;
                    self.unify(place.selected, expected, depth + 1, b)?;
                }
                place
            }
            CoreExprKind::Member {
                object,
                member,
                owner,
            } => {
                let Some(mut place) = self.place_inner(frame, object, record, b, depth + 1)? else {
                    return Ok(None);
                };
                if let Some(owner) = owner {
                    let owner = self.recipe(frame, owner, b, depth + 1)?;
                    self.unify(place.selected, owner, depth + 1, b)?;
                }
                let (ordinal, selected) = self.field(place.selected, member, b, depth + 1)?;
                Self::push(&mut place.path, ordinal, b)?;
                place.selected = selected;
                place
            }
            _ => return Ok(None),
        };
        if record {
            b.lookup(frame.expressions.len(), WORK)?;
            if frame.expressions.insert(value.id, place.selected).is_some() {
                return Err(fail("original observed place node is reused"));
            }
        }
        Ok(Some(place))
    }
    fn inline_package_field(
        &self,
        place: &Place,
        selected: Term,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        // Only the declared inline Package owner can produce this writable
        // description. An address edge or a different owning constructor
        // cannot be turned into an inline Package by field-name selection.
        let mut current = self.type_root(place.root, b)?;
        for field in &place.path {
            b.spend(1, WORK)?;
            let Kind::Nominal { definition, fields, .. } = &self.state.arena[current] else {
                return Ok(false);
            };
            if !matches!(self.inventory.definition(*definition, b)?.body, SemanticDefinitionBody::Nominal {
                kind: LifeNominalKind::Package, body: SemanticNominalBody::Product { .. }, ..
            }) {
                return Ok(false);
            }
            let child = fields.get(*field as usize)
                .ok_or_else(|| fail("original description leaves its complete Package field path"))?;
            current = self.type_root(*child, b)?;
        }
        Ok(!place.path.is_empty() && current == selected)
    }
    pub(super) fn give_inline_field(
        &mut self,
        frame: &mut Frame,
        operand: &SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Place> {
        let place = self.place(frame, operand, true, b, depth + 1)?
            .ok_or_else(|| fail("original give has no exact owning inline place"))?;
        let selected = self.type_root(place.selected, b)?;
        let tuple = place.path.len() == 1
            && matches!(self.state.arena[self.type_root(place.root, b)?], Kind::Tuple(_));
        let owns = match self.state.arena[selected] {
            Kind::Wrapped(Wrapper::Managed, _) => true,
            Kind::Nominal { definition, .. } if !tuple => matches!(
                self.inventory.definition(definition, b)?.body,
                SemanticDefinitionBody::Nominal {
                    kind: LifeNominalKind::Package,
                    body: SemanticNominalBody::Product { .. },
                    ..
                }
            ),
            _ => false,
        };
        if !owns || (!tuple && !self.inline_package_field(&place, selected, b)?) {
            return Err(fail("original give requires a direct owning inline field"));
        }
        if frame.alive {
            self.retire_lexical_place(frame, place.anchor, &place.path, b)?;
        }
        Ok(place)
    }

    pub(super) fn observe_member(
        &mut self,
        frame: &mut Frame,
        expression: &SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<Term>> {
        let CoreExprKind::Member { object, member, owner } = &expression.kind else {
            return Err(fail("original member observation selects another expression"));
        };
        let Some(mut place) = self.place_inner(frame, object, false, b, depth + 1)? else {
            return Ok(None);
        };
        let (ordinal, field) = self.field(place.selected, member, b, depth + 1)?;
        Self::push(&mut place.path, ordinal, b)?;
        self.place_available(frame, place.anchor, &place.path, false, b)?;
        let selected = self.type_root(field, b)?;
        let description = matches!(self.state.arena[selected], Kind::Wrapped(Wrapper::Managed, _))
            && self.inline_package_field(&place, selected, b)?;
        // Numeric inference is finalized after the complete original body.
        // Observing an array/tuple of pending numeric leaves can defer its Copy
        // obligation; it must not default a constructor's Type before later
        // wanted/field constraints or guess a generic resource to be Copy.
        let mut transfer = false;
        if !description && !self.pending_numeric_copy(field, b, depth + 1)?
            && (matches!(self.state.arena[selected], Kind::Infer(None))
                || !self.satisfies(field, Predicate::Copy, b, depth + 1)?)
        {
            // Reading an affine inline aggregate transfers just that field. A
            // later access to a sibling starts at its own declared field path.
            let aggregate = match &self.state.arena[selected] {
                Kind::Nominal { definition, .. } => matches!(
                    self.inventory.definition(*definition, b)?.body,
                    SemanticDefinitionBody::Nominal { kind: LifeNominalKind::Package | LifeNominalKind::Enum, .. }
                ),
                Kind::Tuple(_) | Kind::ErrorUnion { .. } => true,
                _ => false,
            };
            if !aggregate || !self.inline_package_field(&place, selected, b)? {
                return Ok(None);
            }
            transfer = true;
        }
        if let Some(owner) = owner {
            let owner = self.recipe(frame, owner, b, depth + 1)?;
            self.unify(place.selected, owner, depth + 1, b)?;
        }
        self.place_inner(frame, object, true, b, depth + 1)?;
        self.observe_resource_capture(frame, object, place.anchor, place.root, b, depth + 1)?;
        if description {
            return self.describe_argument(frame, expression, &place, field, b, depth + 1).map(Some);
        }
        if transfer {
            if frame.alive {
                self.retire_lexical_place(frame, place.anchor, &place.path, b)?;
            }
        } else {
            Self::push(
                &mut self.needs,
                Need { term: field, predicate: Predicate::Copy },
                b,
            )?;
        }
        b.lookup(frame.expressions.len(), WORK)?;
        if frame.expressions.insert(expression.id, field).is_some() {
            return Err(fail("original selected field node is reused"));
        }
        Ok(Some(field))
    }
    pub(super) fn observe_resource_capture(
        &mut self,
        frame: &mut Frame,
        mut source: &SemanticOriginalExpr,
        anchor: Option<LocalAnchor>,
        root: Term,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        let Some(LocalAnchor::Environment(ordinal)) = anchor else { return Ok(()); };
        if !matches!(self.state.arena[self.type_root(root, b)?], Kind::Wrapped(Wrapper::Managed, _))
            || self.capture_owns_anchor(frame.definition, ordinal, b)?
        {
            return Ok(());
        }
        // The capture's actual description comes from the free Name, even
        // when the user observes it through a chain of Copy fields.
        while let CoreExprKind::Member { object: child, .. }
            | CoreExprKind::Group(child)
            | CoreExprKind::Ascribed { value: child, .. } = &source.kind
        {
            self.step(depth, b)?;
            source = child;
        }
        let owner = Place { anchor, root, path: Vec::new(), selected: root };
        self.describe_argument(frame, source, &owner, root, b, depth + 1)?;
        Ok(())
    }
    pub(super) fn pending_numeric_copy(&self, term: Term, b: &mut Budget, depth: usize) -> CompileResult<bool> {
        self.step(depth, b)?;
        let root = self.type_root(term, b)?;
        match &self.state.arena[root] {
            Kind::Infer(None) => {
                for literal in &self.literals {
                    b.spend(1, WORK)?;
                    if self.type_root(literal.term, b)? == root { return Ok(true); }
                }
                for numeric in self.numeric.iter().chain(&self.integers) {
                    b.spend(1, WORK)?;
                    if self.type_root(*numeric, b)? == root { return Ok(true); }
                }
                Ok(false)
            }
            Kind::Array { element, .. } => self.pending_numeric_copy(*element, b, depth + 1),
            Kind::Tuple(fields) => {
                for field in fields {
                    if !self.pending_numeric_copy(*field, b, depth + 1)? { return Ok(false); }
                }
                Ok(true)
            }
            Kind::Atom(ty) => Ok(ty.is_b0_scalar()),
            _ => Ok(false),
        }
    }
    pub(super) fn field(
        &self,
        term: Term,
        name: &str,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<(u32, Term)> {
        self.step(depth, b)?;
        let mut root = self.type_root(term, b)?;
        for _ in depth..=b.partial_depth {
            b.spend(1, WORK)?;
            root = match &self.state.arena[root] {
                Kind::Address { inner, .. }
                | Kind::Desc { inner, .. }
                | Kind::Wrapped(Wrapper::Managed, inner) => self.type_root(*inner, b)?,
                _ => break,
            };
        }
        if let Kind::Tuple(fields) = &self.state.arena[root] {
            b.spend(name.len().saturating_add(fields.len() + 1), WORK)?;
            if name.is_empty() || !name.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(fail("original tuple place selects an invalid ordinal"));
            }
            let ordinal = name
                .parse::<u32>()
                .map_err(|_| fail("original tuple place ordinal overflows"))?;
            return Ok((
                ordinal,
                *fields
                    .get(ordinal as usize)
                    .ok_or_else(|| fail("original tuple place exceeds its complete arity"))?,
            ));
        }
        let Kind::Nominal {
            definition, fields, ..
        } = &self.state.arena[root]
        else {
            return Err(unknown(
                "original place needs its complete nominal product type",
            ));
        };
        let declaration = self.inventory.definition(*definition, b)?;
        let SemanticDefinitionBody::Nominal {
            body: SemanticNominalBody::Product { fields: recipes },
            ..
        } = &declaration.body
        else {
            return Err(fail("original field place selects another nominal family"));
        };
        let mut selected = None;
        for (ordinal, field) in recipes.iter().enumerate() {
            b.spend(name.len().saturating_add(field.name.len() + 1), WORK)?;
            if field.name == name && selected.replace(ordinal).is_some() {
                return Err(fail("original field place repeats a declaration member"));
            }
        }
        let ordinal =
            selected.ok_or_else(|| fail("original field place selects no declared member"))?;
        Ok((
            u32::try_from(ordinal)
                .map_err(|_| resource("original field place ordinal overflow"))?,
            *fields
                .get(ordinal)
                .ok_or_else(|| fail("original field place omits its substituted full type"))?,
        ))
    }
}
