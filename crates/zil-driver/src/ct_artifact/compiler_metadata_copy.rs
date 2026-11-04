//! Copy qualification of metadata never asks for a serializable pointer value.
use super::*;
impl Compiler<'_> {
    pub(super) fn copy_metadata_nominal(
        &mut self,
        graph: &CtTypeRef,
        depth: usize,
        span: Span,
    ) -> R<()> {
        self.budget.depth(depth, span)?;
        for active in &self.active_copy_metadata {
            self.budget.metadata(active, span)?;
            self.budget.metadata(graph, span)?;
            if active == graph {
                return Err(error(
                    span,
                    "recursive nominal Copy metadata has no finite value layout",
                ));
            }
        }
        self.budget.metadata(graph, span)?;
        self.active_copy_metadata.push(graph.clone());
        let result = self.copy_metadata_nominal_inner(graph, depth, span);
        self.active_copy_metadata.pop();
        result
    }
    fn copy_metadata_nominal_inner(
        &mut self,
        graph: &CtTypeRef,
        depth: usize,
        span: Span,
    ) -> R<()> {
        let CtTypeNode::Nominal { symbol, arguments } = &graph.nodes[graph.root.0 as usize] else {
            unreachable!()
        };
        let (owner, item) = self.metadata_declaration(symbol, span)?;
        let parameters = match &item.kind {
            s::ItemKind::Aggregate(a) if a.kind == s::AggregateKind::Struct => &a.generics,
            s::ItemKind::Enum(e) => &e.generics,
            _ => {
                return Err(error(
                    span,
                    "Copy metadata is not an ordinary Value declaration",
                ));
            }
        };
        self.nominal_parameters(parameters, span)?;
        if parameters.len() != arguments.len() {
            return Err(error(span, "Copy nominal metadata argument count differs"));
        }
        let mut metadata = Vec::new();
        let mut scalars = Vec::new();
        for (parameter, argument) in parameters.iter().zip(arguments) {
            self.budget.text(&parameter.name, span)?;
            match (&parameter.kind, argument) {
                (s::GenericParamKind::Type(_), CtTypeArgument::Type(id)) => {
                    let graph = self.metadata_subgraph(graph, *id, span)?;
                    self.metadata_copy(&graph, depth + 1, span)?;
                    metadata.push((parameter.name.clone(), graph));
                }
                (s::GenericParamKind::Const(ty), CtTypeArgument::Scalar(value)) => {
                    self.budget.work(32, span)?;
                    if expr::scalar_constant(value)
                        != CtTy::Scalar(self.scalar_argument_type(ty, span)?)
                    {
                        return Err(error(span, "Copy nominal scalar metadata domain differs"));
                    }
                    scalars.push((parameter.name.clone(), value.clone()));
                }
                _ => return Err(error(span, "Copy nominal metadata argument kind differs")),
            }
        }
        let old_meta = std::mem::replace(&mut self.metadata_bindings, metadata);
        let old_data = std::mem::take(&mut self.bindings);
        let old_scalar = std::mem::replace(&mut self.scalar_bindings, scalars);
        let result = (|| {
            match &item.kind {
                s::ItemKind::Aggregate(a) => {
                    for field in &a.fields {
                        self.budget.node(field.span)?;
                        if field.manages {
                            return Err(error(
                                field.span,
                                "Copy metadata cannot manage a resource",
                            ));
                        }
                        let graph = self.metadata_type(owner, &field.ty, depth + 1)?;
                        self.metadata_copy(&graph, depth + 1, span)?;
                    }
                }
                s::ItemKind::Enum(e) => {
                    for variant in &e.variants {
                        self.budget.node(variant.span)?;
                        match &variant.fields {
                            s::VariantFields::Unit => {}
                            s::VariantFields::Tuple(fields) => {
                                for field in fields {
                                    self.budget.node(field.span)?;
                                    if field.manages {
                                        return Err(error(
                                            field.span,
                                            "Copy enum metadata cannot manage a resource",
                                        ));
                                    }
                                    let graph = self.metadata_type(owner, &field.ty, depth + 1)?;
                                    self.metadata_copy(&graph, depth + 1, span)?;
                                }
                            }
                            s::VariantFields::Record(fields) => {
                                for field in fields {
                                    self.budget.node(field.span)?;
                                    if field.manages {
                                        return Err(error(
                                            field.span,
                                            "Copy enum metadata cannot manage a resource",
                                        ));
                                    }
                                    let graph = self.metadata_type(owner, &field.ty, depth + 1)?;
                                    self.metadata_copy(&graph, depth + 1, span)?;
                                }
                            }
                        }
                    }
                }
                _ => unreachable!(),
            }
            Ok(())
        })();
        self.metadata_bindings = old_meta;
        self.bindings = old_data;
        self.scalar_bindings = old_scalar;
        result
    }
}
