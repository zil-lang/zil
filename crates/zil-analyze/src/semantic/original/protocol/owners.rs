//! Original owner-result identities. A complete owner type permits tracking a
//! fresh call result, never assuming that an arbitrary input is already Closed.
use super::*;
impl Proof<'_, '_> {
    pub(super) fn async_owner(&self, mut term: Term, b: &mut Budget) -> CompileResult<bool> {
        for _ in 0..=b.partial_depth {
            term = self.checked.root(term, b)?;
            if let Kind::CallResult { hidden, .. } = self.checked.arena[term] {
                term = hidden;
                continue;
            }
            if let Kind::ErrorUnion { result, .. } = self.checked.arena[term] {
                term = result;
                continue;
            }
            let Kind::Wrapped(Wrapper::Managed, payload) = self.checked.arena[term] else {
                return Ok(false);
            };
            return Ok(matches!(
                self.checked.arena[self.checked.root(payload, b)?],
                Kind::Wrapped(Wrapper::AsyncRegistration, _) | Kind::Task(_)
            ));
        }
        Err(resource(
            "original owner-result identity exceeds its finite type depth",
        ))
    }
    pub(super) fn fresh_owner(
        &self,
        state: &mut State,
        node: NodeId,
        b: &mut Budget,
    ) -> CompileResult<Data> {
        let SemanticDefinitionBody::FunctionWithBody { parameters, .. } = &self.definition.body
        else {
            return Err(fail(
                "original owner result has no complete function domain",
            ));
        };
        let frame = self
            .checked
            .frames
            .get(&self.definition.id)
            .ok_or_else(|| fail("original owner construction has no checked lexical frame"))?;
        // Formal parameters, original inherited roots and construction sites
        // occupy disjoint namespaces. Equal node/slot numbers confer nothing.
        let base = u32::try_from(
            parameters
                .len()
                .checked_add(frame.inherited)
                .ok_or_else(|| resource("original owner role domain overflow"))?,
        )
        .map_err(|_| resource("original owner role ordinal overflow"))?;
        let role = base
            .checked_add(node.0)
            .ok_or_else(|| resource("original owner-result occurrence overflow"))?;
        b.lookup(state.phases.len(), WORK)?;
        if state.phases.get(&role).is_some_and(|phase| *phase != 0) {
            return Err(unknown(
                "original repeated owner construction needs separate live-instance responsibility",
            ));
        }
        state.phases.insert(role, ANY);
        Ok(Data {
            alias: Some(role),
            exposed: false,
            outcomes: Vec::new(),
        })
    }
}

impl Proof<'_, '_> {
    pub(super) fn captured_heads(
        &self,
        phases: &mut BTreeMap<u32, u8>,
        locals: &mut Vec<(String, Data)>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let definition = self.inventory.definition(self.definition.id, b)?;
        let SemanticDefinitionBody::FunctionWithBody { parameters, .. } = &definition.body else {
            return Err(fail(
                "original protocol capture has no full function declaration",
            ));
        };
        let frame = self
            .checked
            .frames
            .get(&self.definition.id)
            .ok_or_else(|| {
                fail("original protocol captures have no independently checked lexical frame")
            })?;
        if frame.inherited > frame.locals.len() {
            return Err(fail(
                "original protocol capture prefix exceeds its complete lexical frame",
            ));
        }
        for (index, local) in frame.locals.iter().take(frame.inherited).enumerate() {
            b.lookup(frame.captured.len(), WORK)?;
            let Some(term) = frame.captured.get(&index) else {
                continue;
            };
            let mut root = self.checked.root(*term, b)?;
            for _ in 0..=b.partial_depth {
                if let Kind::CallResult { hidden, .. } = self.checked.arena[root] {
                    root = self.checked.root(hidden, b)?;
                } else {
                    break;
                }
            }
            let head = match self.checked.arena[root] {
                Kind::Address {
                    mutable: true,
                    inner,
                    ..
                } => self.async_owner(inner, b)?,
                _ => self.async_owner(root, b)?,
            };
            let alias = if head {
                let role = u32::try_from(
                    parameters
                        .len()
                        .checked_add(index)
                        .ok_or_else(|| resource("original captured owner role overflow"))?,
                )
                .map_err(|_| resource("original captured owner role ordinal overflow"))?;
                b.lookup(phases.len(), WORK)?;
                if phases.insert(role, ANY).is_some() {
                    return Err(fail(
                        "original captured owner role aliases another declaration namespace",
                    ));
                }
                Some(role)
            } else {
                None
            };
            b.spend(local.name.len(), WORK)?;
            Solver::push(
                locals,
                (
                    local.name.clone(),
                    Data {
                        alias,
                        exposed: !head && self.captured_exposure(root, b, 0)?,
                        outcomes: Vec::new(),
                    },
                ),
                b,
            )?;
        }
        Ok(())
    }
}

impl Proof<'_, '_> {
    fn captured_exposure(&self, term: Term, b: &mut Budget, depth: usize) -> CompileResult<bool> {
        self.step(b, depth)?;
        let term = self.checked.root(term, b)?;
        match &self.checked.arena[term] {
            Kind::Atom(_)
            | Kind::ConstValue(_)
            | Kind::ConstRigid { .. }
            | Kind::Life(_)
            | Kind::LifeVariable(_) => Ok(false),
            Kind::CallResult { hidden, .. } => self.captured_exposure(*hidden, b, depth + 1),
            Kind::Code {
                kind: SemanticCodeKind::FunctionPointer | SemanticCodeKind::ForeignCallback,
                ..
            } => Ok(false),
            Kind::Tuple(fields) | Kind::Nominal { fields, .. } => {
                for field in fields {
                    if self.captured_exposure(*field, b, depth + 1)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Kind::Array { element, .. } | Kind::Slice(element) => {
                self.captured_exposure(*element, b, depth + 1)
            }
            Kind::ErrorUnion { error, result } => {
                Ok(self.captured_exposure(*error, b, depth + 1)?
                    || self.captured_exposure(*result, b, depth + 1)?)
            }
            Kind::Wrapped(_, inner) => self.captured_exposure(*inner, b, depth + 1),
            // An address, opaque callable or unknown generic domain can hide a
            // mutable alias to another tracked head. Its invocation/cleanup is
            // therefore never treated as a no-head call merely from layout.
            _ => Ok(true),
        }
    }
}
