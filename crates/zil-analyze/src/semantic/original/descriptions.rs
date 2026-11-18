//! A contextual weak argument keeps its original owner and physical path.
//! The lexical occupant stays owning; this view supplies neither ownership nor
//! permission to an unrelated same-typed resource.
use super::*;
pub(super) struct Description {
    pub(super) role: ViewSelection,
    pub(super) anchor: LocalAnchor,
    pub(super) root: Term,
    pub(super) path: Vec<u32>,
    pub(super) source: Term,
    pub(super) output: Term,
}
pub(in crate::semantic) struct Source {
    pub(in crate::semantic) role: (crate::semantic::value::Class, Option<crate::semantic::value::Class>, crate::semantic::value::Class, Option<LocalAnchor>),
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
        self.describe_argument_role(frame, value, place, source, None, b, depth)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn describe_argument_role(
        &mut self,
        frame: &mut Frame,
        value: &SemanticOriginalExpr,
        place: &places::Place,
        source: Term,
        wanted: Option<Term>,
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
        let occurrence = self.term(Kind::Life(Life::Implicit(frame.definition, row)), b)?;
        let upper = self.description_upper(place, source, b, depth + 1)?;
        self.remember_view_upper(frame.definition, row, upper, b)?;
        let life = self.select_view_life(frame, occurrence, upper, wanted, None, b, depth + 1)?;
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
                    role: ViewSelection { local: None, occurrence, upper, selected: life },
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

#[derive(Clone, Copy)]
pub(in crate::semantic) struct Temporary {
    pub(in crate::semantic) producer: NodeId,
    source: Term,
    view: Term,
}

impl Solver<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn temporary_description_argument(
        &mut self,
        frame: &mut Frame,
        call: NodeId,
        ordinal: usize,
        argument: &SemanticOriginalExpr,
        expected: Term,
        prepared: Option<Term>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<Term>> {
        if !matches!(self.state.arena[self.type_root(expected, b)?], Kind::Desc { .. }) {
            return Ok(None);
        }
        b.lookup(self.call_parameters.len(), WORK)?;
        if !self.call_parameters.contains(&expected)
            || prepared.is_none() && self.place(frame, argument, false, b, depth + 1)?.is_some()
        {
            return Ok(None);
        }
        let source = match prepared {
            Some(source) => source,
            None => self.expression(frame, argument, b, depth + 1)?,
        };
        self.prepare_call_value(frame.definition, argument.id, source, b)?;
        let deferred = self.deferred_call_type(expected, b)?;
        let Kind::Wrapped(Wrapper::Managed, payload) =
            self.state.arena[self.type_root(source, b)?]
        else {
            self.call_argument_value(source, deferred, b, depth + 1)?;
            return Ok(Some(source));
        };
        let ordinal_u32 = u32::try_from(ordinal)
            .map_err(|_| resource("original temporary description parameter ordinal overflows"))?;
        let life = self.call_occurrence(
            frame, call, call_regions::Role::TemporaryDescription(ordinal_u32), None, b,
        )?;
        let view = self.term(Kind::Desc { life, inner: payload }, b)?;
        self.call_argument_value(view, deferred, b, depth + 1)?;
        let mut producer = argument;
        while let CoreExprKind::Group(inner) | CoreExprKind::Ascribed { value: inner, .. } = &producer.kind {
            b.spend(1, WORK)?;
            producer = inner;
        }
        b.lookup(self.state.temporary_descriptions.len(), WORK)?;
        if self.state.temporary_descriptions.insert(
            (frame.definition, call, ordinal), Temporary { producer: producer.id, source, view },
        ).is_some() {
            return Err(fail("original temporary description repeats one argument occurrence"));
        }
        Ok(Some(view))
    }
}

impl Checked {
    pub(in crate::semantic) fn has_temporary_description(
        &self, context: &SemanticInstance, site: SemanticSiteId, ordinal: usize,
        inventory: &Inventory<'_>, b: &mut Budget,
    ) -> CompileResult<bool> {
        if self.temporary_descriptions.is_empty() { return Ok(false); }
        let node = self.intrinsic_node(context, site, inventory, b)?;
        b.lookup(self.temporary_descriptions.len(), WORK)?;
        Ok(self.temporary_descriptions.contains_key(&(context.definition, node, ordinal)))
    }

    pub(in crate::semantic) fn temporary_descriptions(
        &self,
        owner: SemanticDefinitionId,
        b: &mut Budget,
    ) -> CompileResult<Vec<(NodeId, usize, Temporary)>> {
        b.lookup(self.temporary_descriptions.len(), WORK)?;
        let mut rows = Vec::new();
        for (&(definition, call, ordinal), &source) in self.temporary_descriptions
            .range((owner, NodeId(0), 0)..)
        {
            if definition != owner {
                break;
            }
            Solver::push(&mut rows, (call, ordinal, source), b)?;
        }
        Ok(rows)
    }

    pub(in crate::semantic) fn temporary_description_values(
        &self, context: &SemanticInstance, description: Temporary, program: &SemanticModule,
        inventory: &Inventory<'_>, b: &mut Budget, depth: usize,
    ) -> CompileResult<(value::Value, value::Value)> {
        Ok((
            self.value(description.source, context, program, inventory, b, depth + 1)?,
            self.value(description.view, context, program, inventory, b, depth + 1)?,
        ))
    }
}
