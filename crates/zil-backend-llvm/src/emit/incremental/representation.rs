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
        for index in changed {
            self.representation_checked += 1;
            check_function_representation(&lir.functions[index], lir)?;
        }
        // Publish only the complete successful sequence. Failed checks preserve
        // the original first diagnostic and do not retain a partial new root.
        self.representation.functions = Some(lir.functions.clone());
        Ok(())
    }
}
