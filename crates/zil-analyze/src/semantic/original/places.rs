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
            CoreExprKind::Group(inner) => {
                let Some(place) = self.place(frame, inner, record, b, depth + 1)? else {
                    return Ok(None);
                };
                place
            }
            CoreExprKind::Member {
                object,
                member,
                owner,
            } => {
                let Some(mut place) = self.place(frame, object, record, b, depth + 1)? else {
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
    pub(super) fn copy_member(
        &mut self,
        frame: &mut Frame,
        object: &SemanticOriginalExpr,
        member: &str,
        owner: &Option<SemanticTypeRecipe>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<Term>> {
        let Some(place) = self.place(frame, object, false, b, depth + 1)? else {
            return Ok(None);
        };
        let (_, field) = self.field(place.selected, member, b, depth + 1)?;
        if matches!(
            self.state.arena[self.type_root(field, b)?],
            Kind::Infer(None)
        ) || !self.satisfies(field, Predicate::Copy, b, depth + 1)?
        {
            return Ok(None);
        }
        if let Some(owner) = owner {
            let owner = self.recipe(frame, owner, b, depth + 1)?;
            self.unify(place.selected, owner, depth + 1, b)?;
        }
        self.place(frame, object, true, b, depth + 1)?;
        Self::push(
            &mut self.needs,
            Need {
                term: field,
                predicate: Predicate::Copy,
            },
            b,
        )?;
        Ok(Some(field))
    }
    pub(super) fn field(
        &self,
        term: Term,
        name: &str,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<(u32, Term)> {
        self.step(depth, b)?;
        let root = self.type_root(term, b)?;
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
