//! Closed nominal metadata is independent of the pure values stored in fields.
use super::*;
impl<'a> Compiler<'a> {
    pub(super) fn metadata_subgraph(
        &mut self,
        graph: &CtTypeRef,
        root: CtTypeNodeId,
        span: Span,
    ) -> R<CtTypeRef> {
        self.budget.metadata(graph, span)?;
        let mut graph = graph.clone();
        graph.root = root;
        self.meta_normalize(graph, span)
    }
    pub(super) fn metadata_data_type(
        &mut self,
        graph: &CtTypeRef,
        depth: usize,
        span: Span,
    ) -> R<CtTy> {
        self.budget.depth(depth, span)?;
        let node = graph
            .nodes
            .get(graph.root.0 as usize)
            .ok_or_else(|| error(span, "metadata root is absent"))?;
        Ok(match node {
            CtTypeNode::Primitive(s) => CtTy::Scalar(*s),
            CtTypeNode::Tuple(parts) | CtTypeNode::AnonymousSum(parts) => {
                let mut fields = Vec::new();
                for id in parts {
                    let part = self.metadata_subgraph(graph, *id, span)?;
                    fields.push(self.metadata_data_type(&part, depth + 1, span)?);
                }
                if matches!(node, CtTypeNode::Tuple(_)) {
                    CtTy::Tuple(fields)
                } else {
                    self.canonical_data_sum(fields, span)?
                }
            }
            CtTypeNode::Array { element, length } => {
                let element = self.metadata_subgraph(graph, *element, span)?;
                CtTy::Array {
                    element: Box::new(self.metadata_data_type(&element, depth + 1, span)?),
                    length: *length,
                }
            }
            CtTypeNode::Nominal { symbol, arguments } => {
                let (owner, item) = self.metadata_declaration(symbol, span)?;
                let mut args = Vec::new();
                for arg in arguments {
                    self.budget.node(span)?;
                    args.push(match arg {
                        CtTypeArgument::Type(id) => {
                            CtSchemaArgument::Type(self.metadata_subgraph(graph, *id, span)?)
                        }
                        CtTypeArgument::Scalar(value) => CtSchemaArgument::Scalar(value.clone()),
                    });
                }
                self.closed_nominal(owner, item, args, depth + 1, span)?
            }
            _ => {
                return Err(error(
                    span,
                    "ordinary metadata is not a serializable CT field value",
                ));
            }
        })
    }
    fn metadata_declaration(&mut self, symbol: &CtSymbolId, span: Span) -> R<(usize, &'a s::Item)> {
        for (owner, module) in self.modules.iter().enumerate() {
            self.budget.work(1, span)?;
            if module.source.module_id != symbol.module {
                continue;
            }
            for (actual_span, actual) in module.symbols {
                self.budget.work(1, span)?;
                if actual != symbol {
                    continue;
                }
                for item in &module.syntax.items {
                    self.budget.work(1, span)?;
                    if item.span == *actual_span {
                        return Ok((owner, item));
                    }
                }
            }
        }
        Err(error(
            span,
            "metadata nominal is not a frozen source declaration",
        ))
    }
    fn schema_arguments_cost(&mut self, args: &[CtSchemaArgument], span: Span) -> R<()> {
        for arg in args {
            self.budget.node(span)?;
            match arg {
                CtSchemaArgument::Type(graph) => self.budget.metadata(graph, span)?,
                CtSchemaArgument::Scalar(_) => self.budget.work(32, span)?,
            }
        }
        Ok(())
    }
    pub(super) fn source_nominal_arguments(
        &mut self,
        module: usize,
        parameters: &[s::GenericParam],
        args: &[s::GenericArg],
        span: Span,
        depth: usize,
    ) -> R<Vec<CtSchemaArgument>> {
        let names = self.generic_names(parameters, &[], span)?;
        if names.len() != args.len() {
            return Err(error(span, "CT nominal argument count differs"));
        }
        let mut out = Vec::new();
        for arg in args {
            let s::GenericArg::Type(ty) = arg else {
                return Err(error(
                    span,
                    "CT nominal scalar argument needs a declared scalar binding",
                ));
            };
            let graph = self.metadata_type(module, ty, depth + 1)?;
            self.metadata_copy(&graph, depth + 1, span)?;
            out.push(CtSchemaArgument::Type(graph));
        }
        Ok(out)
    }
    fn metadata_copy(&mut self, graph: &CtTypeRef, depth: usize, span: Span) -> R<()> {
        self.budget.depth(depth, span)?;
        match graph
            .nodes
            .get(graph.root.0 as usize)
            .ok_or_else(|| error(span, "Copy metadata root is absent"))?
        {
            CtTypeNode::FunctionPointer { .. } | CtTypeNode::BorrowedFunctionPointer { .. } => {
                Ok(())
            }
            _ => {
                let data = self.metadata_data_type(graph, depth + 1, span)?;
                self.ordinary_copy(&data, depth + 1, span)
            }
        }
    }
    pub(super) fn closed_nominal(
        &mut self,
        owner: usize,
        item: &'a s::Item,
        arguments: Vec<CtSchemaArgument>,
        depth: usize,
        span: Span,
    ) -> R<CtTy> {
        self.budget.depth(depth, span)?;
        let parameters = match &item.kind {
            s::ItemKind::Aggregate(a) if a.kind == s::AggregateKind::Struct => &a.generics,
            s::ItemKind::Enum(e) => &e.generics,
            _ => {
                return Err(forbidden(
                    span,
                    "CT data nominal is not an ordinary pure struct or enum",
                ));
            }
        };
        let names = self.generic_names(parameters, &[], span)?;
        if names.len() != arguments.len() {
            return Err(error(span, "closed nominal argument count differs"));
        }
        let mut metadata_bindings = Vec::new();
        for (name, argument) in names.into_iter().zip(&arguments) {
            let CtSchemaArgument::Type(graph) = argument else {
                return Err(error(span, "closed nominal scalar binding is not complete"));
            };
            self.metadata_copy(graph, depth + 1, span)?;
            self.budget.metadata(graph, span)?;
            metadata_bindings.push((name, graph.clone()));
        }
        let name = self.symbol(owner, item.span)?;
        let record = matches!(item.kind, s::ItemKind::Aggregate(_));
        let len = if record {
            self.records.len()
        } else {
            self.enums.len()
        };
        for id in 0..len {
            self.budget.work(1, span)?;
            let (same, old_args) = if record {
                (self.records[id].name == name, &self.records[id].arguments)
            } else {
                (self.enums[id].name == name, &self.enums[id].arguments)
            };
            if same {
                // Prepay both complete graphs before their structural equality.
                for arg in old_args {
                    self.budget.node(span)?;
                    if let CtSchemaArgument::Type(graph) = arg {
                        self.budget.metadata(graph, span)?;
                    }
                }
                self.schema_arguments_cost(&arguments, span)?;
                let old_args = if record {
                    &self.records[id].arguments
                } else {
                    &self.enums[id].arguments
                };
                if *old_args == arguments {
                    return Ok(if record {
                        CtTy::Record(CtRecordSchemaId(id as u32))
                    } else {
                        CtTy::Enum(CtEnumSchemaId(id as u32))
                    });
                }
            }
        }
        self.budget.node(span)?;
        let id = if record {
            let s::ItemKind::Aggregate(a) = &item.kind else {
                unreachable!()
            };
            self.record_sources.push((owner, a));
            // Only symbolic definitions use the legacy CtTy actual vector.
            self.record_arguments.push(Vec::new());
            self.records.push(CtRecordSchema {
                name,
                arguments,
                fields: Vec::new(),
                origin: CtSchemaOrigin::UserPure,
            });
            CtTy::Record(CtRecordSchemaId((self.records.len() - 1) as u32))
        } else {
            let s::ItemKind::Enum(e) = &item.kind else {
                unreachable!()
            };
            self.enum_sources.push((owner, e));
            self.enums.push(CtEnumSchema {
                name,
                arguments,
                variants: Vec::new(),
                origin: CtSchemaOrigin::UserPure,
            });
            CtTy::Enum(CtEnumSchemaId((self.enums.len() - 1) as u32))
        };
        let old_meta = std::mem::replace(&mut self.metadata_bindings, metadata_bindings);
        let old_data = std::mem::take(&mut self.bindings);
        let result = (|| {
            match (&item.kind, &id) {
                (s::ItemKind::Aggregate(a), CtTy::Record(id)) => {
                    let mut fields = Vec::new();
                    for field in &a.fields {
                        self.budget.node(field.span)?;
                        if field.manages {
                            return Err(forbidden(
                                field.span,
                                "CT data cannot manage a runtime resource",
                            ));
                        }
                        fields.push(self.ty(owner, &field.ty, depth + 1)?);
                    }
                    self.records[id.0 as usize].fields = fields;
                }
                (s::ItemKind::Enum(e), CtTy::Enum(id)) => {
                    let mut variants = Vec::new();
                    for variant in &e.variants {
                        self.budget.node(variant.span)?;
                        let mut fields = Vec::new();
                        match &variant.fields {
                            s::VariantFields::Unit => {}
                            s::VariantFields::Tuple(fs) => {
                                for field in fs {
                                    self.budget.node(field.span)?;
                                    if field.manages {
                                        return Err(forbidden(
                                            field.span,
                                            "CT enum cannot manage a runtime resource",
                                        ));
                                    }
                                    fields.push(self.ty(owner, &field.ty, depth + 1)?);
                                }
                            }
                            s::VariantFields::Record(fs) => {
                                for field in fs {
                                    self.budget.node(field.span)?;
                                    if field.manages {
                                        return Err(forbidden(
                                            field.span,
                                            "CT enum cannot manage a runtime resource",
                                        ));
                                    }
                                    fields.push(self.ty(owner, &field.ty, depth + 1)?);
                                }
                            }
                        }
                        variants.push(fields);
                    }
                    self.enums[id.0 as usize].variants = variants;
                }
                _ => unreachable!(),
            }
            Ok(id)
        })();
        self.bindings = old_data;
        self.metadata_bindings = old_meta;
        result
    }
    pub(super) fn canonical_data_sum(&mut self, members: Vec<CtTy>, span: Span) -> R<CtTy> {
        let mut meter = zil_ct::CtExecutionBudget::new(self.budget.declared());
        let result =
            zil_ct::canonical_artifact_sum(members, &self.records, &self.enums, &mut meter);
        let usage = meter.usage();
        self.budget.work(
            usize::try_from(usage.analysis_work).unwrap_or(usize::MAX),
            span,
        )?;
        Budget::take(
            &mut self.budget.nodes,
            usize::try_from(usage.generated_nodes).unwrap_or(usize::MAX),
            span,
            "CT sum identity nodes exhausted",
        )?;
        Budget::take(
            &mut self.budget.bytes,
            usize::try_from(usage.peak_temporary_bytes).unwrap_or(usize::MAX),
            span,
            "CT sum identity bytes exhausted",
        )?;
        result
    }
}
