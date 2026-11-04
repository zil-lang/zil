//! Source-owned dyn interface and static implementation recipes. These are
//! untrusted inputs for the public Types/CFG consumers, never permissions.
use super::*;

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
    pub(crate) declaration: origins::MemberId,
    pub(crate) substitutions: BTreeMap<String, TypeRef>,
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
    pub(crate) interfaces: Vec<Interface>,
    pub(crate) vtables: Vec<Vtable>,
    by_application: BTreeMap<trait_instances::Key, usize>,
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
    fn dynamic_interface(&self, ty: &TypeRef, budget: &mut Budget<'_>) -> R<Interface> {
        let TypeKind::Dyn { path, args, .. } = &ty.kind else {
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
        let reference = self.reference(&application, budget)?;
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
                let key = trait_instances::copy_key(&candidate.reference.key, self.budget, ty.span)
                    .map_err(|e| vec![e])?;
                if !self.catalog.by_application.contains_key(&key) {
                    self.node(ty.span, depth)?;
                    self.catalog
                        .by_application
                        .insert(key, self.catalog.interfaces.len());
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
    Ok(catalog)
}
