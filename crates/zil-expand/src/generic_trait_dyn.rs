//! Source-owned dyn interface and static implementation recipes. These are
//! untrusted inputs for the public Types/CFG consumers, never permissions.
use super::*;
#[path = "generic_trait_dyn_source.rs"]
pub(crate) mod source;

pub(crate) struct Slot {
    pub(crate) declaration: origins::MemberId,
    pub(crate) reference: trait_instances::Reference,
    pub(crate) signature: Function,
    /// Empty syntax is an unresolved contract, not an empty effect/Hold claim.
    pub(crate) contract_written: bool,
}
pub(crate) struct Interface {
    pub(crate) reference: trait_instances::Reference,
    pub(crate) declaration: origins::DeclId,
    pub(crate) inherited: Vec<trait_instances::Reference>,
    pub(crate) slots: Vec<Slot>,
}
pub(crate) struct Provider {
    pub(crate) implementation: origins::DeclId,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Retained dyn provider recipe; runtime dispatch consumer is not implemented yet"
        )
    )]
    pub(crate) declaration: origins::MemberId,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Retained dyn provider recipe; runtime dispatch consumer is not implemented yet"
        )
    )]
    pub(crate) substitutions: BTreeMap<String, TypeRef>,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Retained dyn provider recipe; runtime dispatch consumer is not implemented yet"
        )
    )]
    pub(crate) signature: Function,
}
pub(crate) struct Vtable {
    pub(crate) interface: usize,
    pub(crate) implementation: origins::DeclId,
    pub(crate) self_type: TypeRef,
    pub(crate) providers: Vec<Provider>,
}
#[derive(Default)]
pub(crate) struct Catalog {
    declarations: Vec<source::Declaration>,
    targets: Vec<source::Target>,
    pub(crate) interfaces: Vec<Interface>,
    pub(crate) vtables: Vec<Vtable>,
}
fn pay_key(key: &trait_instances::Key, budget: &mut Budget<'_>, span: Span) -> R<()> {
    budget.text(&key.name, span)?;
    for argument in &key.arguments {
        budget.node(span, 0)?;
        budget.text(argument, span)?;
    }
    Ok(())
}
pub(in crate::generics) fn is_interface(ty: &TypeRef) -> bool {
    matches!(grouped(ty).kind, TypeKind::Dyn { .. })
}
impl Catalog {
    fn find(
        &self,
        reference: &trait_instances::Reference,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<Option<usize>> {
        for (ordinal, interface) in self.interfaces.iter().enumerate() {
            budget.node(span, 0)?;
            pay_key(&reference.key, budget, span)?;
            pay_key(&interface.reference.key, budget, span)?;
            if interface.reference.key == reference.key {
                return Ok(Some(ordinal));
            }
        }
        Ok(None)
    }
    pub(in crate::generics) fn method(
        &self,
        index: &Index<'_>,
        actual: &TypeRef,
        name: &str,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<Option<Function>> {
        if !is_interface(actual) {
            return Ok(None);
        }
        let reference = index.dynamic_reference(actual, budget)?;
        let ordinal = self
            .find(&reference, budget, span)?
            .ok_or_else(|| error(span, "dyn method interface was not inventoried"))?;
        let mut selected = None;
        for slot in &self.interfaces[ordinal].slots {
            budget.node(span, 0)?;
            budget.text(name, span)?;
            budget.text(&slot.signature.name, span)?;
            if name == slot.signature.name {
                if selected.is_some() {
                    return Err(error(
                        span,
                        "dyn inherited method needs an exact qualification",
                    ));
                }
                selected = Some(&slot.signature);
            }
        }
        selected
            .map(|signature| {
                budget.const_function(signature)?;
                Ok(signature.clone())
            })
            .transpose()
    }
}
fn copy_reference(
    reference: &trait_instances::Reference,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<trait_instances::Reference> {
    let key = trait_instances::copy_key(&reference.key, budget, span)?;
    let mut arguments = Vec::new();
    for actual in &reference.arguments {
        budget.ty(actual)?;
        arguments.push(actual.clone());
    }
    Ok(trait_instances::Reference { key, arguments })
}
fn owned_signature(
    declaration: &TraitDecl,
    member: &Item,
    reference: &trait_instances::Reference,
    budget: &mut Budget<'_>,
) -> R<Function> {
    let signature = trait_instances::signature(declaration, member, reference, budget)?;
    if let Cow::Borrowed(function) = &signature {
        budget.const_function(function)?;
    }
    let mut signature = signature.into_owned();
    signature.body = FunctionBody::Declaration;
    Ok(signature)
}
impl Index<'_> {
    fn dynamic_reference(
        &self,
        ty: &TypeRef,
        budget: &mut Budget<'_>,
    ) -> R<trait_instances::Reference> {
        if let TypeKind::Dyn { bindings, .. } = &grouped(ty).kind
            && !bindings.is_empty()
        {
            return Err(unsupported(
                ty.span,
                "dyn binding semantic consumer is not yet implemented",
            ));
        }
        let TypeKind::Dyn { path, args, .. } = &grouped(ty).kind else {
            return Err(error(
                ty.span,
                "dyn interface requires its actual type syntax",
            ));
        };
        budget.ty(ty)?;
        let application = TypeRef {
            span: ty.span,
            kind: TypeKind::Named {
                path: path.clone(),
                args: args.clone(),
            },
        };
        self.reference(&application, budget)
    }
    fn dynamic_interface(&self, ty: &TypeRef, budget: &mut Budget<'_>) -> R<Interface> {
        let reference = self.dynamic_reference(ty, budget)?;
        let (owner, _) = self.declaration_item(&reference);
        let origin = self
            .origin(owner, budget)?
            .ok_or_else(|| error(ty.span, "dyn declaration origin absent"))?;
        let mut slots = Vec::new();
        let expanded = self.inherited_bounds(
            vec![Bound::Nominal(copy_reference(&reference, budget, ty.span)?)],
            "Self",
            budget,
            ty.span,
        )?;
        let mut inherited = Vec::new();
        for bound in expanded {
            let Bound::Nominal(actual) = bound else {
                continue;
            };
            let (owner, declaration) = self.declaration_item(&actual);
            let origin = self
                .origin(owner, budget)?
                .ok_or_else(|| error(ty.span, "inherited dyn declaration origin absent"))?;
            if actual.key != reference.key {
                for argument in &actual.arguments {
                    budget.ty(argument)?;
                    if callable::refers(argument, "Self") {
                        return Err(unsupported(
                            argument.span,
                            "dyn inherited argument depends on unfixed Self",
                        ));
                    }
                }
                budget.node(ty.span, 0)?;
                inherited.push(copy_reference(&actual, budget, ty.span)?);
            }
            for (ordinal, member) in declaration.members.iter().enumerate() {
                budget.node(member.span, 0)?;
                let ItemKind::Function(method) = &member.kind else {
                    continue;
                };
                // Generic slots must come from explicit finite binding applications;
                // they never become an executable runtime generic entry.
                if !method.generics.is_empty() {
                    continue;
                }
                let signature = owned_signature(declaration, member, &actual, budget)?;
                let Some(receiver) = signature.params.first().filter(|p| p.name == "self") else {
                    continue;
                };
                if !super::super::super::static_traits::receiver_type(&receiver.ty) {
                    return Err(error(receiver.span, "dyn receiver is not a Self receiver"));
                }
                for ty in signature
                    .params
                    .iter()
                    .skip(1)
                    .map(|p| &p.ty)
                    .chain(signature.return_type.iter())
                {
                    budget.ty(ty)?;
                    if callable::refers(ty, "Self") {
                        return Err(unsupported(
                            ty.span,
                            "dyn slot needs explicit fixed associated/Self bindings",
                        ));
                    }
                }
                slots.push(Slot {
                    declaration: origin.member(ordinal, member.span)?.id(member.span)?,
                    reference: copy_reference(&actual, budget, member.span)?,
                    contract_written: !signature.contract.is_empty(),
                    signature,
                });
            }
        }
        Ok(Interface {
            reference,
            declaration: origin.declaration,
            inherited,
            slots,
        })
    }
    fn dynamic_vtable(
        &self,
        interface: usize,
        requested: &Interface,
        witness: Witness<'_>,
        budget: &mut Budget<'_>,
    ) -> R<Vtable> {
        let implementation_origin = self
            .origin(witness.item, budget)?
            .ok_or_else(|| error(witness.item.span, "dyn impl origin absent"))?;
        let mut providers = Vec::new();
        for slot in &requested.slots {
            budget.node(slot.signature.span, 0)?;
            let selected_witness = self
                .solve(
                    &slot.reference,
                    &witness.actual,
                    &Bounds::new(),
                    &BTreeSet::new(),
                    budget,
                )?
                .ok_or_else(|| {
                    error(
                        slot.signature.span,
                        "dyn slot needs its own committed inherited witness",
                    )
                })?;
            let ItemKind::Impl(implementation) = &selected_witness.item.kind else {
                unreachable!()
            };
            let (trait_item, declaration) = self.declaration_item(&slot.reference);
            let mut selected = None;
            for (ordinal, member) in implementation.members.iter().enumerate() {
                budget.node(member.span, 0)?;
                if let ItemKind::Function(method) = &member.kind {
                    budget.text(&method.name, member.span)?;
                    budget.text(&slot.signature.name, slot.signature.span)?;
                    if method.name == slot.signature.name {
                        selected = Some((selected_witness.item, ordinal, member));
                        break;
                    }
                }
            }
            let (owner, ordinal, member) = if let Some(selected) = selected {
                selected
            } else {
                let mut selected = None;
                for (ordinal, member) in declaration.members.iter().enumerate() {
                    budget.node(member.span, 0)?;
                    if let ItemKind::Function(method) = &member.kind {
                        budget.text(&method.name, member.span)?;
                        budget.text(&slot.signature.name, slot.signature.span)?;
                        if method.name == slot.signature.name
                            && !matches!(method.body, FunctionBody::Declaration)
                        {
                            selected = Some((trait_item, ordinal, member));
                            break;
                        }
                    }
                }
                selected.ok_or_else(|| {
                    error(
                        selected_witness.item.span,
                        "dyn slot has no concrete provider",
                    )
                })?
            };
            let origin = self
                .origin(owner, budget)?
                .ok_or_else(|| error(member.span, "dyn provider origin absent"))?
                .member(ordinal, member.span)?
                .id(member.span)?;
            let mut substitutions = BTreeMap::new();
            if std::ptr::eq(owner, selected_witness.item) {
                for (name, ty) in &selected_witness.substitutions {
                    budget.text(name, member.span)?;
                    budget.ty(ty)?;
                    substitutions.insert(name.clone(), ty.clone());
                }
            } else {
                for (parameter, argument) in
                    declaration.generics.iter().zip(&slot.reference.arguments)
                {
                    budget.text(&parameter.name, member.span)?;
                    budget.ty(argument)?;
                    substitutions.insert(parameter.name.clone(), argument.clone());
                }
            }
            budget.text("Self", member.span)?;
            budget.ty(&witness.actual)?;
            substitutions.insert("Self".into(), witness.actual.clone());
            budget.const_function(&slot.signature)?;
            let mut signature = Item {
                span: member.span,
                public: false,
                attributes: Vec::new(),
                kind: ItemKind::Function(slot.signature.clone()),
            };
            budget.substituted_item(&signature, &substitutions)?;
            substitute_item(&mut signature, &substitutions)?;
            let ItemKind::Function(signature) = signature.kind else {
                unreachable!()
            };
            providers.push(Provider {
                implementation: self
                    .origin(selected_witness.item, budget)?
                    .ok_or_else(|| error(member.span, "dyn slot implementation origin absent"))?
                    .declaration,
                declaration: origin,
                substitutions,
                signature,
            });
        }
        budget.ty(&witness.actual)?;
        Ok(Vtable {
            interface,
            implementation: implementation_origin.declaration,
            self_type: witness.actual,
            providers,
        })
    }
}
pub(in crate::generics) fn collect<'a>(
    module: &'a Module,
    index: &Index<'a>,
    origins: &origins::Origins,
    budget: &mut Budget<'a>,
) -> R<Catalog> {
    use super::super::super::ct_selection::Visitor;
    struct Collect<'a, 'b, 'l> {
        index: &'a Index<'l>,
        catalog: &'b mut Catalog,
        budget: &'b mut Budget<'l>,
    }
    impl Visitor for Collect<'_, '_, '_> {
        fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
            self.budget.node(span, depth).map_err(|e| vec![e])
        }
        fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
            self.budget.text(text, span).map_err(|e| vec![e])
        }
        fn ty(&mut self, ty: &TypeRef, depth: usize) -> CompileResult<()> {
            if matches!(ty.kind, TypeKind::Dyn { .. }) {
                let candidate = self
                    .index
                    .dynamic_interface(ty, self.budget)
                    .map_err(|e| vec![e])?;
                if self
                    .catalog
                    .find(&candidate.reference, self.budget, ty.span)
                    .map_err(|e| vec![e])?
                    .is_none()
                {
                    self.node(ty.span, depth)?;
                    self.catalog.interfaces.push(candidate);
                }
            }
            super::super::super::ct_selection::walk_type(self, ty, depth)
        }
    }
    let mut catalog = Catalog::default();
    for (ordinal, item) in module.items.iter().enumerate() {
        let origin = origins.get(ordinal, item.span)?;
        budget.with_origin(Some(origin), |budget| {
            Collect {
                index,
                catalog: &mut catalog,
                budget,
            }
            .item(item, 0)
            .map_err(|mut errors| errors.remove(0))
        })?;
    }
    for (ordinal, requested) in catalog.interfaces.iter().enumerate() {
        for item in &index.recipes {
            budget.node(item.span, 0)?;
            let ItemKind::Impl(implementation) = &item.kind else {
                unreachable!()
            };
            if !implementation.generics.is_empty() {
                continue;
            }
            let Some(application) = &implementation.trait_type else {
                continue;
            };
            if trait_instances::head(application) != Some(requested.reference.key.name.as_str()) {
                continue;
            }
            let origin = index.origin(item, budget)?;
            let reference = budget.with_origin(origin, |budget| {
                index.pattern_reference(application, budget)
            })?;
            if reference.key != requested.reference.key {
                continue;
            }
            let witness = index
                .solve(
                    &requested.reference,
                    &implementation.target,
                    &Bounds::new(),
                    &BTreeSet::new(),
                    budget,
                )?
                .ok_or_else(|| {
                    error(
                        item.span,
                        "dyn vtable needs a committed concrete impl witness",
                    )
                })?;
            let vtable = index.dynamic_vtable(ordinal, requested, witness, budget)?;
            budget.node(item.span, 0)?;
            catalog.vtables.push(vtable);
        }
    }
    catalog.declarations = source::declarations(module, origins, budget)?;
    Ok(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, ModuleId, SourceUnit};

    #[test]
    fn dyn_source_core_publishes_actual_declarations_and_emitted_provider_ids() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(17),
            "not-a-logical-module.zil",
            include_str!(
                "../../../tests/language-readiness/terminal/generics-and-constants/dyn-source/generic-instances.zil"
            ),
        );
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let aliases = BTreeMap::new();
        let expanded = crate::expand_with_map_limits_target_context(
            &source,
            &parsed,
            &Limits::default(),
            &zil_base::TargetProfile::default(),
            crate::source_context::SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &aliases,
                declarations: crate::source_context::DeclarationPlacement::Original,
            },
        )
        .unwrap();
        let core = expanded.core;
        assert_eq!(core.dyn_interfaces.len(), 2);
        assert_eq!(core.dyn_vtables.len(), 2);
        assert_eq!(core.dyn_declarations.len(), 3);
        assert_ne!(
            core.dyn_interfaces[0].application.actuals,
            core.dyn_interfaces[1].application.actuals
        );
        for table in &core.dyn_vtables {
            let interface = core
                .dyn_interfaces
                .iter()
                .find(|interface| interface.id == table.interface)
                .unwrap();
            assert_eq!(table.entries.len(), interface.slots.len());
            for entry in &table.entries {
                let function = core
                    .functions
                    .iter()
                    .find(|function| function.id == entry.function)
                    .unwrap();
                assert_eq!(entry.module, core.module_id);
                assert_eq!(
                    function.return_type,
                    interface.slots[entry.slot as usize].result
                );
                assert!(
                    function
                        .parameters
                        .first()
                        .is_some_and(|parameter| matches!(
                            parameter.ty,
                            zil_ir::Type::Address { .. }
                        ))
                );
            }
        }
        assert!(
            core.functions
                .iter()
                .any(|function| function.name.ends_with(".mix"))
        );
        for function in &core.functions {
            if matches!(function.name.as_str(), "peek" | "flag") {
                assert!(
                    matches!(&function.parameters[0].ty, zil_ir::Type::Address { inner, .. } if matches!(inner.as_ref(), zil_ir::Type::Dyn { .. }))
                );
            }
            if function.name.ends_with(".mix") {
                assert!(
                    matches!(&function.parameters[1].ty, zil_ir::Type::Tuple(fields)
                    if matches!(&fields[0], zil_ir::Type::Named(name) if name == "Meter")
                    && matches!(&fields[1], zil_ir::Type::Address { inner, .. } if matches!(inner.as_ref(), zil_ir::Type::Dyn { .. })))
                );
            }
        }
        // Until the executable consumer reconstructs the actual CFG claims,
        // missing contracts remain Unknown, never a finite empty permission.
        assert!(
            core.dyn_interfaces
                .iter()
                .flat_map(|interface| &interface.slots)
                .all(|slot| slot.contract.effects.is_none() && slot.contract.call.is_none())
        );
    }

    #[test]
    fn dyn_source_core_keeps_inherited_default_targets_with_shared_spans() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(17),
            "dyn-default.zil",
            include_str!(
                "../../../tests/language-readiness/terminal/generics-and-constants/dyn-source/inherited-default.zil"
            ),
        );
        let mut parsed = zil_syntax::parse(&source).into_result().unwrap();
        let shared = parsed.items[0].span;
        for item in &mut parsed.items {
            item.span = shared;
            let members = match &mut item.kind {
                ItemKind::Trait(declaration) => &mut declaration.members,
                ItemKind::Impl(implementation) => &mut implementation.members,
                _ => continue,
            };
            for member in members {
                member.span = shared;
                if let ItemKind::Function(function) = &mut member.kind {
                    function.span = shared;
                    for parameter in &mut function.params {
                        parameter.span = shared;
                    }
                }
            }
        }
        let aliases = BTreeMap::new();
        let expanded = crate::expand_with_map_limits_target_context(
            &source,
            &parsed,
            &Limits::default(),
            &zil_base::TargetProfile::default(),
            crate::source_context::SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &aliases,
                declarations: crate::source_context::DeclarationPlacement::Original,
            },
        )
        .unwrap();
        let core = expanded.core;
        assert_eq!(core.dyn_interfaces.len(), 1);
        assert_eq!(core.dyn_interfaces[0].inherited.len(), 1);
        assert_eq!(core.dyn_vtables.len(), 1);
        let entries = &core.dyn_vtables[0].entries;
        assert_eq!(entries.len(), 2);
        assert_ne!(entries[0].function, entries[1].function);
        for entry in entries {
            assert!(
                core.functions
                    .iter()
                    .any(|function| function.id == entry.function)
            );
        }
        let declaration_ids = core
            .dyn_declarations
            .iter()
            .map(|declaration| declaration.id.declaration.0)
            .collect::<Vec<_>>();
        assert_eq!(declaration_ids, [0, 1, 3, 4]);
    }

    #[test]
    fn dyn_source_core_prepays_complete_request_and_requires_context() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(17),
            "dyn-budget.zil",
            "trait Read{fn read(self:&Self)->i32;}struct Meter{n:i32}\
                 impl Read for Meter{fn read(self:&Self)->i32{return self.n}}\
                 fn peek(x:&dyn Read)->i32{return x.read()}fn main(){}",
        );
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        assert_eq!(
            crate::expand_with_map(&source, &parsed).unwrap_err()[0].category,
            zil_base::ResultCategory::Unsupported
        );
        let retained = SourceUnit::new(
            FileId(0),
            ModuleId(17),
            "dyn-retained.zil",
            "trait Marker<T:Copy>{fn read(self:&Self)->i32;}struct Tag{n:i32}struct Meter{n:i32}\
             impl Marker<Tag> for Meter{fn read(self:&Self)->i32{return self.n}}\
             fn peek(x:&dyn Marker<Tag>)->i32{return x.read()}fn main(){}",
        );
        let retained_ast = zil_syntax::parse(&retained).into_result().unwrap();
        let retained_aliases = BTreeMap::new();
        let rejected = crate::expand_with_map_limits_target_context(
            &retained,
            &retained_ast,
            &Limits::default(),
            &zil_base::TargetProfile::default(),
            crate::source_context::SourceIdentityContext::ClosedSnapshot {
                root: retained.module_id,
                aliases: &retained_aliases,
                declarations: crate::source_context::DeclarationPlacement::Original,
            },
        )
        .unwrap_err();
        assert_eq!(rejected[0].category, zil_base::ResultCategory::Unsupported);
        assert!(
            rejected[0].message.contains("retained type-class"),
            "{rejected:?}"
        );
        let aliases = BTreeMap::new();
        let run = |limits: &Limits| {
            crate::expand_with_map_limits_target_context(
                &source,
                &parsed,
                limits,
                &zil_base::TargetProfile::default(),
                crate::source_context::SourceIdentityContext::ClosedSnapshot {
                    root: source.module_id,
                    aliases: &aliases,
                    declarations: crate::source_context::DeclarationPlacement::Original,
                },
            )
        };
        for nodes in [false, true] {
            let (mut low, mut high) = (
                0,
                if nodes {
                    Limits::default().max_nodes
                } else {
                    Limits::default().max_analysis_steps
                },
            );
            while low < high {
                let middle = low + (high - low) / 2;
                let mut limits = Limits::default();
                if nodes {
                    limits.max_nodes = middle;
                } else {
                    limits.max_analysis_steps = middle;
                }
                if run(&limits).is_ok() {
                    high = middle;
                } else {
                    low = middle + 1;
                }
            }
            let mut limits = Limits::default();
            if nodes {
                limits.max_nodes = low;
            } else {
                limits.max_analysis_steps = low;
            }
            assert!(run(&limits).is_ok());
            if nodes {
                limits.max_nodes -= 1;
            } else {
                limits.max_analysis_steps -= 1;
            }
            assert_eq!(
                run(&limits).unwrap_err()[0].category,
                zil_base::ResultCategory::ResourceLimit
            );
            println!(
                "dyn-source-core {}={low}",
                if nodes { "nodes" } else { "work" }
            );
        }
    }
    fn catalog(text: &str) -> Result<Catalog, Vec<Diagnostic>> {
        let source = SourceUnit::new(FileId(0), ModuleId(17), "dyn.zil", text);
        let parsed = zil_syntax::parse(&source).into_result()?;
        let mut products = products::Products::default();
        crate::generics::specialize_products(&parsed, &Limits::default(), &mut products)?;
        Ok(products.dynamic)
    }

    #[test]
    fn dyn_catalog_keeps_inherited_member_and_provider_origins() {
        let catalog = catalog(
            "trait Base { fn read(self:&Self)->i32 { return 3 } }\
             trait Child:Base { fn child(self:&Self)->i32; }\
             struct Meter{n:i32}\
             impl Base for Meter {}\
             impl Child for Meter { fn child(self:&Self)->i32{return self.n} }\
             fn peek(x:&dyn Child)->i32{return x.child()} fn main(){}",
        )
        .unwrap();
        assert_eq!(catalog.interfaces.len(), 1);
        let interface = &catalog.interfaces[0];
        assert_eq!(interface.declaration.module, ModuleId(17));
        assert_eq!(interface.inherited.len(), 1);
        assert_eq!(interface.slots.len(), 2);
        assert_ne!(
            interface.slots[0].declaration,
            interface.slots[1].declaration
        );
        assert!(interface.slots.iter().all(|slot| !slot.contract_written));
        assert_eq!(catalog.vtables.len(), 1);
        let table = &catalog.vtables[0];
        assert_eq!(table.interface, 0);
        assert_eq!(table.implementation.ordinal, 4);
        assert_eq!(table.providers.len(), 2);
        let read = table
            .providers
            .iter()
            .find(|p| p.signature.name == "read")
            .unwrap();
        assert_eq!(read.declaration.declaration.ordinal, 0);
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        assert_eq!(
            type_key(&table.self_type, &mut budget).unwrap(),
            type_key(&read.substitutions["Self"], &mut budget).unwrap()
        );
        let child = table
            .providers
            .iter()
            .find(|p| p.signature.name == "child")
            .unwrap();
        assert_eq!(child.declaration.declaration.ordinal, 4);
    }

    #[test]
    fn dyn_catalog_requires_actual_trait_and_argument_obligations() {
        for source in [
            "fn peek(x:&dyn Missing){}fn main(){}",
            "resource R{n:i32}trait Read<T:Copy>{fn read(self:&Self)->i32;}fn peek(x:&dyn Read<R>){}fn main(){}",
            "trait Read{}impl Read for Missing{}fn peek(x:&dyn Read){}fn main(){}",
        ] {
            assert!(catalog(source).is_err(), "{source}");
        }
        let concrete = catalog(
            "trait Read{}struct Meter{n:i32}impl Read for Meter{}fn peek(x:&dyn Read){}fn main(){}",
        )
        .unwrap();
        assert_eq!(concrete.vtables.len(), 1);
        let unresolved = catalog("trait Read<T>{}trait A{}struct Meter{n:i32}impl<T:A>Read<T> for Meter{}fn peek(x:&dyn Read<i32>){}fn main(){}").unwrap();
        assert!(unresolved.vtables.is_empty());
    }
}
