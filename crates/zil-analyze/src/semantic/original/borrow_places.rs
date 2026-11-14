//! Source borrow places retain their physical inline path and root category.
//! Reading a field does not supply exclusive access through a shared head.
use super::*;

pub(super) struct Place {
    pub anchor: LocalAnchor,
    pub root: Term,
    pub path: Vec<u32>,
    pub inner: Term,
    pub indirect: bool,
    pub mutable: bool,
}

pub(in crate::semantic) struct Source {
    pub anchor: LocalAnchor,
    pub root: crate::semantic::value::Value,
    pub path: Vec<u32>,
    pub inner: crate::semantic::value::Value,
    pub indirect: bool,
    pub slice: Option<slice_borrows::SliceProjection>,
}

impl Solver<'_, '_> {
    pub(super) fn borrow_place(
        &mut self,
        frame: &mut Frame,
        value: &SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Place> {
        self.step(depth, b)?;
        let place = match &value.kind {
            CoreExprKind::Name(name) => {
                let mut found = None;
                for (index, local) in frame.locals.iter().enumerate().rev() {
                    b.spend(local.name.len().saturating_add(name.len() + 1), WORK)?;
                    if local.name == *name {
                        found = Some(index);
                        break;
                    }
                }
                let index =
                    found.ok_or_else(|| fail("original borrowed place has no lexical binding"))?;
                let local = &frame.locals[index];
                if !local.initialized {
                    return Err(fail("original borrowed place has no initialized occupant"));
                }
                let (inner, mutable, used, anchor) =
                    (local.ty, local.mutable, local.used, local.anchor);
                if used {
                    Self::push(
                        &mut self.needs,
                        Need {
                            term: inner,
                            predicate: Predicate::Copy,
                        },
                        b,
                    )?;
                }
                self.record_environment_read(frame, index, value.id, b)?;
                if index < frame.inherited {
                    b.lookup(frame.captured.len(), WORK)?;
                    frame.captured.insert(index, inner);
                }
                let anchor = match anchor {
                    Some(anchor) => anchor,
                    None if index < frame.inherited => self.environment_anchor(frame, name, b)?,
                    None => {
                        return Err(unknown(
                            "original borrow needs its exact parameter or binding anchor",
                        ));
                    }
                };
                Place {
                    anchor,
                    root: inner,
                    path: Vec::new(),
                    inner,
                    indirect: false,
                    mutable,
                }
            }
            CoreExprKind::Group(inner) | CoreExprKind::Ascribed { value: inner, .. } => {
                let place = self.borrow_place(frame, inner, b, depth + 1)?;
                if let CoreExprKind::Ascribed { ty, .. } = &value.kind {
                    let expected = self.recipe(frame, ty, b, depth + 1)?;
                    self.unify(place.inner, expected, depth + 1, b)?;
                }
                place
            }
            CoreExprKind::Member {
                object,
                member,
                owner,
            } => {
                let mut place = self.borrow_place(frame, object, b, depth + 1)?;
                self.borrow_head(&mut place, b, depth + 1)?;
                if let Some(owner) = owner {
                    let expected = self.recipe(frame, owner, b, depth + 1)?;
                    self.unify(place.inner, expected, depth + 1, b)?;
                }
                let (ordinal, inner) = self.field(place.inner, member, b, depth + 1)?;
                Self::push(&mut place.path, ordinal, b)?;
                place.inner = inner;
                place
            }
            CoreExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => {
                let mut place = self.borrow_place(frame, operand, b, depth + 1)?;
                let root = self.type_root(place.inner, b)?;
                if !matches!(self.state.arena[root], Kind::Address { .. }) {
                    return Err(fail(
                        "original borrowed dereference has no complete address",
                    ));
                }
                self.borrow_head(&mut place, b, depth + 1)?;
                place
            }
            _ => {
                return Err(unknown(
                    "original borrow needs its exact initialized physical place projection",
                ));
            }
        };
        b.lookup(frame.expressions.len(), WORK)?;
        if frame.expressions.insert(value.id, place.inner).is_some() {
            return Err(fail("original borrowed place reuses its source node"));
        }
        Ok(place)
    }

    fn borrow_head(&self, place: &mut Place, b: &mut Budget, depth: usize) -> CompileResult<()> {
        self.step(depth, b)?;
        let root = self.type_root(place.inner, b)?;
        match self.state.arena[root] {
            Kind::Address { mutable, inner, .. } => {
                if place.indirect || !place.path.is_empty() {
                    return Err(unknown(
                        "original borrow needs the exact nested pointer load boundary",
                    ));
                }
                place.indirect = true;
                place.mutable = mutable;
                place.inner = inner;
                if matches!(
                    self.state.arena[self.type_root(inner, b)?],
                    Kind::Address { .. } | Kind::Desc { .. } | Kind::Wrapped(Wrapper::Managed, _)
                ) {
                    return Err(unknown(
                        "original borrow cannot flatten another owning or pointer boundary",
                    ));
                }
                Ok(())
            }
            Kind::Desc { .. } | Kind::Wrapped(Wrapper::Managed, _) => Err(unknown(
                "original borrow requires its exact resource owning-region boundary",
            )),
            _ => Ok(()),
        }
    }
}
