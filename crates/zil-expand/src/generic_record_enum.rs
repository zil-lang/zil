//! Source labels are resolved against one declaration before positional IR.
use super::*;

pub(crate) fn is_copy_bound(ty: &TypeRef) -> bool {
    copy::is_bound(ty)
}

pub(super) fn variables(declaration: &EnumDecl, budget: &mut Budget<'_>) -> R<BTreeSet<String>> {
    let mut variables = BTreeSet::new();
    for parameter in &declaration.generics {
        budget.node(parameter.span, 0)?;
        budget.text(&parameter.name, parameter.span)?;
        variables.insert(parameter.name.clone());
    }
    Ok(variables)
}

pub(super) fn unresolved(
    variables: &BTreeSet<String>,
    found: &BTreeMap<String, TypeRef>,
    hint: &TypeRef,
    budget: &mut Budget<'_>,
) -> R<bool> {
    for variable in variables {
        budget.node(hint.span, 0)?;
        budget.text(variable, hint.span)?;
        if !found.contains_key(variable) {
            budget.ty(hint)?;
            if enums::contains(hint, variable) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub(crate) fn variant<'a>(
    declaration: &'a EnumDecl,
    name: &str,
    span: Span,
    budget: &mut Budget<'_>,
) -> R<&'a Variant> {
    for variant in &declaration.variants {
        budget.node(variant.span, 0)?;
        budget.text(&variant.name, variant.span)?;
        budget.text(name, span)?;
        if variant.name == name {
            return Ok(variant);
        }
    }
    Err(error(span, "unknown record enum variant"))
}

pub(crate) fn fields<'a>(
    declaration: &'a EnumDecl,
    name: &str,
    span: Span,
    budget: &mut Budget<'_>,
) -> R<&'a [Field]> {
    let variant = variant(declaration, name, span, budget)?;
    match &variant.fields {
        VariantFields::Record(fields) => Ok(fields),
        _ => Err(error(
            span,
            "named fields require a record enum variant, not a tuple or unit variant",
        )),
    }
}

/// Each written label maps to one declaration position; visits and compared
/// text are charged before the search, including unsuccessful searches.
pub(crate) fn order<'a>(
    declared: &[Field],
    supplied: impl IntoIterator<Item = (&'a str, Span)>,
    span: Span,
    budget: &mut Budget<'_>,
) -> R<Vec<usize>> {
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for (name, at) in supplied {
        let mut position = None;
        for (index, field) in declared.iter().enumerate() {
            budget.node(at, 0)?;
            budget.text(name, at)?;
            budget.text(&field.name, field.span)?;
            if field.name == name {
                position = Some(index);
                break;
            }
        }
        let position = position.ok_or_else(|| error(at, "unknown record enum field label"))?;
        budget.node(at, 0)?;
        if !seen.insert(position) {
            return Err(error(at, "duplicate record enum field label"));
        }
        result.push(position);
    }
    if result.len() != declared.len() {
        return Err(error(
            span,
            "record enum requires every declared field exactly once",
        ));
    }
    Ok(result)
}

pub(super) fn validate(
    declaration: &EnumDecl,
    nominals: &BTreeMap<String, &Item>,
    traits: &trait_bounds::Index<'_>,
    budget: &mut Budget<'_>,
) -> R<()> {
    let mut variables = BTreeSet::new();
    let mut conditional = trait_bounds::Bounds::new();
    for parameter in &declaration.generics {
        budget.node(parameter.span, 0)?;
        budget.text(&parameter.name, parameter.span)?;
        budget.text(&parameter.name, parameter.span)?;
        variables.insert(parameter.name.clone());
        conditional.insert(parameter.name.clone(), vec![trait_bounds::Bound::Copy]);
    }
    let real_bounds = traits.parameter_bounds(&declaration.generics, budget)?;
    // These layout-only assumptions are never published as function bounds or
    // used to discharge nested declared premises below.
    let mut variants = Vec::<&str>::new();
    for variant in &declaration.variants {
        budget.node(variant.span, 0)?;
        if !variant.attributes.is_empty() {
            return Err(unsupported(
                variant.span,
                "unexecuted generic enum variant attribute",
            ));
        }
        for previous in &variants {
            budget.text(previous, variant.span)?;
            budget.text(&variant.name, variant.span)?;
            if *previous == variant.name {
                return Err(error(variant.span, "duplicate record enum variant"));
            }
        }
        variants.push(&variant.name);
        let mut payloads = Vec::new();
        match &variant.fields {
            VariantFields::Unit => {}
            VariantFields::Tuple(fields) => {
                for field in fields {
                    budget.node(field.span, 0)?;
                    payloads.push((&field.ty, field.manages, field.span));
                }
            }
            VariantFields::Record(fields) => {
                let mut labels = Vec::<&str>::new();
                for field in fields {
                    budget.node(field.span, 0)?;
                    if field.public || field.default.is_some() || field.manages {
                        return Err(unsupported(
                            field.span,
                            "record enum field pub/default/manages is not implemented",
                        ));
                    }
                    if !field.attributes.is_empty() {
                        return Err(unsupported(
                            field.span,
                            "record enum field attributes are not implemented",
                        ));
                    }
                    for previous in &labels {
                        budget.text(previous, field.span)?;
                        budget.text(&field.name, field.span)?;
                        if *previous == field.name {
                            return Err(error(
                                field.span,
                                "duplicate record enum declaration field",
                            ));
                        }
                    }
                    labels.push(&field.name);
                    budget.node(field.span, 0)?;
                    payloads.push((&field.ty, false, field.span));
                }
            }
        }
        for (ty, manages, span) in payloads {
            if manages {
                return Err(unsupported(
                    span,
                    "record enum profile requires complete ordinary Copy payloads",
                ));
            }
            traits.store_types(
                ty,
                &real_bounds,
                &variables,
                budget,
                callable::StoreCallbacks::None,
            )?;
            copy::require(ty, nominals, &conditional, &variables, budget)?;
        }
    }
    Ok(())
}

impl Engine<'_> {
    pub(super) fn record_enum_expression(
        &mut self,
        expression: &mut Expr,
        env: &mut Env,
        expected: Option<&TypeRef>,
        depth: usize,
    ) -> R<Option<TypeRef>> {
        let span = expression.span;
        let ExprKind::Record {
            ty: Some(callee),
            base,
            fields: supplied,
        } = &mut expression.kind
        else {
            return Ok(None);
        };
        let Some((name, variant, explicit)) = enums::constructor(callee) else {
            return Ok(None);
        };
        let Some(declaration) = self.enums.get(name).copied() else {
            return Ok(None);
        };
        if env.contains_key(name) {
            return Err(error(span, "record enum namespace is shadowed by a value"));
        }
        self.budget.text(variant, span)?;
        let variant = variant.to_owned();
        let declared = fields(declaration, &variant, span, &mut self.budget)?;
        if base.is_some() {
            return Err(unsupported(span, "record enum spread is not implemented"));
        }
        let order = order(
            declared,
            supplied.iter().map(|f| (f.name.as_str(), f.span)),
            span,
            &mut self.budget,
        )?;
        let mut found = BTreeMap::new();
        let variables = variables(declaration, &mut self.budget)?;
        if let Some(explicit) = explicit {
            if explicit.len() != declaration.generics.len() {
                return Err(error(span, "record enum type argument arity mismatch"));
            }
            for (parameter, argument) in declaration.generics.iter().zip(explicit) {
                let GenericArg::Type(ty) = argument else {
                    return Err(unsupported(span, "const enum type argument"));
                };
                self.budget.text(&parameter.name, span)?;
                self.budget.ty(ty)?;
                found.insert(parameter.name.clone(), self.original_type(ty)?);
            }
        }
        let expected = expected.map(|ty| self.original_type(ty)).transpose()?;
        enums::context(declaration, expected.as_ref(), &mut found, &mut self.budget)?;
        for (supplied, index) in supplied.iter_mut().zip(order) {
            if supplied.value.is_none() {
                self.budget.node(supplied.span, depth + 1)?;
                self.budget.text(&supplied.name, supplied.span)?;
                supplied.value = Some(Expr {
                    kind: ExprKind::Name(supplied.name.clone()),
                    span: supplied.span,
                });
            }
            let value = supplied
                .value
                .as_mut()
                .expect("explicit or shorthand value");
            let field = &declared[index];
            let hint = enums::substituted(&field.ty, &found, &mut self.budget)?;
            let unresolved = unresolved(&variables, &found, &hint, &mut self.budget)?;
            let actual =
                self.expr_expected(value, env, (!unresolved).then_some(&hint), depth + 1)?;
            let actual = match actual {
                Some(actual) => actual,
                None => match self.value_hint(value, env, depth + 1, true)? {
                    Some(actual) => actual,
                    None if !unresolved => {
                        self.budget.ty(&hint)?;
                        hint.clone()
                    }
                    None => {
                        return Err(error(
                            supplied.span,
                            "record enum payload type cannot be inferred",
                        ));
                    }
                },
            };
            let actual = self.original_type(&actual)?;
            unify(
                &field.ty,
                &actual,
                &variables,
                &mut found,
                supplied.span,
                &mut self.budget,
            )?;
        }
        let mut result = enums::application(declaration, &found, span, &mut self.budget)?;
        enums::check_injection(&result, expected.as_ref(), &mut self.budget)?;
        self.ty(&mut result, depth + 1)?;
        let TypeKind::Named { path, .. } = &result.kind else {
            unreachable!()
        };
        self.budget.text(&path[0], span)?;
        self.budget.node(span, 0)?;
        callee.kind = ExprKind::Member {
            object: Box::new(Expr {
                kind: ExprKind::Name(path[0].clone()),
                span,
            }),
            member: MemberName::Name(variant),
        };
        Ok(Some(result))
    }

    pub(super) fn record_enum_pattern(
        &mut self,
        path: &mut [String],
        supplied: &mut [RecordPatternField],
        ty: Option<&TypeRef>,
        env: &mut Env,
        span: Span,
        depth: usize,
    ) -> R<()> {
        let ty = ty.ok_or_else(|| error(span, "record enum pattern type cannot be inferred"))?;
        let written = self.original_type(ty)?;
        let TypeKind::Named { path: root, args } = &grouped(&written).kind else {
            return Err(error(span, "record enum pattern requires an enum"));
        };
        let declaration = self
            .enums
            .get(&root.join("."))
            .copied()
            .ok_or_else(|| error(span, "record enum pattern requires a declared enum"))?;
        if let Some(alias) = path.first().and_then(|name| self.pattern_types.get(name)) {
            if type_key(&written, &mut self.budget)? != type_key(alias, &mut self.budget)? {
                return Err(error(
                    span,
                    "alias pattern has a different concrete enum type",
                ));
            }
            self.budget.text(&declaration.name, span)?;
            path[0] = declaration.name.clone();
        }
        if path.len() != 2 || path[0] != declaration.name {
            return Err(error(
                span,
                "record pattern has a different nominal enum type",
            ));
        }
        let declared = fields(declaration, &path[1], span, &mut self.budget)?;
        let order = order(
            declared,
            supplied.iter().map(|f| (f.name.as_str(), f.span)),
            span,
            &mut self.budget,
        )?;
        let mut found = BTreeMap::new();
        if declaration.generics.len() != args.len() {
            return Err(error(span, "record enum pattern type arity mismatch"));
        }
        for (parameter, argument) in declaration.generics.iter().zip(args) {
            let GenericArg::Type(ty) = argument else {
                return Err(unsupported(span, "const enum argument"));
            };
            self.budget.text(&parameter.name, span)?;
            self.budget.ty(ty)?;
            found.insert(parameter.name.clone(), ty.clone());
        }
        let TypeKind::Named { path: instance, .. } = &grouped(ty).kind else {
            unreachable!()
        };
        self.budget.text(&instance[0], span)?;
        path[0] = instance[0].clone();
        for (supplied, index) in supplied.iter_mut().zip(order) {
            let mut field_type = enums::substituted(&declared[index].ty, &found, &mut self.budget)?;
            self.ty(&mut field_type, depth + 1)?;
            self.enum_pattern(&mut supplied.pattern, Some(&field_type), env, depth + 1)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod record_enum329_ledger {
    use super::*;
    use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};

    fn exhausted<T>(result: CompileResult<T>) {
        let errors = match result {
            Ok(_) => panic!("record enum received a fresh downstream allowance"),
            Err(errors) => errors,
        };
        assert!(!errors.is_empty());
        assert!(
            errors
                .iter()
                .all(|e| e.category == ResultCategory::ResourceLimit),
            "{errors:?}"
        );
    }

    #[test]
    fn source_alias_and_downstream_expansion_share_one_ledger() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "record329.zil",
            "type Int=i32;enum E{V{value:Int,flag:bool}}fn main()->i32{let e=E.V{flag:true,value:7};match e{E.V{value:n,flag:_}=>{return n-7}}}",
        );
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let limits = Limits::default();
        let mut products = products::Products::default();
        specialize_products(&syntax, &limits, &mut products).unwrap();
        assert!(products.record_variants && products.metered());
        let remaining = products.allowance.as_ref().unwrap();
        let work = limits.max_analysis_steps - remaining.max_analysis_steps;
        let nodes = limits.max_nodes - remaining.max_nodes;
        let (alias, alias_budget) =
            aliases::normalize_with_budget(&syntax, Budget::new(&limits)).unwrap();
        let alias = alias.unwrap();
        let alias_remaining = alias_budget.remaining_limits();
        let mut engine = products::Products::default();
        specialize_with_budget(
            &alias.module,
            &limits,
            &alias.pattern_types,
            Budget::new(&limits),
            &mut engine,
            false, // This retained source fixture has no associated declarations.
            false, // No generic methods in this retained source fixture.
            None, // This retained-source control has no runtime CT callback.
        )
        .unwrap();
        let engine_remaining = engine.allowance.unwrap();
        assert!(
            work >= (limits.max_analysis_steps - alias_remaining.max_analysis_steps)
                + (limits.max_analysis_steps - engine_remaining.max_analysis_steps)
        );
        assert!(
            nodes
                >= (limits.max_nodes - alias_remaining.max_nodes)
                    + (limits.max_nodes - engine_remaining.max_nodes)
        );
        for selected in [
            Limits {
                max_analysis_steps: work,
                ..limits.clone()
            },
            Limits {
                max_nodes: nodes,
                ..limits.clone()
            },
        ] {
            let mut exact = products::Products::default();
            specialize_products(&syntax, &selected, &mut exact).unwrap();
            exhausted(crate::expand_with_map_limits(&source, &syntax, &selected));
        }
        crate::expand_with_map_limits(&source, &syntax, &limits).unwrap();
        println!(
            "329 source specialization work={work}, nodes={nodes}; exhausted actual balances reject downstream expansion"
        );
    }

    #[test]
    fn module_normalization_and_named_lowering_share_the_remainder() {
        let sources = [
            SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "main.zil",
                "import data;fn main()->i32{let e=data.E.V{value:7,flag:true};match e{data.E.V{flag:_,value:n}=>{return n-7}}}",
            ),
            SourceUnit::new(
                FileId(1),
                ModuleId(1),
                "data.zil",
                "pub enum E{V{flag:bool,value:i32}}",
            ),
        ];
        let syntax = sources
            .iter()
            .map(|s| zil_syntax::parse(s).into_result().unwrap())
            .collect::<Vec<_>>();
        let modules = syntax.iter().collect::<Vec<_>>();
        let imports = [
            vec![zil_ir::CoreImport {
                alias: "data".into(),
                module: ModuleId(1),
                span: syntax[0].items[0].span,
            }],
            Vec::new(),
        ];
        let limits = Limits::default();
        let mut products = products::Products::default();
        crate::modules::specialize(&sources, &modules, &imports, &limits, &mut products).unwrap();
        assert!(products.record_variants);
        let remaining = products.allowance.as_ref().unwrap();
        let work = limits.max_analysis_steps - remaining.max_analysis_steps;
        let nodes = limits.max_nodes - remaining.max_nodes;
        for selected in [
            Limits {
                max_analysis_steps: work,
                ..limits.clone()
            },
            Limits {
                max_nodes: nodes,
                ..limits.clone()
            },
        ] {
            let mut exact = products::Products::default();
            crate::modules::specialize(&sources, &modules, &imports, &selected, &mut exact)
                .unwrap();
            exhausted(crate::expand_program_with_limits(
                &sources, &modules, &imports, &selected,
            ));
        }
        crate::expand_program_with_limits(&sources, &modules, &imports, &limits).unwrap();
        println!(
            "329 project specialization work={work}, nodes={nodes}; exhausted actual balances reject downstream expansion"
        );
    }
}
