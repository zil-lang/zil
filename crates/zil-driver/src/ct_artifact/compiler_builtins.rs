use super::*;
use cfg::{Builder, Value};
impl Builder<'_, '_> {
    pub(super) fn builtin(
        &mut self,
        module: ImportTarget,
        name: &str,
        args: &[s::Argument],
        depth: usize,
        span: Span,
    ) -> R<Value> {
        let mut actual = Vec::new();
        for arg in args {
            self.compiler.budget.node(arg.span)?;
            actual.push(self.expr(&arg.value, None, depth + 1)?);
        }
        let ids = actual.iter().map(|v| v.id).collect::<Vec<_>>();
        let exact = |types: &[CtTy]| {
            actual.len() == types.len() && actual.iter().zip(types).all(|(v, t)| v.ty == *t)
        };
        let all_type = || actual.iter().all(|v| v.ty == CtTy::Type);
        let integer = |v: &Value| {
            matches!(
                v.ty,
                CtTy::Scalar(CtScalarTy::U8 | CtScalarTy::U16 | CtScalarTy::I32 | CtScalarTy::I64)
            )
        };
        match module {
            ImportTarget::Meta => {
                let constructor = match name {
                    "array" | "array_type" => Some(CtTypeConstructor::Array),
                    "tuple" | "tuple_type" => Some(CtTypeConstructor::Tuple),
                    "function_pointer" => Some(CtTypeConstructor::FunctionPointer),
                    "address" => Some(CtTypeConstructor::Address { mutable: false }),
                    "address_mut" => Some(CtTypeConstructor::Address { mutable: true }),
                    "slice" => Some(CtTypeConstructor::Slice),
                    "managed" => Some(CtTypeConstructor::Managed),
                    "desc" => Some(CtTypeConstructor::Desc),
                    "store" => Some(CtTypeConstructor::OwnedStore),
                    "error_union" => Some(CtTypeConstructor::ErrorUnion),
                    _ => None,
                };
                if let Some(constructor) = constructor {
                    let valid = match constructor {
                        CtTypeConstructor::Array => {
                            actual.len() == 2 && actual[0].ty == CtTy::Type && integer(&actual[1])
                        }
                        CtTypeConstructor::Tuple => all_type(),
                        CtTypeConstructor::FunctionPointer => !actual.is_empty() && all_type(),
                        CtTypeConstructor::ErrorUnion => actual.len() == 2 && all_type(),
                        _ => actual.len() == 1 && all_type(),
                    };
                    if !valid {
                        return Err(error(
                            span,
                            "actual type-constructor arguments differ from the CT standard signature",
                        ));
                    }
                    return self.emit(
                        CtInstructionKind::TypeConstruct {
                            constructor,
                            arguments: ids,
                        },
                        CtTy::Type,
                        span,
                    );
                }
                let query = match name {
                    "shape" => Some((CtQueryKind::Shape, CtTy::ShapeInfo)),
                    "field" => Some((CtQueryKind::Field, CtTy::FieldInfo)),
                    "impls" | "impl_index" => {
                        Some((CtQueryKind::ImplIndex, CtTy::List(Box::new(CtTy::ImplInfo))))
                    }
                    "layout" => Some((CtQueryKind::Layout, CtTy::LayoutInfo)),
                    _ => None,
                };
                if let Some((kind, ty)) = query {
                    let valid = match kind {
                        CtQueryKind::Field => exact(&[CtTy::Type, CtTy::Text]),
                        CtQueryKind::ImplIndex => !actual.is_empty() && all_type(),
                        _ => exact(&[CtTy::Type]),
                    };
                    if !valid {
                        return Err(error(
                            span,
                            "actual reflection query arguments differ from the standard signature",
                        ));
                    }
                    return self.emit(
                        CtInstructionKind::Query {
                            kind,
                            arguments: ids,
                        },
                        ty,
                        span,
                    );
                }
                let projection = match name {
                    "fields" | "shape_fields" => Some((
                        CtMetaProjection::ShapeFields,
                        CtTy::ShapeInfo,
                        CtTy::List(Box::new(CtTy::FieldInfo)),
                    )),
                    "complete" | "shape_complete" => Some((
                        CtMetaProjection::ShapeComplete,
                        CtTy::ShapeInfo,
                        CtTy::Scalar(CtScalarTy::Bool),
                    )),
                    "field_name" => {
                        Some((CtMetaProjection::FieldName, CtTy::FieldInfo, CtTy::Ident))
                    }
                    "field_type" => {
                        Some((CtMetaProjection::FieldType, CtTy::FieldInfo, CtTy::Type))
                    }
                    "field_managed" => Some((
                        CtMetaProjection::FieldManaged,
                        CtTy::FieldInfo,
                        CtTy::Scalar(CtScalarTy::Bool),
                    )),
                    "impl_type" => Some((CtMetaProjection::ImplType, CtTy::ImplInfo, CtTy::Type)),
                    "size" | "layout_size" => Some((
                        CtMetaProjection::LayoutSize,
                        CtTy::LayoutInfo,
                        CtTy::Scalar(CtScalarTy::I64),
                    )),
                    "alignment" | "layout_alignment" => Some((
                        CtMetaProjection::LayoutAlignment,
                        CtTy::LayoutInfo,
                        CtTy::Scalar(CtScalarTy::I64),
                    )),
                    "field_offsets" => Some((
                        CtMetaProjection::LayoutFieldOffsets,
                        CtTy::LayoutInfo,
                        CtTy::List(Box::new(CtTy::Scalar(CtScalarTy::I64))),
                    )),
                    _ => None,
                };
                if let Some((projection, input, ty)) = projection {
                    if !exact(&[input]) {
                        return Err(error(span, "actual reflection projection input differs"));
                    }
                    return self.emit(
                        CtInstructionKind::MetaProject {
                            value: ids[0],
                            projection,
                        },
                        ty,
                        span,
                    );
                }
            }
            ImportTarget::Syntax => {
                if name == "parse_ident" {
                    if !exact(&[CtTy::TokenTree]) {
                        return Err(error(
                            span,
                            "syntax.parse_ident consumes one actual input TokenTree",
                        ));
                    }
                    return self.emit(
                        CtInstructionKind::ParseIdent { input: ids[0] },
                        CtTy::Ident,
                        span,
                    );
                }
                let syntax = match name {
                    "literal" => Some((
                        CtSyntaxBuilder::Literal,
                        CtTy::Syntax(CtSyntaxCategory::Expr),
                    )),
                    "type_ref" => Some((
                        CtSyntaxBuilder::TypeRef,
                        CtTy::Syntax(CtSyntaxCategory::Type),
                    )),
                    "name_expr" => Some((
                        CtSyntaxBuilder::NameExpr,
                        CtTy::Syntax(CtSyntaxCategory::Expr),
                    )),
                    _ => None,
                };
                if let Some((kind, ty)) = syntax {
                    let valid = match kind {
                        CtSyntaxBuilder::Literal => {
                            actual.len() == 1
                                && matches!(actual[0].ty, CtTy::Scalar(_) | CtTy::Text)
                        }
                        CtSyntaxBuilder::TypeRef => exact(&[CtTy::Type]),
                        CtSyntaxBuilder::NameExpr => exact(&[CtTy::Ident]),
                    };
                    if !valid {
                        return Err(error(span, "actual syntax builder arguments differ"));
                    }
                    return self.emit(
                        CtInstructionKind::SyntaxBuild {
                            kind,
                            arguments: ids,
                        },
                        ty,
                        span,
                    );
                }
                if matches!(name, "fresh" | "export") {
                    if !exact(&[CtTy::Text]) {
                        return Err(error(span, "name constructor requires one CT Text"));
                    }
                    return self.emit(
                        CtInstructionKind::NameBuild {
                            kind: if name == "fresh" {
                                CtNameBuilder::Fresh
                            } else {
                                CtNameBuilder::Export
                            },
                            text: ids[0],
                        },
                        CtTy::Ident,
                        span,
                    );
                }
                let category = match name {
                    "parse_expr" => Some(CtSyntaxCategory::Expr),
                    "parse_type" => Some(CtSyntaxCategory::Type),
                    "parse_stmts" | "parse_stmt_list" => Some(CtSyntaxCategory::StmtList),
                    "parse_items" | "parse_item_list" => Some(CtSyntaxCategory::ItemList),
                    _ => None,
                };
                if let Some(category) = category {
                    if !exact(&[CtTy::TokenTree]) {
                        return Err(error(
                            span,
                            "syntax.parse consumes exactly one lossless TokenTree",
                        ));
                    }
                    return self.emit(
                        CtInstructionKind::Parse {
                            category,
                            input: ids[0],
                        },
                        CtTy::Syntax(category),
                        span,
                    );
                }
                let token = match name {
                    "root_count" => Some((
                        CtTokenProjection::RootCount,
                        CtTy::TokenTree,
                        CtTy::Scalar(CtScalarTy::I64),
                        false,
                    )),
                    "root_at" => Some((
                        CtTokenProjection::RootAt,
                        CtTy::TokenTree,
                        CtTy::TokenCursor,
                        true,
                    )),
                    "token_class" => Some((
                        CtTokenProjection::Class,
                        CtTy::TokenCursor,
                        CtTy::Scalar(CtScalarTy::I32),
                        false,
                    )),
                    "token_text" => Some((
                        CtTokenProjection::Text,
                        CtTy::TokenCursor,
                        CtTy::Text,
                        false,
                    )),
                    "delimiter" => Some((
                        CtTokenProjection::Delimiter,
                        CtTy::TokenCursor,
                        CtTy::Scalar(CtScalarTy::I32),
                        false,
                    )),
                    "child_count" => Some((
                        CtTokenProjection::ChildCount,
                        CtTy::TokenCursor,
                        CtTy::Scalar(CtScalarTy::I64),
                        false,
                    )),
                    "child_at" => Some((
                        CtTokenProjection::ChildAt,
                        CtTy::TokenCursor,
                        CtTy::TokenCursor,
                        true,
                    )),
                    "input_name" => Some((
                        CtTokenProjection::InputName,
                        CtTy::TokenCursor,
                        CtTy::Ident,
                        false,
                    )),
                    _ => None,
                };
                if let Some((projection, source, ty, indexed)) = token {
                    if actual.len() != if indexed { 2 } else { 1 }
                        || actual[0].ty != source
                        || (indexed && !integer(&actual[1]))
                    {
                        return Err(error(span, "actual token traversal arguments differ"));
                    }
                    return self.emit(
                        CtInstructionKind::TokenInspect {
                            source: ids[0],
                            projection,
                            index: if indexed { Some(ids[1]) } else { None },
                        },
                        ty,
                        span,
                    );
                }
                let text = match name {
                    "text_equal" => Some((CtTextOp::Equal, 2, CtTy::Scalar(CtScalarTy::Bool))),
                    "text_len" => Some((CtTextOp::Length, 1, CtTy::Scalar(CtScalarTy::I64))),
                    "text_slice" => Some((CtTextOp::Slice, 3, CtTy::Text)),
                    "text_concat" => Some((CtTextOp::Concatenate, 2, CtTy::Text)),
                    _ => None,
                };
                if let Some((op, count, ty)) = text {
                    if actual.len() != count
                        || actual[0].ty != CtTy::Text
                        || (op == CtTextOp::Slice && (!integer(&actual[1]) || !integer(&actual[2])))
                        || (matches!(op, CtTextOp::Equal | CtTextOp::Concatenate)
                            && actual[1].ty != CtTy::Text)
                    {
                        return Err(error(span, "actual Text operation arguments differ"));
                    }
                    return self.emit(CtInstructionKind::Text { op, arguments: ids }, ty, span);
                }
            }
            _ => {}
        }
        Err(error(
            span,
            "unknown operation in the actual CT standard module",
        ))
    }
}
