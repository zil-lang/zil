//! Preserve value-versus-payload syntax before generic AST substitution.
//! These categories propose types only. They never establish a resource,
//! borrow, class domain, initialization state or operation permission.
use super::*;
use std::borrow::Cow;
use zil_ir::{
    SemanticActualRecipe, SemanticOriginalNameBinding, SemanticOriginalNameValue,
    SemanticOriginalNamespace,
};

struct Normalize<'a, 'b, 'l> {
    nominals: &'a BTreeMap<String, &'a Item>,
    instances: &'a BTreeMap<String, TypeRef>,
    names: &'a [SemanticOriginalNameBinding],
    budget: &'b mut Budget<'l>,
    key_peak: usize,
}
impl Normalize<'_, '_, '_> {
    fn key<'a>(&mut self, path: &'a [String], span: Span) -> R<Cow<'a, str>> {
        if let [name] = path {
            return Ok(Cow::Borrowed(name));
        }
        let length = path
            .iter()
            .try_fold(path.len().saturating_sub(1), |n, p| n.checked_add(p.len()))
            .ok_or_else(|| error(span, "resource spelling length overflow"))?;
        if length > self.key_peak {
            self.budget.scratch_storage(length - self.key_peak, span)?;
            self.key_peak = length;
        }
        self.budget.identity_work(length, span)?;
        Ok(Cow::Owned(path.join(".")))
    }
    fn bare_resource(&mut self, source: &TypeRef, depth: usize) -> R<bool> {
        self.budget.node(source.span, depth)?;
        let source = grouped(source);
        let TypeKind::Named { path, args } = &source.kind else {
            return Ok(false);
        };
        let key = self.key(path, source.span)?;
        let candidate=self.nominals.get(key.as_ref()).map(|item|matches!(&item.kind,ItemKind::Aggregate(a) if a.kind==AggregateKind::Resource&&a.generics.len()==args.len()));
        let generated = self.instances.get(key.as_ref());
        let builtin = crate::receivers::bare_resource_syntax(source, &|_| None);
        if candidate != Some(true) && generated.is_none() && !builtin {
            return Ok(false);
        }
        if let [name] = path.as_slice() {
            self.budget
                .identity_work(self.names.len().saturating_mul(name.len() + 1), source.span)?;
            if self.names.iter().any(|binding| {
                binding.namespace == SemanticOriginalNamespace::Type
                    && binding.path.as_slice() == [name.as_str()]
                    && matches!(
                        binding.value,
                        SemanticOriginalNameValue::Recipe(SemanticActualRecipe::Type(_))
                    )
            }) {
                return Ok(false);
            }
        }
        if let Some(generated) = generated {
            return self.bare_resource(generated, depth + 1);
        }
        Ok(candidate.unwrap_or(builtin))
    }
    fn carrier(&mut self, ty: &mut TypeRef, name: &str) -> R<()> {
        let bytes = std::mem::size_of::<TypeRef>()
            .checked_add(std::mem::size_of::<String>())
            .and_then(|n| n.checked_add(name.len()))
            .and_then(|n| n.checked_add(std::mem::size_of::<GenericArg>()))
            .ok_or_else(|| error(ty.span, "resource carrier allocation overflow"))?;
        self.budget.lineage_bytes(bytes, ty.span)?;
        let span = ty.span;
        let kind = std::mem::replace(&mut ty.kind, TypeKind::Tuple(Vec::new()));
        *ty = TypeRef {
            span,
            kind: TypeKind::Named {
                path: vec![name.to_owned()],
                args: vec![GenericArg::Type(TypeRef { span, kind })],
            },
        };
        Ok(())
    }
    fn ty(&mut self, ty: &mut TypeRef, payload: bool, freeze_values: bool, depth: usize) -> R<()> {
        self.budget.node(ty.span, depth)?;
        let resource = if payload {
            false
        } else {
            self.bare_resource(ty, depth + 1)?
        };
        if let TypeKind::Reference { inner, .. } = &mut ty.kind {
            let head = self.bare_resource(inner, depth + 1)?;
            self.ty(inner, head, freeze_values, depth + 1)?;
            if head {
                self.carrier(inner, crate::receivers::REFERENCE_OWNER)?;
            }
            return Ok(());
        }
        match &mut ty.kind {
            TypeKind::Named { path, args } | TypeKind::Dyn { path, args, .. } => {
                let carrier = matches!(path.as_slice(),[name] if matches!(name.as_str(),"Managed"|"Ref"|"Desc")||crate::receivers::internal_resource_carrier(name));
                for argument in args {
                    match argument {
                        GenericArg::Type(inner) => {
                            self.ty(inner, carrier, freeze_values, depth + 1)?
                        }
                        GenericArg::AssociatedBinding(binding) => match &mut binding.kind {
                            DynBindingKind::Type { value, .. } => {
                                self.ty(value, false, freeze_values, depth + 1)?
                            }
                            DynBindingKind::Constant { ty, .. } => {
                                self.ty(ty, false, freeze_values, depth + 1)?
                            }
                            DynBindingKind::Method { .. } => {}
                        },
                        GenericArg::Lifetime(_) | GenericArg::Const(_) => {}
                    }
                }
            }
            TypeKind::Group(inner)
            | TypeKind::Optional(inner)
            | TypeKind::ForLifetime { inner, .. } => {
                self.ty(inner, payload, freeze_values, depth + 1)?
            }
            TypeKind::Tuple(fields) | TypeKind::Union(fields) | TypeKind::Impl(fields) => {
                for field in fields {
                    self.ty(field, false, freeze_values, depth + 1)?;
                }
            }
            TypeKind::Array { element, .. } => self.ty(element, false, freeze_values, depth + 1)?,
            TypeKind::Function { params, result }
            | TypeKind::ForeignFunction { params, result, .. } => {
                for parameter in params {
                    self.ty(parameter, false, freeze_values, depth + 1)?;
                }
                self.ty(result, false, freeze_values, depth + 1)?;
            }
            TypeKind::Error { error, result } => {
                self.ty(error, false, freeze_values, depth + 1)?;
                self.ty(result, false, freeze_values, depth + 1)?;
            }
            TypeKind::Reference { .. } => unreachable!("handled above"),
            TypeKind::MetaEval(_) | TypeKind::MetaSplice(_) | TypeKind::MetaBlock(_) => {}
        }
        // Grouping preserves a single value category. Its child has already
        // carried the category, so it must not be wrapped a second time.
        if resource && freeze_values && !matches!(ty.kind, TypeKind::Group(_)) {
            self.carrier(ty, crate::receivers::VALUE_DESCRIPTION)?;
        }
        Ok(())
    }
}
impl Engine<'_> {
    pub(super) fn preserve_resource_actuals(
        &mut self,
        parameters: &[&GenericParam],
        actuals: &mut [TypeRef],
        span: Span,
    ) -> R<()> {
        let program = self.products.semantic.as_ref();
        let owner = self.nominal_owner;
        let nominals = &self.nominals;
        let instances = &self.instance_types;
        self.budget.metadata_scope(span, false, |budget| {
            let names = match (program, owner) {
                (Some(program), Some(owner)) => program.nominal_owner_names(owner, budget, span)?,
                _ => &[],
            };
            let mut normalize = Normalize {
                nominals,
                instances,
                names,
                budget,
                key_peak: 0,
            };
            for (formal, actual) in parameters.iter().zip(actuals) {
                if matches!(formal.kind, GenericParamKind::Type(_)) {
                    normalize.ty(actual, false, true, 0)?;
                }
            }
            Ok(())
        })
    }
    pub(super) fn preserve_resource_header(&mut self, function: &mut Function) -> R<()> {
        let program = self.products.semantic.as_ref();
        let owner = self.nominal_owner;
        let nominals = &self.nominals;
        let instances = &self.instance_types;
        self.budget.metadata_scope(function.span, false, |budget| {
            let names = match (program, owner) {
                (Some(program), Some(owner)) => {
                    program.nominal_owner_names(owner, budget, function.span)?
                }
                _ => &[],
            };
            let mut normalize = Normalize {
                nominals,
                instances,
                names,
                budget,
                key_peak: 0,
            };
            for parameter in &mut function.params {
                normalize.ty(&mut parameter.ty, false, false, 0)?;
            }
            if let Some(result) = &mut function.return_type {
                normalize.ty(result, false, false, 0)?;
            }
            Ok(())
        })
    }
}

pub(super) fn public_input(module: &Module, limits: &Limits) -> CompileResult<Limits> {
    struct Input<'a, 'l>(&'a mut Budget<'l>);
    impl crate::ct_selection::Visitor for Input<'_, '_> {
        fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
            self.0.node(span, depth).map_err(|error| vec![error])
        }
        fn text(&mut self, value: &str, span: Span) -> CompileResult<()> {
            self.0.text(value, span).map_err(|error| vec![error])?;
            if crate::is_internal_source_type_name(value) {
                return Err(vec![error(
                    span,
                    "private resource category is not public Source syntax",
                )]);
            }
            Ok(())
        }
        fn type_binder(&mut self, name: &str, span: Span) -> CompileResult<()> {
            if name == "Managed" {
                return Err(vec![super::super::primitive_name_error(name, span)]);
            }
            Ok(())
        }
    }
    use crate::ct_selection::Visitor;
    let mut budget = Budget::new(limits);
    budget
        .metadata_scope(module.span, false, |budget| {
            let mut visitor = Input(budget);
            for item in &module.items {
                visitor
                    .item(item, 0)
                    .map_err(|mut errors| errors.remove(0))?;
            }
            Ok(())
        })
        .map_err(|error| vec![error])?;
    Ok(budget.remaining_limits())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, ModuleId, SourceUnit};
    use zil_ir::{SemanticFormalRef, SemanticScopeRef, SemanticTypeRecipe};

    fn syntax() -> Module {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "resource-actuals.zil",
            "resource R{code:i32}package P{code:i32}fn sample<T>(a:&R,b:&mut R,c:&Ref<R>,d:&Managed<R>,e:&P,f:&T){}fn shadow<Ref>(){}",
        );
        zil_syntax::parse(&source).into_result().unwrap()
    }
    fn lower(ty: &TypeRef) -> zil_ir::Type {
        crate::type_ref_with_resources(ty, &|ty| match &grouped(ty).kind {
            TypeKind::Named { path, .. } if path.as_slice() == ["R"] => Some(true),
            TypeKind::Named { path, .. } if path.as_slice() == ["P"] => Some(false),
            _ => None,
        })
    }
    #[test]
    fn complete_actual_value_category_survives_under_reference_and_is_idempotent() {
        let module = syntax();
        let nominals = module
            .items
            .iter()
            .filter_map(|item| match &item.kind {
                ItemKind::Aggregate(value) => Some((value.name.clone(), item)),
                _ => None,
            })
            .collect();
        let instances = BTreeMap::new();
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        // The caller has a legal formal named Ref. A generated descriptor is
        // not that binder and must never be scanned/substituted as one.
        let names = [SemanticOriginalNameBinding {
            path: vec!["Ref".into()],
            namespace: SemanticOriginalNamespace::Type,
            value: SemanticOriginalNameValue::Recipe(SemanticActualRecipe::Type(
                SemanticTypeRecipe::Formal(SemanticFormalRef {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal: 0,
                }),
            )),
        }];
        let mut actual = named("R", module.span);
        budget
            .metadata_scope(module.span, false, |budget| {
                let mut normalize = Normalize {
                    nominals: &nominals,
                    instances: &instances,
                    names: &names,
                    budget,
                    key_peak: 0,
                };
                normalize.ty(&mut actual, false, true, 0)?;
                let once = actual.clone();
                normalize.ty(&mut actual, false, true, 0)?;
                assert_eq!(actual, once);
                Ok(())
            })
            .unwrap();
        assert!(
            matches!(&actual.kind,TypeKind::Named{path,args} if path.as_slice()==[crate::receivers::VALUE_DESCRIPTION]&&args.len()==1)
        );
        let mut formal = TypeRef {
            span: module.span,
            kind: TypeKind::Reference {
                mutable: false,
                lifetime: None,
                inner: Box::new(named("T", module.span)),
            },
        };
        Substitution {
            substitutions: &BTreeMap::from([("T".into(), actual)]),
        }
        .ty(&mut formal);
        assert!(
            matches!(lower(&formal),zil_ir::Type::Address{inner,..} if matches!(*inner,zil_ir::Type::Desc(_)))
        );
        assert!(
            matches!(lower(&TypeRef {span:module.span,kind:TypeKind::Reference{mutable:true,lifetime:None,inner:Box::new(named("R",module.span))}}),zil_ir::Type::Address{inner,..} if matches!(*inner,zil_ir::Type::Managed(_)))
        );
    }
    #[test]
    fn header_and_runtime_self_keep_resource_heads_distinct_from_descriptor_slots() {
        let module = syntax();
        let nominals = module
            .items
            .iter()
            .filter_map(|item| match &item.kind {
                ItemKind::Aggregate(value) => Some((value.name.clone(), item)),
                _ => None,
            })
            .collect();
        let instances = BTreeMap::new();
        let ItemKind::Function(function) = &module.items[2].kind else {
            panic!()
        };
        let mut parameters = function.params.clone();
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        budget
            .metadata_scope(module.span, false, |budget| {
                let mut normalize = Normalize {
                    nominals: &nominals,
                    instances: &instances,
                    names: &[],
                    budget,
                    key_peak: 0,
                };
                for parameter in &mut parameters {
                    normalize.ty(&mut parameter.ty, false, false, 0)?;
                }
                Ok(())
            })
            .unwrap();
        for parameter in &parameters[..2] {
            assert!(
                matches!(lower(&parameter.ty),zil_ir::Type::Address{inner,..} if matches!(*inner,zil_ir::Type::Managed(_)))
            );
        }
        assert!(
            matches!(lower(&parameters[2].ty),zil_ir::Type::Address{inner,..} if matches!(*inner,zil_ir::Type::Desc(_)))
        );
        assert!(
            matches!(lower(&parameters[3].ty),zil_ir::Type::Address{inner,..} if matches!(*inner,zil_ir::Type::Managed(_)))
        );
        assert!(
            matches!(lower(&parameters[4].ty),zil_ir::Type::Address{inner,..} if matches!(*inner,zil_ir::Type::Named(ref name) if name=="P"))
        );
        assert!(
            matches!(lower(&parameters[5].ty),zil_ir::Type::Address{inner,..} if matches!(*inner,zil_ir::Type::Named(ref name) if name=="T"))
        );
        let declarations = crate::declaration_index([&module]);
        let mut expander = crate::Expander::new(ModuleId(0), false, &limits, None, &declarations);
        let reference = TypeRef {
            span: module.span,
            kind: TypeKind::Reference {
                mutable: false,
                lifetime: None,
                inner: Box::new(named("Self", module.span)),
            },
        };
        expander.self_type = Some(zil_ir::Type::Named("R".into()));
        assert!(
            matches!(expander.type_ref(&reference),zil_ir::Type::Address{inner,..} if matches!(*inner,zil_ir::Type::Managed(_)))
        );
        expander.self_type = Some(zil_ir::Type::Desc(Box::new(zil_ir::Type::Named(
            "R".into(),
        ))));
        assert!(
            matches!(expander.type_ref(&reference),zil_ir::Type::Address{inner,..} if matches!(*inner,zil_ir::Type::Desc(_)))
        );
    }
    #[test]
    fn carrier_shape_and_nested_invalid_modes_are_never_laundered() {
        let wrap = |name: &str, inner: TypeRef| TypeRef {
            span: inner.span,
            kind: TypeKind::Named {
                path: vec![name.into()],
                args: vec![GenericArg::Type(inner)],
            },
        };
        let resource = named("TaskI32", Span::default());
        for malformed in [
            named(crate::receivers::VALUE_DESCRIPTION, Span::default()),
            TypeRef {
                span: Span::default(),
                kind: TypeKind::Named {
                    path: vec![crate::receivers::VALUE_DESCRIPTION.into(), "R".into()],
                    args: vec![GenericArg::Type(resource.clone())],
                },
            },
        ] {
            assert!(matches!(
                crate::type_ref(&malformed),
                zil_ir::Type::Unsupported(_)
            ));
        }
        for invalid in [
            wrap("Managed", wrap("Ref", resource.clone())),
            wrap("Managed", wrap("Managed", resource.clone())),
            wrap(
                "Managed",
                wrap(crate::receivers::VALUE_DESCRIPTION, resource),
            ),
        ] {
            assert!(matches!(
                crate::type_ref(&invalid),
                zil_ir::Type::Unsupported(_)
            ));
        }
    }
    #[test]
    fn public_ast_cannot_smuggle_a_carrier_or_rebind_managed() {
        let mut module = syntax();
        let limits = Limits::default();
        let ItemKind::Function(function) = &mut module.items[2].kind else {
            panic!()
        };
        function.params[0].ty = named(crate::receivers::VALUE_DESCRIPTION, function.span);
        assert!(public_input(&module, &limits).is_err());
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "managed-formal.zil",
            "fn bad<Managed>(){}",
        );
        let module = zil_syntax::parse(&source).into_result().unwrap();
        assert!(public_input(&module, &limits).is_err());
    }
}
