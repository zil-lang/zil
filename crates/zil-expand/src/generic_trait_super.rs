//! Supertrait closure and candidate commitment. A candidate does not enter the
//! implementation evidence set until every inherited premise has been proved.
use super::*;

#[derive(Clone, Copy)]
enum Host<'a> {
    Name(&'a str),
    Type(&'a TypeRef),
}

impl<'a> Index<'a> {
    fn inherited(
        &self,
        reference: &trait_instances::Reference,
        host: Option<Host<'_>>,
        budget: &mut Budget<'_>,
    ) -> R<Vec<Bound>> {
        let (item, declaration) = self.declarations[reference.key.name.as_str()];
        let origin = self.origin(item, budget)?;
        budget.with_origin(origin, |budget| {
            self.inherited_inner(declaration, reference, host, budget)
        })
    }
    fn inherited_inner(
        &self,
        declaration: &TraitDecl,
        reference: &trait_instances::Reference,
        host: Option<Host<'_>>,
        budget: &mut Budget<'_>,
    ) -> R<Vec<Bound>> {
        if declaration.bounds.is_empty() {
            return Ok(vec![]);
        }
        let mut substitutions = BTreeMap::new();
        if let Some(host) = host {
            budget.text("Self", reference.key.declaration)?;
            let actual = match host {
                Host::Name(name) => {
                    budget.text(name, reference.key.declaration)?;
                    named(name, reference.key.declaration)
                }
                Host::Type(ty) => {
                    budget.ty(ty)?;
                    ty.clone()
                }
            };
            substitutions.insert("Self".to_owned(), actual);
        }
        for (parameter, actual) in declaration.generics.iter().zip(&reference.arguments) {
            budget.text(&parameter.name, actual.span)?;
            budget.ty(actual)?;
            substitutions.insert(parameter.name.clone(), actual.clone());
        }
        let mut result = Vec::new();
        for bound in &declaration.bounds {
            budget.substituted_type(bound, &substitutions)?;
            let mut bound = bound.clone();
            Substitution {
                substitutions: &substitutions,
            }
            .ty(&mut bound);
            if copy::is_bound(&bound) {
                result.push(Bound::Copy);
            } else if stores::is_bound(&bound) {
                result.push(Bound::StoreElement);
            } else {
                let required = self.declaration(&bound, budget)?;
                let (item, _) = self.declarations[required.name.as_str()];
                result.push(Bound::Nominal(trait_instances::arguments(
                    item.span, required, &bound, budget,
                )?));
            }
        }
        Ok(result)
    }

    pub(super) fn super_bounds(
        &self,
        roots: &[Bound],
        budget: &mut Budget<'_>,
        span: Span,
        host: Option<&str>,
    ) -> R<Vec<Bound>> {
        self.super_bounds_host(roots, budget, span, host.map(Host::Name))
    }
    pub(super) fn super_bounds_type(
        &self,
        roots: &[Bound],
        budget: &mut Budget<'_>,
        subject: &TypeRef,
    ) -> R<Vec<Bound>> {
        self.super_bounds_host(roots, budget, subject.span, Some(Host::Type(subject)))
    }
    fn super_bounds_host(
        &self,
        roots: &[Bound],
        budget: &mut Budget<'_>,
        span: Span,
        host: Option<Host<'_>>,
    ) -> R<Vec<Bound>> {
        #[allow(clippy::too_many_arguments)]
        fn visit(
            index: &Index<'_>,
            bound: &Bound,
            output: &mut Vec<Bound>,
            active: &mut BTreeSet<trait_instances::Key>,
            budget: &mut Budget<'_>,
            span: Span,
            depth: usize,
            host: Option<Host<'_>>,
        ) -> R<()> {
            budget.node(span, depth)?;
            if let Bound::Nominal(reference) = bound {
                if active.contains(&reference.key) {
                    return Err(error(span, "active supertrait obligation cycle"));
                }
                for prior in output.iter() {
                    budget.node(span, depth)?;
                    if let Bound::Nominal(prior) = prior {
                        budget.text(&prior.key.name, span)?;
                        for argument in &prior.key.arguments {
                            budget.text(argument, span)?;
                        }
                        if prior.key == reference.key {
                            return Ok(());
                        }
                    }
                }
                let key = trait_instances::copy_key(&reference.key, budget, span)?;
                active.insert(key);
                for inherited in index.inherited(reference, host, budget)? {
                    visit(
                        index,
                        &inherited,
                        output,
                        active,
                        budget,
                        span,
                        depth + 1,
                        host,
                    )?;
                }
                active.remove(&reference.key);
                trait_instances::copy_key(&reference.key, budget, span)?;
                for actual in &reference.arguments {
                    budget.ty(actual)?;
                }
                output.push(bound.clone());
            } else {
                let mut duplicate = false;
                for b in output.iter() {
                    budget.node(span, depth)?;
                    duplicate |= matches!(
                        (b, bound),
                        (Bound::Copy, Bound::Copy) | (Bound::StoreElement, Bound::StoreElement)
                    );
                }
                if !duplicate {
                    output.push(bound.clone());
                }
            }
            Ok(())
        }
        let mut output = Vec::new();
        let mut active = BTreeSet::new();
        for bound in roots {
            visit(self, bound, &mut output, &mut active, budget, span, 0, host)?;
        }
        Ok(output)
    }

    pub(super) fn validate_supertraits(&mut self, budget: &mut Budget<'_>) -> R<()> {
        // Even unused declarations must have a well-founded published closure.
        for (item, declaration) in self.declarations.values() {
            budget.context.origin = self.origin(item, budget)?;
            if declaration.bounds.is_empty() {
                continue;
            }
            budget.node(item.span, 0)?;
            budget.text(&declaration.name, item.span)?;
            for parameter in &declaration.generics {
                budget.node(parameter.span, 0)?;
                budget.text(&parameter.name, parameter.span)?;
            }
            let arguments = declaration
                .generics
                .iter()
                .map(|p| GenericArg::Type(named(&p.name, p.span)))
                .collect();
            let application = TypeRef {
                span: item.span,
                kind: TypeKind::Named {
                    path: vec![declaration.name.clone()],
                    args: arguments,
                },
            };
            budget.ty(&application)?;
            let root = trait_instances::arguments(item.span, declaration, &application, budget)?;
            // A declared inherited application must be valid under its own
            // binders, including when no implementation is ever requested.
            let mut available = Bounds::new();
            let mut variables = BTreeSet::from(["Self".to_owned()]);
            for parameter in &declaration.generics {
                budget.node(parameter.span, 0)?;
                budget.text(&parameter.name, parameter.span)?;
                variables.insert(parameter.name.clone());
                let mut structural = Vec::new();
                if let GenericParamKind::Type(bounds) = &parameter.kind {
                    for bound in bounds {
                        budget.ty(bound)?;
                        if copy::is_bound(bound) {
                            structural.push(Bound::Copy);
                        }
                        if stores::is_bound(bound) {
                            structural.push(Bound::StoreElement);
                        }
                    }
                }
                available.insert(parameter.name.clone(), structural);
            }
            let mut self_structural = Vec::new();
            for bound in &declaration.bounds {
                if copy::is_bound(bound) {
                    self_structural.push(Bound::Copy);
                }
                if stores::is_bound(bound) {
                    self_structural.push(Bound::StoreElement);
                }
            }
            if !self_structural.is_empty() {
                budget.text("Self", item.span)?;
                available.insert("Self".into(), self_structural);
            }
            for bound in &declaration.bounds {
                if copy::is_bound(bound) || stores::is_bound(bound) {
                    continue;
                }
                let inherited = self.pattern_reference(bound, budget)?;
                self.reference_arguments(&inherited, &available, &variables, budget)?;
            }
            self.super_bounds(&[Bound::Nominal(root)], budget, item.span, Some("Self"))?;
        }
        if !self.recipes.is_empty() {
            return Ok(());
        }
        let candidates = std::mem::take(&mut self.implementations);
        let mut active = BTreeSet::new();
        for (key, host) in &candidates {
            self.commit_super(key, host, &candidates, &mut active, budget, 0)?;
        }
        Ok(())
    }

    fn commit_super(
        &mut self,
        key: &trait_instances::Key,
        host: &'a str,
        candidates: &BTreeSet<(trait_instances::Key, &'a str)>,
        active: &mut BTreeSet<(trait_instances::Key, &'a str)>,
        budget: &mut Budget<'_>,
        depth: usize,
    ) -> R<()> {
        let span = key.declaration;
        budget.node(span, depth)?;
        let goal = (trait_instances::copy_key(key, budget, span)?, host);
        if self.implementations.contains(&goal) {
            return Ok(());
        }
        if !candidates.contains(&goal) {
            return Err(error(
                span,
                format!(
                    "type {host} does not satisfy required supertrait {}",
                    key.name
                ),
            ));
        }
        if !active.insert(goal) {
            return Err(error(span, "active supertrait implementation cycle"));
        }
        let reference = self.actual_reference(key, budget, span)?;
        for bound in self.inherited(&reference, Some(Host::Name(host)), budget)? {
            match bound {
                Bound::Copy => copy::require(
                    &named(host, span),
                    &self.nominals,
                    &Bounds::new(),
                    &BTreeSet::new(),
                    budget,
                )?,
                Bound::StoreElement => self.store_element(
                    &named(host, span),
                    &Bounds::new(),
                    &BTreeSet::new(),
                    budget,
                    callable::StoreCallbacks::None,
                )?,
                Bound::Nominal(reference) => {
                    self.commit_super(&reference.key, host, candidates, active, budget, depth + 1)?
                }
            }
        }
        let goal = (trait_instances::copy_key(key, budget, span)?, host);
        active.remove(&goal);
        self.implementations.insert(goal);
        Ok(())
    }
}
