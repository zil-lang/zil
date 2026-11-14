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
        let base = u32::try_from(parameters.len())
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
            outcomes: Vec::new(),
        })
    }
}
