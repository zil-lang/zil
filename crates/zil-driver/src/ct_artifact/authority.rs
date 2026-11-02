//! Verification authority for one immutable source graph and stage snapshot.
//! The raw artifact's copied table and snapshot are compared to these values.
use super::source_index::SourceIndex;
use zil_base::Fingerprint;
use zil_ir::ct_artifact::{CtInputTable, CtQueryKind, CtReadyStage, CtSnapshotRequest, CtSymbolId};
use zil_verify::{CtArtifactAuthority, CtSymbolUsage};

pub(super) struct FrozenAuthority<'a> {
    pub source_fingerprint: Fingerprint,
    pub snapshot: &'a CtSnapshotRequest,
    pub input: &'a CtInputTable,
    pub index: &'a SourceIndex,
}

fn stage_allows(stage: CtReadyStage, kind: CtQueryKind) -> bool {
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
    fn source_fingerprint(&self) -> Fingerprint { self.source_fingerprint }
    fn snapshot(&self) -> &CtSnapshotRequest { self.snapshot }
    fn authenticate_input_table(&self, input: &CtInputTable) -> bool { input == self.input }
    fn authorize_symbol(&self, requester: &CtSymbolId, symbol: &CtSymbolId, _usage: CtSymbolUsage) -> bool {
        self.index.visible(requester, symbol)
    }
    fn authorize_query(&self, requester: &CtSymbolId, kind: CtQueryKind) -> bool {
        self.index.contains(requester)
            && self.snapshot.requested_queries.contains(&kind)
            && stage_allows(self.snapshot.ready, kind)
    }
}
