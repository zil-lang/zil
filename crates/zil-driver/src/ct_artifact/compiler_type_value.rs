//! Reconstruct Type values from complete metadata; no metadata node is a value pointer.
use super::*;
use cfg::{Builder, Value};
impl Builder<'_, '_> {
    pub(super) fn metadata_type_value(
        &mut self,
        graph: &CtTypeRef,
        id: CtTypeNodeId,
        depth: usize,
        span: Span,
    ) -> R<Value> {
        self.compiler.budget.depth(depth, span)?;
        self.compiler.budget.node(span)?;
        if id == graph.root
            && (!graph.ambient_classes.is_empty()
                || matches!(
                    graph.nodes.get(id.0 as usize),
                    Some(CtTypeNode::BorrowedStorage { .. })
                ))
        {
            self.compiler.budget.metadata(graph, span)?;
            let value = self.emit(
                CtInstructionKind::TypeInContext { ty: graph.clone() },
                CtTy::Type,
                span,
            )?;
            self.record_source_type_site(value.id, span)?;
            return Ok(value);
        }
        let node = graph
            .nodes
            .get(id.0 as usize)
            .ok_or_else(|| error(span, "Type metadata node is absent"))?;
        let mut arguments = Vec::new();
        let constructor = match node {
            CtTypeNode::BorrowedStorage { .. } => {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Ct,
                    "CT4 scoped Type value construction is not yet implemented",
                    Some(span),
                )]);
            }
            CtTypeNode::Primitive(s) => {
                return self.emit(CtInstructionKind::TypePrimitive(*s), CtTy::Type, span);
            }
            CtTypeNode::Builtin(b) => {
                return self.emit(CtInstructionKind::TypeBuiltin(*b), CtTy::Type, span);
            }
            CtTypeNode::Nominal {
                symbol,
                arguments: args,
            } => {
                for arg in args {
                    self.compiler.budget.node(span)?;
                    let value = match arg {
                        CtTypeArgument::Type(id) => {
                            self.metadata_type_value(graph, *id, depth + 1, span)?
                        }
                        CtTypeArgument::Lifetime(_) => {
                            return Err(vec![Diagnostic::unsupported(
                                Phase::Ct,
                                "CT4 scoped Type argument construction is not yet implemented",
                                Some(span),
                            )]);
                        }
                        CtTypeArgument::Scalar(c) => self.constant(
                            CtConstant::Scalar(c.clone()),
                            expr::scalar_constant(c),
                            span,
                        )?,
                    };
                    arguments.push(value.id);
                }
                let instruction = if arguments.is_empty() {
                    CtInstructionKind::TypeName(symbol.clone())
                } else {
                    CtInstructionKind::TypeApply {
                        nominal: symbol.clone(),
                        arguments,
                    }
                };
                return self.emit(instruction, CtTy::Type, span);
            }
            CtTypeNode::Tuple(parts) | CtTypeNode::AnonymousSum(parts) => {
                for id in parts {
                    arguments.push(self.metadata_type_value(graph, *id, depth + 1, span)?.id);
                }
                if matches!(node, CtTypeNode::Tuple(_)) {
                    CtTypeConstructor::Tuple
                } else {
                    CtTypeConstructor::AnonymousSum
                }
            }
            CtTypeNode::Array { element, length } => {
                arguments.push(
                    self.metadata_type_value(graph, *element, depth + 1, span)?
                        .id,
                );
                arguments.push(
                    self.constant(
                        CtConstant::Scalar(Constant::U64(u64::from(*length))),
                        CtTy::Scalar(CtScalarTy::U64),
                        span,
                    )?
                    .id,
                );
                CtTypeConstructor::Array
            }
            CtTypeNode::FunctionPointer { parameters, result }
            | CtTypeNode::BorrowedFunctionPointer {
                parameters, result, ..
            } => {
                for id in parameters.iter().chain(std::iter::once(result)) {
                    arguments.push(self.metadata_type_value(graph, *id, depth + 1, span)?.id);
                }
                if let CtTypeNode::BorrowedFunctionPointer { life, .. } = node {
                    self.compiler.budget.signature_metadata(life, span)?;
                    CtTypeConstructor::BorrowedFunctionPointer { life: life.clone() }
                } else {
                    CtTypeConstructor::FunctionPointer
                }
            }
            CtTypeNode::Address { mutable, inner } => {
                arguments.push(self.metadata_type_value(graph, *inner, depth + 1, span)?.id);
                CtTypeConstructor::Address { mutable: *mutable }
            }
            CtTypeNode::Slice(inner)
            | CtTypeNode::Managed(inner)
            | CtTypeNode::Desc(inner)
            | CtTypeNode::OwnedStore(inner) => {
                arguments.push(self.metadata_type_value(graph, *inner, depth + 1, span)?.id);
                match node {
                    CtTypeNode::Slice(_) => CtTypeConstructor::Slice,
                    CtTypeNode::Managed(_) => CtTypeConstructor::Managed,
                    CtTypeNode::Desc(_) => CtTypeConstructor::Desc,
                    _ => CtTypeConstructor::OwnedStore,
                }
            }
            CtTypeNode::ErrorUnion { error, result } => {
                arguments.push(self.metadata_type_value(graph, *error, depth + 1, span)?.id);
                arguments.push(
                    self.metadata_type_value(graph, *result, depth + 1, span)?
                        .id,
                );
                CtTypeConstructor::ErrorUnion
            }
            CtTypeNode::Opaque { identity } => {
                return self.emit(
                    CtInstructionKind::TypeName(identity.clone()),
                    CtTy::Type,
                    span,
                );
            }
            CtTypeNode::ForeignCallback { .. } => {
                return Err(error(
                    span,
                    "foreign callback metadata requires its own closed Type constructor",
                ));
            }
        };
        self.emit(
            CtInstructionKind::TypeConstruct {
                constructor,
                arguments,
            },
            CtTy::Type,
            span,
        )
    }
}
