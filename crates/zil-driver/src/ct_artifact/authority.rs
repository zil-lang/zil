//! Verification authority for one immutable source graph and stage snapshot.
//! The raw artifact's copied table and snapshot are compared to these values.
use super::source_index::SourceIndex;
use zil_base::Fingerprint;
use zil_ir::ct_artifact::{
    CtArtifactProgram, CtFunction, CtInputTable, CtQueryKind, CtReadyStage, CtSnapshotRequest,
    CtSymbolId,
};
use zil_verify::{CtArtifactAuthority, CtSymbolUsage};

pub(super) struct FrozenAuthority<'a> {
    pub invocation_context: Option<&'a super::invocation_context::FrozenInvocationContext>,
    pub source_fingerprint: Fingerprint,
    pub snapshot: &'a CtSnapshotRequest,
    pub input: &'a CtInputTable,
    pub index: &'a SourceIndex,
    /// Reattributed from the immutable source request by the driver. This
    /// comparison authenticates definition-site provenance, not CT correctness.
    pub expected_functions: &'a [CtFunction],
    pub expected_artifact: &'a CtArtifactProgram,
}

pub(super) fn stage_allows(stage: CtReadyStage, kind: CtQueryKind) -> bool {
    let reached = match stage {
        CtReadyStage::Tokens => 0,
        CtReadyStage::Config => 1,
        CtReadyStage::Shape => 2,
        CtReadyStage::ImplIndex => 3,
        CtReadyStage::Layout => 4,
        CtReadyStage::Contract => 5,
        CtReadyStage::VerifiedBody => 6,
    };
    let needed = match kind {
        CtQueryKind::Shape | CtQueryKind::Field => 2,
        CtQueryKind::ImplIndex => 3,
        CtQueryKind::Layout => 4,
    };
    reached >= needed
}

impl CtArtifactAuthority for FrozenAuthority<'_> {
    fn active_scope_count(&self) -> Option<usize> {
        self.invocation_context
            .map(|context| context.active_scope_count())
    }

    fn scope_parameter_count(
        &self,
        scope: &zil_ir::ct_artifact::CtAmbientLifeScope,
    ) -> Option<usize> {
        self.invocation_context?.parameter_count(scope)
    }
    fn scope_parameter_kind(
        &self,
        scope: &zil_ir::ct_artifact::CtAmbientLifeScope,
        ordinal: usize,
    ) -> Option<zil_verify::CtScopeParameterKind> {
        self.invocation_context?.parameter_kind(scope, ordinal)
    }
    fn source_fingerprint(&self) -> Fingerprint {
        self.source_fingerprint
    }
    fn snapshot(&self) -> &CtSnapshotRequest {
        self.snapshot
    }
    fn authenticate_input_table(&self, input: &CtInputTable) -> bool {
        input == self.input
    }
    fn authenticate_artifact(&self, raw: &CtArtifactProgram) -> bool {
        raw == self.expected_artifact
    }
    fn authenticate_function(&self, function: &CtFunction) -> bool {
        self.index.contains(&function.definition)
            && self
                .expected_functions
                .iter()
                .any(|expected| expected == function)
    }
    fn authorize_symbol(
        &self,
        requester: &CtSymbolId,
        symbol: &CtSymbolId,
        _usage: CtSymbolUsage,
    ) -> bool {
        self.index.visible(requester, symbol)
    }
    fn authenticate_type_context(
        &self,
        function: &CtFunction,
        instruction: &zil_ir::ct_artifact::CtInstruction,
        ty: &zil_ir::ct_artifact::CtTypeRef,
    ) -> bool {
        self.invocation_context
            .is_some_and(|context| context.authenticated_type_context(function, instruction, ty))
    }
    fn authorize_query(&self, requester: &CtSymbolId, kind: CtQueryKind) -> bool {
        let _ = kind;
        self.index.contains(requester)
    }
}
