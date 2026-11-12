//! Late dyn Context materialization in the actual source lifetime scope.
//! Compiler tokens point to complete catalog entries; generated nominal names
//! never recover a lifetime or an implementation witness.
use super::*;
use crate::stored_types::{self, QualifiedActual};

impl Engine<'_> {
    pub(super) fn prepare_owned_dyn_context(
        &mut self,
        ty: &mut TypeRef,
        depth: usize,
    ) -> R<Option<QualifiedActual>> {
        let TypeKind::Named { path, args } = &ty.kind else {
            return Ok(None);
        };
        if path.as_slice() != ["OwnedDyn"] || args.len() != 2 {
            return Ok(None);
        }
        let [GenericArg::Type(target), GenericArg::Type(context)] = args.as_slice() else {
            return Err(error(
                ty.span,
                "OwnedDyn requires a dyn interface and an actual Context type",
            ));
        };
        self.budget.ty(target)?;
        self.budget.ty(context)?;
        let target = target.clone();
        let mut context = context.clone();
        let actual = self
            .qualified_type_actual(&mut context, depth + 1)?
            .ok_or_else(|| {
                unsupported(
                    ty.span,
                    "dyn Context requires a complete qualified source application",
                )
            })?;
        if !matches!(actual.ty, zil_ir::Type::BorrowedStorage { .. }) {
            return Err(unsupported(
                ty.span,
                "dyn Context requires an authenticated inline storage class shape",
            ));
        }
        let (symbol, lowered) = self.products.dynamic.materialize_context(
            &self.traits,
            &target,
            actual.ty,
            &mut self.budget,
            ty.span,
        )?;
        *ty = named(&symbol, ty.span);
        Ok(Some(QualifiedActual {
            name: symbol,
            ty: lowered,
            classes: actual.classes,
        }))
    }
}

pub(super) fn retained(
    context: &zil_ir::Type,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<zil_ir::RetainedLifeShape> {
    stored_types::pay_type(context, budget, span, 0)?;
    let zil_ir::Type::BorrowedStorage { storage, life } = context else {
        return Err(error(
            span,
            "dyn Context has no complete physical class shape",
        ));
    };
    let mut ports: Vec<zil_ir::RetainedBorrowPort> = Vec::new();
    for slot in &life.slots {
        budget.node(span, 0)?;
        let mut ty = storage.as_ref();
        for step in &slot.path {
            budget.node(span, 0)?;
            if let zil_ir::Type::BorrowedStorage { storage, .. } = ty {
                ty = storage;
            }
            ty = match (ty, step) {
                (
                    zil_ir::Type::Record { fields, .. } | zil_ir::Type::Package { fields, .. },
                    zil_ir::PlaceProjection::Field(index),
                ) => fields.get(*index as usize).map(|(_, ty)| ty),
                (zil_ir::Type::Tuple(fields), zil_ir::PlaceProjection::Field(index)) => {
                    fields.get(*index as usize)
                }
                (
                    zil_ir::Type::Array { element, length },
                    zil_ir::PlaceProjection::Field(index),
                ) if *index < *length => Some(element.as_ref()),
                (
                    zil_ir::Type::Enum { variants, .. },
                    zil_ir::PlaceProjection::Variant(projection),
                ) => variants
                    .get(projection.variant as usize)
                    .and_then(|(_, fields)| fields.get(projection.index as usize)),
                (
                    zil_ir::Type::ErrorUnion { error, result },
                    zil_ir::PlaceProjection::ErrorArm(error_arm),
                ) => Some(if *error_arm {
                    error.as_ref()
                } else {
                    result.as_ref()
                }),
                _ => None,
            }
            .ok_or_else(|| error(span, "dyn Context slot crosses an absent physical field"))?;
        }
        stored_types::pay_type(ty, budget, span, 0)?;
        let candidate = zil_ir::RetainedBorrowPort {
            leaf: ty.clone(),
            mode: slot.mode,
            binder: slot.binder,
        };
        for prior in &ports {
            stored_types::pay_type(&prior.leaf, budget, span, 0)?;
        }
        if !ports.contains(&candidate) {
            ports.push(candidate);
        }
    }
    // These rows retain declaration occurrences and phantom classes. They
    // constrain type identity and never supply a target for an empty port.
    Ok(zil_ir::RetainedLifeShape {
        binder_count: life.binder_count,
        ports,
        retained: life.retained.clone(),
        nominal_arguments: life.nominal_arguments.clone(),
    })
}

impl Engine<'_> {
    // Matching binds only declared impl lifetimes, in the current source domain.
    // Full Type actuals and every constructor are still compared structurally.
    fn dyn_match_life(
        &mut self,
        pattern: &str,
        actual: &str,
        formals: &BTreeSet<String>,
        found: &mut BTreeMap<String, crate::stored_types::LexicalLife>,
        span: Span,
    ) -> R<bool> {
        self.budget.text(pattern, span)?;
        self.budget.text(actual, span)?;
        self.budget
            .identity_work(formals.len().saturating_add(found.len()), span)?;
        if !formals.contains(pattern) {
            return Err(error(span, "dyn impl uses an undeclared lifetime"));
        }
        let actual = self.source_life(actual, span)?;
        if let Some(prior) = found.get(pattern) {
            return Ok(*prior == actual);
        }
        found.insert(pattern.to_owned(), actual);
        Ok(true)
    }
    fn dyn_match_type(
        &mut self,
        p: &TypeRef,
        a: &TypeRef,
        formals: &BTreeSet<String>,
        found: &mut BTreeMap<String, crate::stored_types::LexicalLife>,
        d: usize,
    ) -> R<bool> {
        self.budget.node(p.span, d)?;
        self.budget.node(a.span, d)?;
        if let TypeKind::Group(p) = &p.kind {
            return self.dyn_match_type(p, a, formals, found, d + 1);
        }
        if let TypeKind::Group(a) = &a.kind {
            return self.dyn_match_type(p, a, formals, found, d + 1);
        }
        Ok(match (&p.kind, &a.kind) {
            (TypeKind::Named { path: p, args: pa }, TypeKind::Named { path: a, args: aa }) => {
                for s in p.iter().chain(a) {
                    self.budget.text(s, Span::default())?;
                }
                if p != a || pa.len() != aa.len() {
                    return Ok(false);
                }
                for (p, a) in pa.iter().zip(aa) {
                    match (p, a) {
                        (GenericArg::Lifetime(p), GenericArg::Lifetime(a)) => {
                            if !self.dyn_match_life(p, a, formals, found, Span::default())? {
                                return Ok(false);
                            }
                        }
                        (GenericArg::Type(p), GenericArg::Type(a)) => {
                            if !self.dyn_match_type(p, a, formals, found, d + 1)? {
                                return Ok(false);
                            }
                        }
                        (GenericArg::Const(p), GenericArg::Const(a)) => {
                            self.budget.const_expression_depth(p, d + 1)?;
                            self.budget.const_expression_depth(a, d + 1)?;
                            if consts::key(p)? != consts::key(a)? {
                                return Ok(false);
                            }
                        }
                        _ => return Ok(false),
                    }
                }
                true
            }
            (
                TypeKind::Reference {
                    lifetime: p,
                    mutable: pm,
                    inner: pi,
                },
                TypeKind::Reference {
                    lifetime: a,
                    mutable: am,
                    inner: ai,
                },
            ) => {
                if pm != am {
                    return Ok(false);
                }
                match (p, a) {
                    (Some(p), Some(a)) => {
                        if !self.dyn_match_life(p, a, formals, found, pi.span)? {
                            return Ok(false);
                        }
                    }
                    (None, None) => {}
                    _ => return Ok(false),
                }
                self.dyn_match_type(pi, ai, formals, found, d + 1)?
            }
            (TypeKind::Tuple(p), TypeKind::Tuple(a)) => {
                if p.len() != a.len() {
                    return Ok(false);
                }
                for (p, a) in p.iter().zip(a) {
                    if !self.dyn_match_type(p, a, formals, found, d + 1)? {
                        return Ok(false);
                    }
                }
                true
            }
            (
                TypeKind::Error {
                    error: p,
                    result: pr,
                },
                TypeKind::Error {
                    error: a,
                    result: ar,
                },
            ) => {
                self.dyn_match_type(p, a, formals, found, d + 1)?
                    && self.dyn_match_type(pr, ar, formals, found, d + 1)?
            }
            _ => type_key_bounded(p, &mut self.budget)? == type_key_bounded(a, &mut self.budget)?,
        })
    }
    pub(super) fn materialize_dyn_provider(
        &mut self,
        owner: &QualifiedActual,
        source: &TypeRef,
        span: Span,
        depth: usize,
    ) -> R<usize> {
        let mut selected_interface = None;
        for (symbol, ordinal, owned) in &self.products.dynamic.context_types {
            self.budget.text(symbol, span)?;
            self.budget.text(&owner.name, span)?;
            if *owned && symbol == &owner.name {
                selected_interface = Some(*ordinal);
            }
        }
        let interface = selected_interface
            .ok_or_else(|| error(span, "dyn provider has no authentic Context recipe"))?;
        self.budget.ty(source)?;
        let mut source_type = source.clone();
        let actual = self
            .qualified_type_actual(&mut source_type, depth + 1)?
            .ok_or_else(|| {
                unsupported(
                    span,
                    "dyn borrowed provider requires its complete source application",
                )
            })?;
        let reference = dynamic::copy_reference(
            &self.products.dynamic.interfaces[interface].reference,
            &mut self.budget,
            span,
        )?;
        if !self.products.dynamic.interfaces[interface]
            .inherited
            .is_empty()
        {
            return Err(unsupported(
                span,
                "dyn lifetime provider inherited impl premises need their own committed witnesses",
            ));
        }
        let mut selected = None;
        for index in 0..self.traits.recipes.len() {
            let item = self.traits.recipes[index];
            self.budget.node(item.span, 0)?;
            let ItemKind::Impl(implementation) = &item.kind else {
                unreachable!()
            };
            let Some(application) = &implementation.trait_type else {
                continue;
            };
            if trait_instances::head(application) != Some(reference.key.name.as_str()) {
                continue;
            }
            let candidate = self.traits.reference(application, &mut self.budget)?;
            if candidate.key != reference.key {
                continue;
            }
            if implementation
                .generics
                .iter()
                .any(|p| !matches!(p.kind, GenericParamKind::Lifetime))
            {
                return Err(unsupported(
                    item.span,
                    "dyn Context provider requires complete type/const impl premise solving",
                ));
            }
            let mut formals = BTreeSet::new();
            for p in &implementation.generics {
                self.budget.text(&p.name, p.span)?;
                if !formals.insert(p.name.clone()) {
                    return Err(error(p.span, "duplicate dyn impl lifetime"));
                }
            }
            let mut found = BTreeMap::new();
            if !self.dyn_match_type(
                &implementation.target,
                source,
                &formals,
                &mut found,
                depth + 1,
            )? {
                continue;
            }
            if !implementation.where_clause.is_empty() {
                return Err(unsupported(
                    item.span,
                    "dyn lifetime provider where premises are not proved",
                ));
            }
            if found.len() != formals.len() {
                return Err(unsupported(
                    item.span,
                    "dyn impl lifetime lacks an actual source occurrence",
                ));
            }
            if selected.is_some() {
                return Err(error(
                    span,
                    "dyn provider has overlapping actual impl witnesses",
                ));
            }
            selected = Some((item, found));
        }
        let (item, found) = selected
            .ok_or_else(|| error(span, "dyn provider has no actual lifetime impl witness"))?;
        let ItemKind::Impl(implementation) = &item.kind else {
            unreachable!()
        };
        let origin = self
            .traits
            .origin(item, &mut self.budget)?
            .ok_or_else(|| error(span, "dyn impl source origin absent"))?;
        let context = self.products.dynamic.interfaces[interface]
            .context
            .as_ref()
            .ok_or_else(|| error(span, "dyn Context metadata absent"))?;
        let ports = retained(context, &mut self.budget, span)?;
        let provider_ports = retained(&actual.ty, &mut self.budget, span)?;
        let zil_ir::Type::BorrowedStorage { life, .. } = &actual.ty else {
            unreachable!()
        };
        let mut class_map = Vec::new();
        for class in &actual.classes {
            self.budget.identity_work(owner.classes.len(), span)?;
            let index = owner
                .classes
                .iter()
                .position(|p| p == class)
                .ok_or_else(|| error(span, "dyn provider class is outside its declared Context"))?;
            class_map.push(
                u32::try_from(index)
                    .map_err(|_| error(span, "dyn Context class index overflow"))?,
            );
        }
        let mut mappings = Vec::new();
        for (index, port) in ports.ports.iter().enumerate() {
            let mut sources = Vec::new();
            for slot in &life.slots {
                self.budget.node(span, 0)?;
                let leaf = slot_leaf(&actual.ty, &slot.path, &mut self.budget, span)?;
                if slot.mode == port.mode
                    && class_map.get(slot.binder as usize) == Some(&port.binder)
                    && leaf == &port.leaf
                {
                    self.budget.identity_work(slot.path.len(), span)?;
                    sources.push(zil_ir::StoragePosition {
                        base: zil_ir::StorageBase::Result,
                        path: slot
                            .path
                            .iter()
                            .cloned()
                            .map(zil_ir::ContractProjection::Place)
                            .collect(),
                    });
                }
            }
            mappings.push(zil_ir::DynRetainedPortBinding {
                port: index as u32,
                sources,
            });
        }
        for port in &provider_ports.ports {
            if !ports.ports.iter().any(|p| {
                p.leaf == port.leaf
                    && p.mode == port.mode
                    && class_map.get(port.binder as usize) == Some(&p.binder)
            }) {
                return Err(error(
                    span,
                    "dyn Context omits an actual provider borrow leaf",
                ));
            }
        }
        let mut actuals = Vec::new();
        for formal in &implementation.generics {
            let life = found[&formal.name];
            self.budget.identity_work(owner.classes.len(), span)?;
            let index = owner
                .classes
                .iter()
                .position(|p| *p == life)
                .ok_or_else(|| error(span, "dyn impl actual is outside Context class domain"))?;
            actuals.push(zil_ir::DynGenericActual::Life(
                zil_ir::StorageBinder::Fixed(index as u32),
            ));
        }
        // Individually alpha-normalized Self and Context types do not identify
        // their joint application. Reuse only the complete actual port relation
        // and impl lifetime actuals as well; swapped same-type roots stay distinct.
        for (ordinal, table) in self.products.dynamic.vtables.iter().enumerate() {
            self.budget.node(span, 0)?;
            if table.interface != interface || table.implementation != origin.declaration {
                continue;
            }
            let Some(prior) = &table.qualified_self else {
                continue;
            };
            stored_types::pay_type(prior, &mut self.budget, span, 0)?;
            stored_types::pay_type(&actual.ty, &mut self.budget, span, 0)?;
            self.budget
                .identity_work(table.actuals.len().saturating_add(actuals.len()), span)?;
            for row in table.ports.iter().chain(&mappings) {
                self.budget.node(span, 0)?;
                for source in &row.sources {
                    self.budget
                        .identity_work(source.path.len().saturating_add(1), span)?;
                }
            }
            if prior == &actual.ty
                && table.ports == mappings
                && table.actuals == actuals
                && table
                    .self_classes
                    .iter()
                    .copied()
                    .eq(class_map.iter().copied().map(zil_ir::StorageBinder::Fixed))
            {
                return Ok(ordinal);
            }
        }
        let table_index = self.products.dynamic.vtables.len();
        let mut providers = Vec::new();
        let slot_count = self.products.dynamic.interfaces[interface].slots.len();
        for slot_index in 0..slot_count {
            let slot = &self.products.dynamic.interfaces[interface].slots[slot_index];
            self.budget.const_function(&slot.signature)?;
            let signature = slot.signature.clone();
            let mut selected = None;
            for (ordinal, member) in implementation.members.iter().enumerate() {
                self.budget.node(member.span, 0)?;
                if let ItemKind::Function(method) = &member.kind {
                    self.budget.text(&method.name, member.span)?;
                    if method.name == signature.name {
                        if selected.is_some() {
                            return Err(error(member.span, "duplicate dyn provider method"));
                        }
                        selected = Some((ordinal, member, method));
                    }
                }
            }
            let (ordinal, member, method) = selected.ok_or_else(|| {
                unsupported(
                    span,
                    "dyn lifetime provider requires an actual method body witness",
                )
            })?;
            if !method.generics.is_empty()
                || !method.where_clause.is_empty()
                || matches!(method.body, FunctionBody::Declaration)
            {
                return Err(unsupported(
                    member.span,
                    "dyn provider method has unresolved binders or premises",
                ));
            }
            self.budget.item(member)?;
            let mut generated = method.clone();
            if !generated.params.first().is_some_and(|p| p.name == "self") {
                let receiver = signature
                    .params
                    .first()
                    .filter(|p| p.name == "self")
                    .ok_or_else(|| error(span, "dyn slot receiver absent"))?;
                self.budget.ty(&receiver.ty)?;
                generated.params.insert(0, receiver.clone());
            }
            let substitutions =
                BTreeMap::from([("Self".to_owned(), implementation.target.clone())]);
            let mut generated_item = Item {
                span: member.span,
                public: false,
                attributes: Vec::new(),
                kind: ItemKind::Function(generated),
            };
            self.budget
                .substituted_item(&generated_item, &substitutions)?;
            substitute_item(&mut generated_item, &substitutions)?;
            let mut occupied = BTreeSet::new();
            collect_names(&generated_item, &mut occupied, &mut self.budget)?;
            let mut names = BTreeMap::new();
            let mut joint = Vec::new();
            let mut formals = Vec::new();
            for formal in &implementation.generics {
                let class = found[&formal.name];
                self.budget.identity_work(joint.len(), span)?;
                let at = if let Some(i) = joint.iter().position(|p| *p == class) {
                    i
                } else {
                    let mut nonce = joint.len();
                    let name = loop {
                        self.budget.identity_work(64, span)?;
                        let candidate = format!("$dyn_life_{nonce}");
                        if !occupied.contains(&candidate) {
                            break candidate;
                        }
                        nonce = nonce
                            .checked_add(1)
                            .ok_or_else(|| error(span, "dyn lifetime alpha identity exhausted"))?;
                    };
                    self.budget.text(&name, span)?;
                    occupied.insert(name.clone());
                    joint.push(class);
                    formals.push(GenericParam {
                        name,
                        kind: GenericParamKind::Lifetime,
                        span,
                    });
                    joint.len() - 1
                };
                self.budget.text(&formal.name, span)?;
                self.budget.text(&formals[at].name, span)?;
                names.insert(formal.name.clone(), formals[at].name.clone());
            }
            let ItemKind::Function(generated) = &mut generated_item.kind else {
                unreachable!()
            };
            source_life_alpha::function(generated, &names, &mut self.budget)?;
            generated.generics = formals;
            self.budget.identity_work(96, span)?;
            generated.name = format!("$generic_method_dyn_{table_index}_{slot_index}");
            let generated_name = generated.name.clone();
            trait_instances::rename_default_receiver(&mut generated_item, &mut self.budget)?;
            let mut member_origin = origin.member(ordinal, member.span)?;
            member_origin.provider = true;
            self.enqueue(generated_item, Some(member_origin))?;
            providers.push(dynamic::Provider {
                generated: Some(generated_name),
                implementation: origin.declaration,
                declaration: member_origin.id(member.span)?,
                substitutions,
                signature,
            });
        }
        self.products.dynamic.vtables.push(dynamic::Vtable {
            self_classes: class_map
                .into_iter()
                .map(zil_ir::StorageBinder::Fixed)
                .collect(),
            qualified_self: Some(actual.ty),
            ports: mappings,
            actuals,
            interface,
            implementation: origin.declaration,
            self_type: source_type,
            providers,
        });
        Ok(table_index)
    }
}
fn collect_names(item: &Item, names: &mut BTreeSet<String>, budget: &mut Budget<'_>) -> R<()> {
    use crate::ct_selection::Visitor;
    struct Names<'a, 'b, 'l> {
        names: &'a mut BTreeSet<String>,
        budget: &'b mut Budget<'l>,
    }
    impl Visitor for Names<'_, '_, '_> {
        fn node(&mut self, s: Span, d: usize) -> CompileResult<()> {
            self.budget.node(s, d).map_err(|e| vec![e])
        }
        fn text(&mut self, n: &str, s: Span) -> CompileResult<()> {
            self.budget.text(n, s).map_err(|e| vec![e])?;
            self.budget
                .identity_work(
                    self.names
                        .len()
                        .checked_ilog2()
                        .map_or(1, |n| n as usize + 2),
                    s,
                )
                .map_err(|e| vec![e])?;
            self.names.insert(n.to_owned());
            Ok(())
        }
    }
    Names { names, budget }
        .item(item, 0)
        .map_err(|mut e| e.remove(0))
}
fn slot_leaf<'a>(
    ty: &'a zil_ir::Type,
    path: &[zil_ir::PlaceProjection],
    budget: &mut Budget<'_>,
    span: Span,
) -> R<&'a zil_ir::Type> {
    let mut ty = ty;
    for step in path {
        budget.node(span, 0)?;
        if let zil_ir::Type::BorrowedStorage { storage, .. } = ty {
            ty = storage;
        }
        ty = match (ty, step) {
            (
                zil_ir::Type::Record { fields, .. } | zil_ir::Type::Package { fields, .. },
                zil_ir::PlaceProjection::Field(i),
            ) => fields.get(*i as usize).map(|(_, t)| t),
            (zil_ir::Type::Tuple(fields), zil_ir::PlaceProjection::Field(i)) => {
                fields.get(*i as usize)
            }
            (zil_ir::Type::Array { element, length }, zil_ir::PlaceProjection::Field(i))
                if *i < *length =>
            {
                Some(element.as_ref())
            }
            (zil_ir::Type::Enum { variants, .. }, zil_ir::PlaceProjection::Variant(p)) => variants
                .get(p.variant as usize)
                .and_then(|(_, fs)| fs.get(p.index as usize)),
            (zil_ir::Type::ErrorUnion { error, result }, zil_ir::PlaceProjection::ErrorArm(e)) => {
                Some(if *e { error.as_ref() } else { result.as_ref() })
            }
            _ => None,
        }
        .ok_or_else(|| error(span, "dyn source leaf has no actual physical projection"))?;
    }
    stored_types::pay_type(ty, budget, span, 0)?;
    Ok(ty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, ModuleId, SourceUnit};
    fn expand(text: &str) -> zil_ir::CoreModule {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "dyn-context.zil", text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        crate::expand_with_map_limits_target_context(
            &source,
            &parsed,
            &Limits::default(),
            &zil_base::TargetProfile::default(),
            crate::source_context::SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &BTreeMap::new(),
                declarations: crate::source_context::DeclarationPlacement::Original,
            },
        )
        .unwrap()
        .core
    }
    #[test]
    fn source_context_materializes_real_lifetime_impl_and_actual_leaf_paths() {
        let module = expand(
            r#"
struct ReadContext<life a>{leaf:&'a i32}
struct P<life a>{pointer:&'a i32}
trait Read{fn read(self:&Self)->i32;}
impl<life a> Read for P<'a>{fn read(self:&Self)->i32{return *self.pointer}}
fn erase<life a>(p:P<'a>)->DynCreateError!OwnedDyn<dyn Read,ReadContext<'a>>{return owned_dyn::<dyn Read,ReadContext<'a>>(p)}
fn main()->i32{return 0}
"#,
        );
        let interface = module
            .dyn_interfaces
            .iter()
            .find(|i| i.context.is_some())
            .unwrap();
        assert_eq!(interface.retained_life.ports.len(), 1);
        let table = module
            .dyn_vtables
            .iter()
            .find(|t| t.interface == interface.id)
            .unwrap();
        assert!(matches!(
            table.self_type,
            zil_ir::Type::BorrowedStorage { .. }
        ));
        assert_eq!(table.self_classes, [zil_ir::StorageBinder::Fixed(0)]);
        assert!(
            module
                .functions
                .iter()
                .flat_map(|f| &f.body.statements)
                .any(|statement| matches!(
                    &statement.kind,
                    zil_ir::CoreStatementKind::Return(Some(zil_ir::CoreExpr {
                        kind: zil_ir::CoreExprKind::MakeDynOwned { vtable, .. }, ..
                    })) if *vtable == table.id
                ))
        );
        assert_eq!(table.ports.len(), 1);
        assert_eq!(table.ports[0].sources[0].base, zil_ir::StorageBase::Result);
        assert_eq!(
            table.ports[0].sources[0].path,
            [zil_ir::ContractProjection::Place(
                zil_ir::PlaceProjection::Field(0)
            )]
        );
        assert!(
            module
                .functions
                .iter()
                .any(|f| f.id == table.entries[0].function)
        );
    }
    #[test]
    fn source_constructor_keeps_same_self_with_different_joint_context_maps() {
        let module = expand(
            r#"
struct Context<life a,life b>{left:&'a i32,right:&'b i32}
struct P<life a>{pointer:&'a i32}
trait Read{fn read(self:&Self)->i32;}
impl<life x> Read for P<'x>{fn read(self:&Self)->i32{return *self.pointer}}
fn left<life a,life b>(p:P<'a>,anchor:Context<'a,'b>)->DynCreateError!OwnedDyn<dyn Read,Context<'a,'b>>{return owned_dyn::<dyn Read,Context<'a,'b>>(p)}
fn right<life a,life b>(p:P<'b>,anchor:Context<'a,'b>)->DynCreateError!OwnedDyn<dyn Read,Context<'a,'b>>{return owned_dyn::<dyn Read,Context<'a,'b>>(p)}
fn main()->i32{return 0}
"#,
        );
        let contextual: Vec<_> = module
            .dyn_interfaces
            .iter()
            .filter(|item| item.context.is_some())
            .collect();
        assert_eq!(contextual.len(), 1);
        let tables: Vec<_> = module
            .dyn_vtables
            .iter()
            .filter(|table| table.interface == contextual[0].id)
            .collect();
        assert_eq!(tables.len(), 2);
        assert_eq!(tables[0].self_type, tables[1].self_type);
        assert_ne!(tables[0].self_classes, tables[1].self_classes);
        assert_ne!(tables[0].ports, tables[1].ports);
        let selected: BTreeSet<_> = module
            .functions
            .iter()
            .flat_map(|f| &f.body.statements)
            .filter_map(|statement| match &statement.kind {
                zil_ir::CoreStatementKind::Return(Some(zil_ir::CoreExpr {
                    kind: zil_ir::CoreExprKind::MakeDynOwned { vtable, .. },
                    ..
                })) => Some(*vtable),
                _ => None,
            })
            .collect();
        assert!(selected.contains(&tables[0].id));
        assert!(selected.contains(&tables[1].id));
    }
}
