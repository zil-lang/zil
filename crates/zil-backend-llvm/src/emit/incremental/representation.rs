//! Successful pure representation predicates over immutable raw functions.
//! Current Verify, exact LIR binding and all paid/module-level checks stay live.
use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Cache {
    functions: Option<zil_ir::PersistentVec<TypedFunction>>,
}
impl Session {
    pub(in crate::emit) fn check_representations(
        &mut self,
        lir: &LirView<'_>,
    ) -> CompileResult<()> {
        // These predicates read functions plus closure/resource definitions.
        // Keep the original path for either nominal dependency domain; an
        // empty-domain owner can never authorize a changed nominal world.
        if !lir.closures.is_empty() || !lir.resources.is_empty() {
            for function in lir.functions {
                self.representation_checked += 1;
                check_function_representation(function, lir)?;
            }
            return Ok(());
        }
        let changed = match &self.representation.functions {
            Some(old) if old.len() == lir.functions.len() => lir.functions.changed_indices(old),
            _ => (0..lir.functions.len()).collect(),
        };
        // changed_indices visits actual owning tree differences in source order.
        // It cannot be supplied by a caller, and both roots own every reused
        // input. No digest, diagnostic fingerprint or previous capability grants
        // the result; mutations and independently allocated inputs are rechecked.
        self.representation_reused = lir.functions.len() - changed.len();
        let mut bodies = std::collections::HashMap::<scalar_bodies::Owners, &TypedFunction>::new();
        for index in changed {
            let function = &lir.functions[index];
            let owner = scalar_bodies::Owners::of(function);
            if bodies
                .get(&owner)
                .is_some_and(|old| scalar_bodies::Owners::same(old, function))
            {
                // Successful pure predicates read no self ID/name/source span.
                // Reuse only within this current nominal-free LIR view, after
                // complete binding; failures always run at the current site.
                self.representation_reused += 1;
                continue;
            }
            self.representation_checked += 1;
            check_function_representation(function, lir)?;
            bodies.insert(owner, function);
        }
        // Publish only the complete successful sequence. Failed checks preserve
        // the original first diagnostic and do not retain a partial new root.
        self.representation.functions = Some(lir.functions.clone());
        Ok(())
    }
}
