//! Actual Core mint events. Source origins are carried by request-local AST
//! owners, never recovered from a span, generated spelling or matching shape.
use super::*;
#[derive(Clone, Debug)]
pub(super) struct SemanticFunctionEmission {
    pub marker: u32,
    pub function: NodeId,
}
#[derive(Clone, Debug)]
pub(crate) struct SemanticNodeEmission {
    pub marker: u32,
    pub creator: NodeId,
    pub node: NodeId,
    pub statement: bool,
}
impl Expander<'_> {
    pub(super) fn emit_semantic_node(
        &mut self,
        marker: s::SemanticContextMarker,
        node: NodeId,
        statement: bool,
        span: Span,
    ) {
        let Some(marker) = marker.raw_index() else {
            return;
        };
        let Some(creator) = self.current_function else {
            self.unsupported("semantic node has no actual Core creator", span);
            return;
        };
        if self.member_owner_work >= self.member_owner_limit || self.member_owner_nodes == 0 {
            self.diagnostics.push(Diagnostic::limit(
                Phase::Expand,
                "Source semantic node emission budget exceeded",
                Some(span),
            ));
            return;
        }
        self.member_owner_work += 1;
        self.member_owner_nodes -= 1;
        self.semantic_node_emissions.push(SemanticNodeEmission {
            marker,
            creator,
            node,
            statement,
        });
    }

    pub(super) fn emit_semantic_function(
        &mut self,
        marker: s::SemanticContextMarker,
        function: NodeId,
        span: Span,
    ) {
        let Some(marker) = marker.raw_index() else {
            return;
        };
        if self.member_owner_work >= self.member_owner_limit || self.member_owner_nodes == 0 {
            self.diagnostics.push(Diagnostic::limit(
                Phase::Expand,
                "Source semantic function emission budget exceeded",
                Some(span),
            ));
            return;
        }
        self.member_owner_work += 1;
        self.member_owner_nodes -= 1;
        self.semantic_function_emissions
            .push(SemanticFunctionEmission { marker, function });
    }
}
impl generics::products::Products {
    pub(crate) fn collect_semantic_nodes(
        &mut self,
        module: ModuleId,
        nodes: Vec<SemanticNodeEmission>,
    ) {
        if let Some(program) = self.semantic.as_mut() {
            program
                .nodes
                .extend(nodes.into_iter().map(|node| (module, node)));
        }
    }
    pub(crate) fn consume_semantic_nodes(
        &mut self,
        outputs: &[(ProgramModule, SourceMap)],
        limits: &Limits,
    ) -> CompileResult<()> {
        let Some(program) = self.semantic.as_mut() else {
            return Ok(());
        };
        let mut budget = generics::Budget::new(self.allowance.as_ref().unwrap_or(limits));
        let result = program.consume_nodes(outputs, &mut budget);
        self.allowance = Some(budget.remaining_limits());
        result.map_err(|e| vec![e])
    }

    pub(crate) fn bind_semantic_functions(
        &mut self,
        module: &s::Module,
        expanded: &mut ExpandedModule,
        limits: &Limits,
    ) -> CompileResult<()> {
        let Some(program) = self.semantic.as_mut() else {
            if !expanded.semantic_function_emissions.is_empty() {
                return Err(vec![Diagnostic::error(
                    Phase::Expand,
                    "E-SOURCE-SEMANTIC-EMISSION",
                    Some(module.span),
                    "semantic AST owner has no actual request producer",
                )]);
            }
            return Ok(());
        };
        let mut budget = generics::Budget::new(self.allowance.as_ref().unwrap_or(limits));
        let result = (|| {
            for emission in std::mem::take(&mut expanded.semantic_function_emissions) {
                program.bind_function(
                    emission.marker,
                    module.id,
                    emission.function,
                    &mut budget,
                    module.span,
                )?;
            }
            Ok(())
        })();
        self.allowance = Some(budget.remaining_limits());
        result.map_err(|e| vec![e])
    }
    pub(crate) fn publish_semantics(
        &mut self,
        cores: &mut [(&mut CoreModule, &mut SourceMap)],
        limits: &Limits,
    ) -> CompileResult<()> {
        let Some(program) = self.semantic.as_mut() else {
            return Ok(());
        };
        let mut budget = generics::Budget::new(self.allowance.as_ref().unwrap_or(limits));
        let result = program.publish(cores, &mut budget);
        self.allowance = Some(budget.remaining_limits());
        result.map_err(|e| vec![e])
    }
}
