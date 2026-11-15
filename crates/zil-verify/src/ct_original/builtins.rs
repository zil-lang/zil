//! The closed standard CT signatures. A builtin name is interpreted only
//! after resolving its actual compiler-library import edge and local shadows.
use super::*;

impl Checker<'_, '_, '_> {
    pub(super) fn builtin(
        &mut self,
        env: &mut Env,
        module: CtOriginalImportTarget,
        name: &str,
        args: &[s::Argument],
        depth: usize,
        span: Span,
    ) -> R<Value> {
        self.step(span, depth)?;
        if args.iter().any(|arg| arg.spread) {
            return Err(bad(span, "CT standard Call cannot spread an argument"));
        }
        let mut values = self.vector(args.len(), span)?;
        for arg in args {
            values.push(self.expression(env, &arg.value, None, depth + 1)?);
        }
        let mut types = self.vector(values.len(), span)?;
        for value in &values {
            types.push(self.duplicate_type(&value.ty, span)?);
        }
        let type_ty = Ty::Meta(Meta::Type);
        let text = Ty::Meta(Meta::Text);
        let tree = Ty::Meta(Meta::TokenTree);
        let cursor = Ty::Meta(Meta::TokenCursor);
        let exact = |expected: &[Ty]| types == expected;
        let all_types = types.iter().all(|ty| *ty == type_ty);
        let signature_error = || {
            bad(
                span,
                "CT standard operation arguments differ from its complete original signature",
            )
        };
        if module == CtOriginalImportTarget::Meta {
            let constructor = match name {
                "array" | "array_type" => Some(("Array", 2)),
                "tuple" | "tuple_type" => Some(("Tuple", usize::MAX)),
                "function_pointer" => Some(("Function", usize::MAX)),
                "address" => Some(("Address", 1)),
                "address_mut" => Some(("AddressMut", 1)),
                "slice" => Some(("Slice", 1)),
                "managed" => Some(("Managed", 1)),
                "desc" => Some(("Desc", 1)),
                "store" => Some(("Store", 1)),
                "error_union" => Some(("ErrorUnion", 2)),
                _ => None,
            };
            if let Some((constructor, count)) = constructor {
                let valid = if constructor == "Array" {
                    types.len() == 2 && types[0] == type_ty && integer(&types[1])
                } else {
                    all_types
                        && (count == usize::MAX || types.len() == count)
                        && (constructor != "Function" || !types.is_empty())
                };
                if !valid {
                    return Err(signature_error());
                }
                let mut result = Value::typed(type_ty);
                if values.iter().enumerate().all(|(i, value)| {
                    constructor == "Array" && i == 1 || value.represented.is_some()
                }) {
                    let represented = if constructor == "Array" {
                        let mut arguments = values.into_iter();
                        let element = arguments.next().and_then(|value| value.represented);
                        let length = arguments.next().and_then(|value| value.constant);
                        match (element, length) {
                            (Some(element), Some(mut length)) => {
                                if matches!(length.term,Term::Integer(n)if n<0||n>u32::MAX as i128)
                                {
                                    return Err(bad(
                                        span,
                                        "CT Type array cardinality is outside the finite domain",
                                    ));
                                }
                                length.carrier = Scalar::Usize;
                                self.alloc(std::mem::size_of::<Ty>(), span)?;
                                Some(Ty::Array(Box::new(element), length))
                            }
                            _ => None,
                        }
                    } else if constructor == "Tuple" {
                        let mut fields = self.vector(values.len(), span)?;
                        for value in values {
                            fields.push(value.represented.ok_or_else(signature_error)?);
                        }
                        Some(if fields.is_empty() {
                            unit()
                        } else {
                            Ty::Tuple(fields)
                        })
                    } else {
                        let mut actuals = self.vector(values.len(), span)?;
                        for value in values {
                            actuals
                                .push(Actual::Type(value.represented.ok_or_else(signature_error)?));
                        }
                        Some(Ty::Runtime(constructor, actuals))
                    };
                    result.represented = represented;
                }
                return Ok(result);
            }
            let result = match name {
                "shape" => {
                    if !exact(&[type_ty.clone()]) {
                        return Err(signature_error());
                    }
                    Ty::Meta(Meta::Shape)
                }
                "field" => {
                    if !exact(&[type_ty.clone(), text]) {
                        return Err(signature_error());
                    }
                    Ty::Meta(Meta::Field)
                }
                "impls" | "impl_index" => {
                    if !all_types || types.is_empty() {
                        return Err(signature_error());
                    }
                    self.alloc(std::mem::size_of::<Ty>(), span)?;
                    Ty::List(Box::new(Ty::Meta(Meta::Impl)))
                }
                "layout" => {
                    if !exact(&[type_ty]) {
                        return Err(signature_error());
                    }
                    Ty::Meta(Meta::Layout)
                }
                "fields" | "shape_fields" => {
                    if !exact(&[Ty::Meta(Meta::Shape)]) {
                        return Err(signature_error());
                    }
                    self.alloc(std::mem::size_of::<Ty>(), span)?;
                    Ty::List(Box::new(Ty::Meta(Meta::Field)))
                }
                "complete" | "shape_complete" => {
                    if !exact(&[Ty::Meta(Meta::Shape)]) {
                        return Err(signature_error());
                    }
                    bool_ty()
                }
                "field_name" => {
                    if !exact(&[Ty::Meta(Meta::Field)]) {
                        return Err(signature_error());
                    }
                    Ty::Meta(Meta::Ident)
                }
                "field_type" => {
                    if !exact(&[Ty::Meta(Meta::Field)]) {
                        return Err(signature_error());
                    }
                    Ty::Meta(Meta::Type)
                }
                "field_managed" => {
                    if !exact(&[Ty::Meta(Meta::Field)]) {
                        return Err(signature_error());
                    }
                    bool_ty()
                }
                "impl_type" => {
                    if !exact(&[Ty::Meta(Meta::Impl)]) {
                        return Err(signature_error());
                    }
                    Ty::Meta(Meta::Type)
                }
                "size" | "layout_size" | "alignment" | "layout_alignment" => {
                    if !exact(&[Ty::Meta(Meta::Layout)]) {
                        return Err(signature_error());
                    }
                    Ty::Scalar(Scalar::I64)
                }
                "field_offsets" => {
                    if !exact(&[Ty::Meta(Meta::Layout)]) {
                        return Err(signature_error());
                    }
                    self.alloc(std::mem::size_of::<Ty>(), span)?;
                    Ty::List(Box::new(Ty::Scalar(Scalar::I64)))
                }
                _ => {
                    return Err(bad(
                        span,
                        "unknown operation in the actual CT reflection library",
                    ));
                }
            };
            return Ok(Value::typed(result));
        }
        if module != CtOriginalImportTarget::Syntax {
            return Err(bad(
                span,
                "ordinary source module cannot grant a closed CT builtin",
            ));
        }
        let result = match name {
            "parse_expr" | "parse_type" | "parse_stmts" | "parse_stmt_list" | "parse_items"
            | "parse_item_list" => {
                if !exact(&[tree.clone()]) {
                    return Err(signature_error());
                }
                Ty::Meta(Meta::Syntax(match name {
                    "parse_expr" => Category::Expr,
                    "parse_type" => Category::Type,
                    "parse_stmts" | "parse_stmt_list" => Category::StmtList,
                    _ => Category::ItemList,
                }))
            }
            "parse_ident" => {
                if !exact(&[tree.clone()]) {
                    return Err(signature_error());
                }
                Ty::Meta(Meta::Ident)
            }
            "literal" => {
                if types.len() != 1 || !matches!(types[0], Ty::Scalar(_) | Ty::Meta(Meta::Text)) {
                    return Err(signature_error());
                }
                Ty::Meta(Meta::Syntax(Category::Expr))
            }
            "type_ref" => {
                if !exact(&[type_ty]) {
                    return Err(signature_error());
                }
                Ty::Meta(Meta::Syntax(Category::Type))
            }
            "name_expr" => {
                if !exact(&[Ty::Meta(Meta::Ident)]) {
                    return Err(signature_error());
                }
                Ty::Meta(Meta::Syntax(Category::Expr))
            }
            "fresh" | "export" => {
                if !exact(&[text.clone()]) {
                    return Err(signature_error());
                }
                Ty::Meta(Meta::Ident)
            }
            "root_count" => {
                if !exact(&[tree.clone()]) {
                    return Err(signature_error());
                }
                Ty::Scalar(Scalar::I64)
            }
            "root_at" => {
                if types.len() != 2 || types[0] != tree || !integer(&types[1]) {
                    return Err(signature_error());
                }
                cursor.clone()
            }
            "token_class" | "delimiter" => {
                if !exact(&[cursor.clone()]) {
                    return Err(signature_error());
                }
                Ty::Scalar(Scalar::I32)
            }
            "token_text" => {
                if !exact(&[cursor.clone()]) {
                    return Err(signature_error());
                }
                text.clone()
            }
            "child_count" => {
                if !exact(&[cursor.clone()]) {
                    return Err(signature_error());
                }
                Ty::Scalar(Scalar::I64)
            }
            "child_at" => {
                if types.len() != 2 || types[0] != cursor || !integer(&types[1]) {
                    return Err(signature_error());
                }
                cursor.clone()
            }
            "input_name" => {
                if !exact(&[cursor.clone()]) {
                    return Err(signature_error());
                }
                Ty::Meta(Meta::Ident)
            }
            "text_equal" | "text_concat" => {
                if !exact(&[text.clone(), text.clone()]) {
                    return Err(signature_error());
                }
                if name == "text_equal" {
                    bool_ty()
                } else {
                    text.clone()
                }
            }
            "text_len" => {
                if !exact(&[text.clone()]) {
                    return Err(signature_error());
                }
                Ty::Scalar(Scalar::I64)
            }
            "text_slice" => {
                if types.len() != 3
                    || types[0] != text
                    || !integer(&types[1])
                    || !integer(&types[2])
                {
                    return Err(signature_error());
                }
                text
            }
            _ => {
                return Err(bad(
                    span,
                    "unknown operation in the actual CT syntax library",
                ));
            }
        };
        Ok(Value::typed(result))
    }
    pub(super) fn meta_field(&self, ty: &Ty, name: &str) -> Option<Ty> {
        Some(match (ty, name) {
            (Ty::Meta(Meta::Shape), "fields") => Ty::List(Box::new(Ty::Meta(Meta::Field))),
            (Ty::Meta(Meta::Shape), "complete") => bool_ty(),
            (Ty::Meta(Meta::Field), "name") => Ty::Meta(Meta::Ident),
            (Ty::Meta(Meta::Field), "ty") | (Ty::Meta(Meta::Impl), "for_type") => {
                Ty::Meta(Meta::Type)
            }
            (Ty::Meta(Meta::Field), "managed") => bool_ty(),
            (Ty::Meta(Meta::Layout), "size" | "alignment") => Ty::Scalar(Scalar::I64),
            (Ty::Meta(Meta::Layout), "field_offsets") => {
                Ty::List(Box::new(Ty::Scalar(Scalar::I64)))
            }
            _ => return None,
        })
    }
}
