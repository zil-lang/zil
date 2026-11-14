//! A pattern binding retains its actual subject and discriminator path. This
//! directory supplies identities only; initialized tags and loans are checked
//! by the actual storage kernels.
use super::*;

pub(in crate::semantic) struct PatternSubject {
    pub(in crate::semantic) node: NodeId,
    pub(in crate::semantic) anchor: Option<LocalAnchor>,
    pub(in crate::semantic) path: Vec<u32>,
}
pub(in crate::semantic) struct PatternOrigin {
    pub(in crate::semantic) owner: SemanticDefinitionId,
    pub(in crate::semantic) subject: PatternSubject,
    pub(in crate::semantic) path: Vec<PlaceProjection>,
}
impl Solver<'_, '_> {
    pub(super) fn pattern_subject(
        &mut self,
        frame: &mut Frame,
        source: &SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<PatternSubject> {
        let place = self.place(frame, source, false, b, depth + 1)?;
        Ok(PatternSubject {
            node: source.id,
            anchor: place.as_ref().and_then(|place| place.anchor),
            path: place.map(|place| place.path).unwrap_or_default(),
        })
    }
    pub(super) fn pattern_anchor(
        &mut self,
        owner: SemanticDefinitionId,
        source: &PatternSubject,
        path: &[PlaceProjection],
        b: &mut Budget,
    ) -> CompileResult<LocalAnchor> {
        b.spend(source.path.len().saturating_add(path.len() + 1), WORK)?;
        let index = u32::try_from(self.state.pattern_origins.len())
            .map_err(|_| resource("original pattern origin ordinal overflow"))?;
        Self::push(
            &mut self.state.pattern_origins,
            PatternOrigin {
                owner,
                subject: PatternSubject {
                    node: source.node,
                    anchor: source.anchor,
                    path: source.path.clone(),
                },
                path: path.to_vec(),
            },
            b,
        )?;
        Ok(LocalAnchor::Pattern(index))
    }
}
impl Checked {
    pub(in crate::semantic) fn pattern_origin(
        &self,
        ordinal: u32,
        owner: SemanticDefinitionId,
        b: &mut Budget,
    ) -> CompileResult<&PatternOrigin> {
        b.lookup(self.pattern_origins.len(), WORK)?;
        self.pattern_origins
            .get(ordinal as usize)
            .filter(|origin| origin.owner == owner)
            .ok_or_else(|| fail("pattern origin belongs to another original definition"))
    }
}
