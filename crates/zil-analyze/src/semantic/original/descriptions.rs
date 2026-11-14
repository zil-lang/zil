//! A contextual weak argument keeps its original owner and physical path.
//! The lexical occupant stays owning; this view supplies neither ownership nor
//! permission to an unrelated same-typed resource.
use super::*;
pub(super) struct Description {
    pub(super) anchor: LocalAnchor,
    pub(super) root: Term,
    pub(super) path: Vec<u32>,
    pub(super) source: Term,
    pub(super) output: Term,
}
pub(in crate::semantic) struct Source {
    pub(in crate::semantic) anchor: LocalAnchor,
    pub(in crate::semantic) root: crate::semantic::value::Value,
    pub(in crate::semantic) path: Vec<u32>,
    pub(in crate::semantic) source: crate::semantic::value::Value,
    pub(in crate::semantic) output: crate::semantic::value::Value,
}
impl Solver<'_, '_> {
    pub(super) fn describe_argument(
        &mut self,
        frame: &mut Frame,
        value: &SemanticOriginalExpr,
        place: &places::Place,
        source: Term,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        let source = self.type_root(source, b)?;
        let Kind::Wrapped(Wrapper::Managed, payload) = self.state.arena[source] else {
            return Err(fail(
                "original description argument has no actual owning source category",
            ));
        };
        let anchor = place.anchor.ok_or_else(|| {
            unknown("original description argument lacks its exact owning place anchor")
        })?;
        let row = self
            .borrow_occurrence(frame.definition, value.id, b)
            .map_err(|mut errors| {
                for error in &mut errors {
                    error.notes.push(format!(
                        "original description owner {}:{} argument expression {}",
                        frame.definition.module.0, frame.definition.index, value.id.0
                    ));
                }
                errors
            })?;
        let life = self.term(Kind::Life(Life::Implicit(frame.definition, row)), b)?;
        let output = self.term(
            Kind::Desc {
                life,
                inner: payload,
            },
            b,
        )?;
        b.spend(place.path.len(), WORK)?;
        let path = place.path.clone();
        b.lookup(self.state.descriptions.len(), WORK)?;
        if self
            .state
            .descriptions
            .insert(
                (frame.definition, value.id),
                Description {
                    anchor,
                    root: place.root,
                    path,
                    source,
                    output,
                },
            )
            .is_some()
        {
            return Err(fail(
                "original description argument repeats a source occurrence",
            ));
        }
        b.lookup(frame.expressions.len(), WORK)?;
        frame.expressions.insert(value.id, output);
        Ok(output)
    }
}
