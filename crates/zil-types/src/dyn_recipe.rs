//! Reconstruct original impl formals from actual nominal occurrences.
//! Canonical Self class ordinals are not declaration parameter ordinals.
use super::*;

struct Proof<'a, 'b> {
    modules: &'a [&'a CoreModule],
    table: &'a CoreDynVtable,
    implementation: &'a DynDeclaration,
    root_life: Option<&'a StorageLifeShape>,
    limits: &'a Limits,
    left: &'b mut usize,
}
impl Proof<'_, '_> {
    fn fail(&self) -> Diagnostic {
        unsupported(
            Span::default(),
            "dyn impl recipe differs from the actual complete nominal application",
        )
    }
    fn pay(&mut self, n: usize) -> R<()> {
        *self.left = self.left.checked_sub(n).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "dyn implementation substitution work exceeded",
                None,
            )
        })?;
        Ok(())
    }
    fn actual(&mut self, ordinal: u32, kind: &LifeParameterKind) -> R<&DynGenericActual> {
        self.pay(1)?;
        if self.implementation.parameters.get(ordinal as usize) != Some(kind) {
            return Err(self.fail());
        }
        self.table
            .actuals
            .get(ordinal as usize)
            .ok_or_else(|| self.fail())
    }
    fn constant(&mut self, pattern: &DynConstPattern, actual: &Constant) -> R<()> {
        self.pay(1)?;
        match pattern {
            DynConstPattern::Value(value) if value == actual => Ok(()),
            DynConstPattern::Parameter(i) => {
                let Some(LifeParameterKind::Const(_)) =
                    self.implementation.parameters.get(*i as usize)
                else {
                    return Err(self.fail());
                };
                if self.table.actuals.get(*i as usize)
                    == Some(&DynGenericActual::Const(actual.clone()))
                {
                    Ok(())
                } else {
                    Err(self.fail())
                }
            }
            _ => Err(self.fail()),
        }
    }
    fn closed(&mut self, ty: &Type) -> R<()> {
        inference::charge_type_snapshot(ty, Span::default(), self.limits, self.left, 2)?;
        if crate::dyn_dispatch::closed_owner_shape(ty, self.left, 0, self.limits)? == Some(0) {
            Ok(())
        } else {
            Err(self.fail())
        }
    }
    fn life(
        &mut self,
        ordinal: u32,
        declaration: LifeDeclarationId,
        argument: usize,
        local_class: u32,
        path: &[LifeTypeProjection],
    ) -> R<()> {
        self.pay(path.len().saturating_add(1))?;
        let shape = self.root_life.ok_or_else(|| self.fail())?;
        self.pay(
            shape
                .nominal_arguments
                .len()
                .saturating_mul(path.len().saturating_add(1)),
        )?;
        let mut matches = shape.nominal_arguments.iter().filter(|row| {
            row.path == path && row.declaration == declaration && row.argument as usize == argument
        });
        let row = matches.next().ok_or_else(|| self.fail())?;
        if matches.next().is_some() {
            return Err(self.fail());
        }
        let root_class = if path.is_empty() {
            local_class
        } else {
            self.pay(
                shape
                    .retained
                    .len()
                    .saturating_mul(path.len().saturating_add(1)),
            )?;
            let mut maps = shape
                .retained
                .iter()
                .filter(|map| map.path == path && map.class == local_class);
            let map = maps.next().ok_or_else(|| self.fail())?;
            if maps.next().is_some() {
                return Err(self.fail());
            }
            map.binder
        };
        if row.binder != root_class {
            return Err(self.fail());
        }
        let class = self
            .table
            .self_classes
            .get(row.binder as usize)
            .ok_or_else(|| self.fail())?;
        if self.actual(ordinal, &LifeParameterKind::Lifetime)? == &DynGenericActual::Life(*class) {
            Ok(())
        } else {
            Err(self.fail())
        }
    }
    fn nominal(
        &mut self,
        recipe: &DynSelfPattern,
        ty: &Type,
        path: &mut Vec<LifeTypeProjection>,
        depth: usize,
    ) -> R<()> {
        self.pay(1)?;
        if depth > self.limits.max_call_depth.min(128) {
            return Err(Diagnostic::limit(
                Phase::Types,
                "dyn impl recipe depth exceeded",
                None,
            ));
        }
        inference::charge_type_snapshot(ty, Span::default(), self.limits, self.left, 3)?;
        let mut found = None;
        let mut declaration = None;
        for module in self.modules {
            self.pay(
                module
                    .life_instances
                    .len()
                    .saturating_add(module.life_declarations.len()),
            )?;
            for instance in &module.life_instances {
                inference::charge_type_snapshot(
                    &instance.ty,
                    Span::default(),
                    self.limits,
                    self.left,
                    2,
                )?;
                if instance.ty == *ty && found.replace(instance).is_some() {
                    return Err(self.fail());
                }
            }
            for candidate in &module.life_declarations {
                if candidate.id == recipe.declaration && declaration.replace(candidate).is_some() {
                    return Err(self.fail());
                }
            }
        }
        let instance = found.ok_or_else(|| self.fail())?;
        let declaration = declaration.ok_or_else(|| self.fail())?;
        let physical = match ty {
            Type::BorrowedStorage { storage, .. } => storage.as_ref(),
            _ => ty,
        };
        let actual_kind = match physical {
            Type::Record { .. } => LifeNominalKind::Record,
            Type::Package { .. } => LifeNominalKind::Package,
            Type::Managed(_) => LifeNominalKind::Resource,
            Type::Enum { .. } => LifeNominalKind::Enum,
            _ => return Err(self.fail()),
        };
        if declaration.kind != actual_kind
            || instance.declaration != recipe.declaration
            || instance.arguments.len() != recipe.arguments.len()
            || declaration.parameters.len() != recipe.arguments.len()
        {
            return Err(self.fail());
        }
        self.pay(recipe.arguments.len())?;
        for (index, ((pattern, actual), kind)) in recipe
            .arguments
            .iter()
            .zip(&instance.arguments)
            .zip(&declaration.parameters)
            .enumerate()
        {
            match (pattern, actual, kind) {
                (
                    DynPatternActual::Life(zil_ir::DynLifePattern::DeclarationParameter(i)),
                    LifeNominalActual::Lifetime(local),
                    LifeParameterKind::Lifetime,
                ) => self.life(*i, recipe.declaration, index, *local, path)?,
                (
                    DynPatternActual::Const(p),
                    LifeNominalActual::Const(c),
                    LifeParameterKind::Const(_),
                ) => self.constant(p, c)?,
                (
                    DynPatternActual::Type(p),
                    LifeNominalActual::Type(t),
                    LifeParameterKind::Type,
                ) => {
                    path.push(LifeTypeProjection::TypeArgument(index as u32));
                    self.ty(p, t, path, depth + 1)?;
                    path.pop();
                }
                _ => return Err(self.fail()),
            }
        }
        Ok(())
    }
    fn ty(
        &mut self,
        pattern: &DynTypePattern,
        ty: &Type,
        path: &mut Vec<LifeTypeProjection>,
        depth: usize,
    ) -> R<()> {
        self.pay(1)?;
        if depth > self.limits.max_call_depth.min(128) {
            return Err(Diagnostic::limit(
                Phase::Types,
                "dyn impl type recipe depth exceeded",
                None,
            ));
        }
        match pattern {
            DynTypePattern::Concrete(expected) => {
                self.closed(expected)?;
                inference::charge_type_snapshot(ty, Span::default(), self.limits, self.left, 2)?;
                if expected == ty {
                    Ok(())
                } else {
                    Err(self.fail())
                }
            }
            DynTypePattern::Parameter(i) => {
                self.closed(ty)?;
                if self.actual(*i, &LifeParameterKind::Type)? == &DynGenericActual::Type(ty.clone())
                {
                    Ok(())
                } else {
                    Err(self.fail())
                }
            }
            DynTypePattern::Nominal(recipe) => self.nominal(recipe, ty, path, depth + 1),
            DynTypePattern::Tuple(patterns) => {
                let Type::Tuple(fields) = ty else {
                    return Err(self.fail());
                };
                if patterns.len() != fields.len() {
                    return Err(self.fail());
                }
                self.pay(fields.len())?;
                for (index, (p, t)) in patterns.iter().zip(fields).enumerate() {
                    path.push(LifeTypeProjection::Place(PlaceProjection::Field(
                        index as u32,
                    )));
                    self.ty(p, t, path, depth + 1)?;
                    path.pop();
                }
                Ok(())
            }
            // Address lifetime occurrences require the enclosing shape's
            // explicit slot/retained relation; never use a local class ordinal.
            _ => Err(self.fail()),
        }
    }
}

pub(super) fn prove(
    table: &CoreDynVtable,
    self_type: &Type,
    implementation: &DynDeclaration,
    interface: &DynInterface,
    modules: &[&CoreModule],
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let root_life = match self_type {
        Type::BorrowedStorage { life, .. } => Some(life),
        _ => None,
    };
    let mut proof = Proof {
        modules,
        table,
        implementation,
        root_life,
        limits,
        left,
    };
    proof.pay(
        implementation
            .parameters
            .len()
            .saturating_add(table.actuals.len())
            .saturating_add(1),
    )?;
    if implementation.kind != DynDeclarationKind::Impl
        || implementation.parameters.len() != table.actuals.len()
    {
        return Err(proof.fail());
    }
    for (kind, actual) in implementation.parameters.iter().zip(&table.actuals) {
        match (kind, actual) {
            (LifeParameterKind::Lifetime, DynGenericActual::Life(StorageBinder::Fixed(i)))
                if *i < interface.retained_life.binder_count => {}
            (LifeParameterKind::Type, DynGenericActual::Type(ty)) => proof.closed(ty)?,
            (LifeParameterKind::Const(carrier), DynGenericActual::Const(value))
                if *carrier == value.ty() => {}
            _ => return Err(proof.fail()),
        }
    }
    if let Some(recipe) = &implementation.self_pattern {
        proof.ty(recipe, self_type, &mut vec![], 0)?;
    } else if !implementation.parameters.is_empty()
        || root_life.is_some()
        || !table.self_classes.is_empty()
    {
        return Err(proof.fail());
    }
    let mut trait_declaration = None;
    for module in modules {
        proof.pay(module.dyn_declarations.len())?;
        for declaration in &module.dyn_declarations {
            if declaration.id == interface.application.declaration {
                if trait_declaration.is_some_and(|previous| previous != declaration) {
                    return Err(proof.fail());
                }
                trait_declaration = Some(declaration);
            }
        }
    }
    let trait_declaration = trait_declaration.ok_or_else(|| proof.fail())?;
    if trait_declaration.kind != DynDeclarationKind::Trait
        || trait_declaration.parameters.len() != interface.application.actuals.len()
    {
        return Err(proof.fail());
    }
    for (kind, actual) in trait_declaration
        .parameters
        .iter()
        .zip(&interface.application.actuals)
    {
        match (kind, actual) {
            (LifeParameterKind::Lifetime, DynGenericActual::Life(StorageBinder::Fixed(i)))
                if *i < interface.retained_life.binder_count => {}
            (LifeParameterKind::Type, DynGenericActual::Type(ty)) => proof.closed(ty)?,
            (LifeParameterKind::Const(carrier), DynGenericActual::Const(value))
                if *carrier == value.ty() => {}
            _ => return Err(proof.fail()),
        }
    }
    let recipe = implementation
        .implemented_trait
        .as_ref()
        .ok_or_else(|| proof.fail())?;
    proof.pay(
        recipe
            .arguments
            .len()
            .saturating_add(interface.application.actuals.len()),
    )?;
    if recipe.declaration != interface.application.declaration
        || recipe.arguments.len() != interface.application.actuals.len()
    {
        return Err(proof.fail());
    }
    for (pattern, actual) in recipe.arguments.iter().zip(&interface.application.actuals) {
        match (pattern, actual) {
            (
                DynPatternActual::Life(zil_ir::DynLifePattern::DeclarationParameter(i)),
                DynGenericActual::Life(_),
            ) if proof.actual(*i, &LifeParameterKind::Lifetime)? == actual => {}
            (DynPatternActual::Const(p), DynGenericActual::Const(c)) => proof.constant(p, c)?,
            (DynPatternActual::Type(p), DynGenericActual::Type(t)) => {
                proof.ty(p, t, &mut vec![], 0)?
            }
            _ => return Err(proof.fail()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{Fingerprint, ModuleId, NodeId};
    fn id(n: u32) -> LifeDeclarationId {
        LifeDeclarationId {
            module: ModuleId(0),
            declaration: NodeId(n),
            compiler: false,
        }
    }
    fn identity() -> LifeDeclarationIdentity {
        LifeDeclarationIdentity {
            package: "p".into(),
            module_path: vec![],
            declaration_path: vec!["P".into()],
            generation_path: vec![],
        }
    }
    fn fixture(merged: bool) -> (CoreModule, CoreDynVtable, DynDeclaration, DynInterface) {
        let nominal = id(1);
        let ty = Type::BorrowedStorage {
            storage: Box::new(Type::Record {
                name: "P".into(),
                fields: vec![],
            }),
            life: StorageLifeShape {
                binder_count: if merged { 1 } else { 2 },
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![
                    NominalLifeArgument {
                        path: vec![],
                        declaration: nominal,
                        argument: 0,
                        binder: 0,
                    },
                    NominalLifeArgument {
                        path: vec![],
                        declaration: nominal,
                        argument: 1,
                        binder: if merged { 0 } else { 1 },
                    },
                ],
            },
        };
        let module = CoreModule {
            semantic: None,
            dyn_declarations: vec![DynDeclaration {
                direct_supertraits: None,
                requirements: None,
                associated_definitions: None,
                method_signatures: None,
                method_definitions: None,
                self_pattern: None,
                implemented_trait: None,
                id: id(3),
                identity: identity(),
                kind: DynDeclarationKind::Trait,
                parameters: vec![LifeParameterKind::Lifetime],
                members: vec![],
            }],
            dyn_interfaces: vec![],
            dyn_vtables: vec![],
            life_declarations: vec![LifeNominalDeclaration {
                id: nominal,
                identity: identity(),
                kind: LifeNominalKind::Record,
                parameters: vec![LifeParameterKind::Lifetime, LifeParameterKind::Lifetime],
            }],
            life_instances: vec![LifeNominalInstance {
                declaration: nominal,
                ty: ty.clone(),
                arguments: vec![
                    LifeNominalActual::Lifetime(0),
                    LifeNominalActual::Lifetime(if merged { 0 } else { 1 }),
                ],
            }],
            closure_life_contexts: vec![],
            schema_version: SCHEMA_VERSION,
            module_id: ModuleId(0),
            source_fingerprint: Fingerprint::ZERO,
            functions: vec![],
            aggregates: vec![],
            aggregate_templates: vec![],
            aggregate_applications: vec![],
            enums: vec![],
            unsupported: vec![],
        };
        let table = CoreDynVtable {
            self_life: None,
            implementation_witnesses: None,
            self_classes: if merged {
                vec![StorageBinder::Fixed(0)]
            } else {
                vec![StorageBinder::Fixed(0), StorageBinder::Fixed(1)]
            },
            id: DynVtableId {
                module: ModuleId(0),
                index: 0,
            },
            interface: DynInterfaceId {
                module: ModuleId(0),
                index: 0,
            },
            self_type: ty.clone(),
            implementation: id(2),
            actuals: vec![
                DynGenericActual::Life(StorageBinder::Fixed(if merged { 0 } else { 1 })),
                DynGenericActual::Life(StorageBinder::Fixed(0)),
            ],
            ports: vec![],
            entries: vec![],
            cleanup: None,
            owned_storage: Some(DynOwnedStorage::Heap),
        };
        let implementation = DynDeclaration {
            direct_supertraits: None,
            requirements: None,
            associated_definitions: None,
            method_signatures: None,
            method_definitions: None,
            self_pattern: Some(zil_ir::DynTypePattern::Nominal(Box::new(DynSelfPattern {
                declaration: nominal,
                arguments: vec![
                    DynPatternActual::Life(zil_ir::DynLifePattern::DeclarationParameter(1)),
                    DynPatternActual::Life(zil_ir::DynLifePattern::DeclarationParameter(0)),
                ],
            }))),
            implemented_trait: Some(DynSelfPattern {
                declaration: id(3),
                arguments: vec![DynPatternActual::Life(
                    zil_ir::DynLifePattern::DeclarationParameter(0),
                )],
            }),
            id: id(2),
            identity: identity(),
            kind: DynDeclarationKind::Impl,
            parameters: vec![LifeParameterKind::Lifetime, LifeParameterKind::Lifetime],
            members: vec![],
        };
        let interface = DynInterface {
            context: Some(ty),
            owning_cleanup: None,
            id: table.interface,
            application: DynTraitApplication {
                declaration: id(3),
                actuals: vec![table.actuals[0].clone()],
            },
            inherited: vec![],
            associated_types: vec![],
            associated_constants: vec![],
            retained_life: RetainedLifeShape {
                binder_count: if merged { 1 } else { 2 },
                ports: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
            slots: vec![],
        };
        (module, table, implementation, interface)
    }
    fn run(m: &CoreModule, t: &CoreDynVtable, d: &DynDeclaration, i: &DynInterface) -> R<()> {
        let limits = Limits::default();
        let mut left = limits.max_analysis_steps;
        prove(t, &t.self_type, d, i, &[m], &limits, &mut left)
    }
    #[test]
    fn reordered_formals_and_equal_actuals_keep_all_occurrences() {
        for merged in [false, true] {
            let (m, t, d, i) = fixture(merged);
            run(&m, &t, &d, &i).unwrap();
        }
        let (m, mut t, d, i) = fixture(false);
        t.actuals.swap(0, 1);
        assert!(run(&m, &t, &d, &i).is_err());
    }
    #[test]
    fn phantom_trait_life_cannot_be_relabelled_by_matching_method_shape() {
        let (m, t, d, mut i) = fixture(false);
        i.application.actuals[0] = DynGenericActual::Life(StorageBinder::Fixed(0));
        assert!(run(&m, &t, &d, &i).is_err());
        let mut d = d;
        d.implemented_trait = None;
        assert!(run(&m, &t, &d, &i).is_err());
    }
    #[test]
    fn missing_occurrence_duplicate_instance_and_budget_exhaustion_are_not_empty() {
        let (mut m, t, d, i) = fixture(false);
        m.life_instances.push(m.life_instances[0].clone());
        assert!(run(&m, &t, &d, &i).is_err());
        m.life_instances.pop();
        let mut t = t;
        if let Type::BorrowedStorage { life, .. } = &mut t.self_type {
            life.nominal_arguments.pop();
        }
        assert!(run(&m, &t, &d, &i).is_err());
        let (m, t, d, i) = fixture(false);
        assert!(prove(&t, &t.self_type, &d, &i, &[&m], &Limits::default(), &mut 0).is_err());
    }
}
