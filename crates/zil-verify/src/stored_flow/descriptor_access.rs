//! Actual descriptor-manager accesses are distinct from accesses to a pointer
//! cell containing that descriptor. The selector identifies the real Desc
//! input occurrence; the suffix remains relative to its manager allocation.
use super::*;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ManagerSuffix {
    pub(super) inline: Path,
    pub(super) heap_parents: Vec<Path>,
    pub(super) heap_element: Option<Path>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DescriptionAccess {
    pub(super) source: Source,
    pub(super) suffix: ManagerSuffix,
    pub(super) write: bool,
}
fn suffix_work(suffix: &ManagerSuffix, b: &mut Budget) -> Check<()> {
    b.spend(
        suffix
            .inline
            .len()
            .saturating_add(suffix.heap_parents.len())
            .saturating_add(suffix.heap_element.as_ref().map_or(0, Vec::len))
            .saturating_add(1),
    )?;
    for path in &suffix.heap_parents {
        b.spend(path.len())?;
    }
    Ok(())
}
pub(super) fn work(rows: &[DescriptionAccess], b: &mut Budget) -> Check<()> {
    b.spend(rows.len())?;
    for row in rows {
        source_work(std::slice::from_ref(&row.source), b)?;
        suffix_work(&row.suffix, b)?;
    }
    Ok(())
}
fn input_type<'a>(
    module: &'a TypedModule,
    f: &'a TypedFunction,
    source: &Source,
) -> Check<Option<&'a Type>> {
    Ok(match source.base {
        HoldRoot::Parameter(index) => Some(
            &f.locals
                .get(
                    f.parameters
                        .get(index as usize)
                        .ok_or_else(|| err("manager access input ordinal is absent"))?
                        .0 as usize,
                )
                .ok_or_else(|| err("manager access input declaration is absent"))?
                .ty,
        ),
        HoldRoot::Capture(index) => Some(
            module
                .closures
                .get(
                    f.environment
                        .ok_or_else(|| err("manager access capture has no actual environment"))?
                        .0 as usize,
                )
                .and_then(|d| d.captures.get(index as usize))
                .ok_or_else(|| err("manager access capture declaration is absent"))?,
        ),
        _ => None,
    })
}
/// None preserves the independently checked owner/head access route. Only an
/// actual Desc endpoint changes the source role; Managed owners stay owners.
fn classify(
    module: &TypedModule,
    f: &TypedFunction,
    source: &Source,
    write: bool,
    b: &mut Budget,
) -> Check<Option<DescriptionAccess>> {
    if source.retained_port.is_some() {
        return Ok(None);
    }
    let Some(mut ty) = input_type(module, f, source)? else {
        return Ok(None);
    };
    let total = source
        .leaf
        .len()
        .checked_add(source.inline.len())
        .ok_or_else(|| limit("descriptor access selector length overflow"))?;
    let mut selected = 0usize;
    let mut depth = 0usize;
    loop {
        b.spend(1)?;
        depth += 1;
        if depth > b.limits.max_call_depth.min(128) {
            return Err(limit("descriptor access input path depth exceeded"));
        }
        if matches!(ty, Type::Desc(_)) {
            // leaf denotes a saved-pointer namespace. It must be wholly
            // consumed before entering the descriptor's manager allocation.
            if selected < source.leaf.len() {
                return Err(err(
                    "descriptor access crosses an unresolved saved-pointer namespace",
                ));
            }
            let inline = selected - source.leaf.len();
            source_work(std::slice::from_ref(source), b)?;
            let mut selector = source.clone();
            selector.inline.truncate(inline);
            selector.heap_parents.clear();
            selector.heap_element = None;
            b.spend(
                source
                    .inline
                    .len()
                    .saturating_sub(inline)
                    .saturating_add(source.heap_parents.len())
                    .saturating_add(source.heap_element.as_ref().map_or(0, Vec::len))
                    .saturating_add(1),
            )?;
            for path in &source.heap_parents {
                b.spend(path.len())?;
            }
            let suffix = ManagerSuffix {
                inline: source.inline[inline..].to_vec(),
                heap_parents: source.heap_parents.clone(),
                heap_element: source.heap_element.clone(),
            };
            return Ok(Some(DescriptionAccess {
                source: selector,
                suffix,
                write,
            }));
        }
        match ty {
            Type::Address { inner, .. } => {
                ty = inner;
                continue;
            }
            Type::BorrowedStorage { storage, .. } => {
                ty = storage;
                continue;
            }
            _ => {}
        }
        if selected == total {
            return Ok(None);
        }
        let step = if selected < source.leaf.len() {
            source.leaf[selected]
        } else {
            source.inline[selected - source.leaf.len()]
        };
        let child = match (ty, step) {
            (Type::Tuple(fields), PlaceProjection::Field(index)) => fields.get(index as usize),
            (
                Type::Record { fields, .. } | Type::Package { fields, .. },
                PlaceProjection::Field(index),
            ) => fields.get(index as usize).map(|(_, ty)| ty),
            (Type::Array { element, length }, PlaceProjection::Field(index)) if index < *length => {
                Some(element.as_ref())
            }
            (Type::Enum { variants, .. }, PlaceProjection::Variant(index)) => variants
                .get(index.variant as usize)
                .and_then(|(_, fields)| fields.get(index.index as usize)),
            (Type::ErrorUnion { error, result }, PlaceProjection::ErrorArm(failed)) => {
                Some(if failed {
                    error.as_ref()
                } else {
                    result.as_ref()
                })
            }
            (Type::Closure(id), PlaceProjection::Field(index)) => module
                .closures
                .get(id.0 as usize)
                .filter(|d| d.id == *id)
                .and_then(|d| d.captures.get(index as usize)),
            // Store/Managed/head allocation boundaries keep the original
            // head route, including its exact owning-environment exception.
            _ => None,
        };
        let Some(child) = child else {
            return Ok(None);
        };
        ty = child;
        selected += 1;
    }
}
pub(super) fn record(
    module: &TypedModule,
    f: &TypedFunction,
    heads: &mut Vec<(Source, bool)>,
    descriptions: &mut Vec<DescriptionAccess>,
    permission: &Permission,
    write: bool,
    b: &mut Budget,
) -> Check<()> {
    for source in &permission.sources {
        if !matches!(source.base, HoldRoot::Parameter(_) | HoldRoot::Capture(_)) {
            continue;
        }
        if let Some(row) = classify(module, f, source, write, b)? {
            source_work(std::slice::from_ref(&row.source), b)?;
            suffix_work(&row.suffix, b)?;
            work(descriptions, b)?;
            let suffix_width = row
                .suffix
                .inline
                .len()
                .saturating_add(row.suffix.heap_parents.len())
                .saturating_add(
                    row.suffix
                        .heap_parents
                        .iter()
                        .fold(0usize, |n, path| n.saturating_add(path.len())),
                )
                .saturating_add(row.suffix.heap_element.as_ref().map_or(0, Vec::len))
                .saturating_add(1);
            b.spend(
                descriptions
                    .len()
                    .saturating_mul(width(&row.source).saturating_add(suffix_width)),
            )?;
            if !descriptions.contains(&row) {
                b.spend(1)?;
                descriptions.push(row);
            }
        } else {
            source_work(std::slice::from_ref(source), b)?;
            b.spend(1)?;
            store_paths::record(
                heads,
                &Permission {
                    sources: vec![source.clone()],
                    loans: vec![],
                },
                write,
                b,
            )?;
        }
    }
    Ok(())
}
pub(super) fn resolve(
    before: &call_results::Before<'_>,
    row: &DescriptionAccess,
    b: &mut Budget,
) -> Check<Permission> {
    source_work(std::slice::from_ref(&row.source), b)?;
    suffix_work(&row.suffix, b)?;
    let mut actual = before.descriptor_source(&row.source, b)?;
    let relative = Source {
        base: row.source.base,
        leaf: vec![],
        inline: row.suffix.inline.clone(),
        retained_port: None,
        heap_parents: row.suffix.heap_parents.clone(),
        heap_element: row.suffix.heap_element.clone(),
        static_dependency: row.source.static_dependency.clone(),
    };
    for target in &mut actual.sources {
        store_paths::compose(target, &relative, b)?;
    }
    Ok(actual)
}

pub(super) fn symbolic(row: &DescriptionAccess, b: &mut Budget) -> Check<Source> {
    source_work(std::slice::from_ref(&row.source), b)?;
    suffix_work(&row.suffix, b)?;
    let mut source = row.source.clone();
    let relative = Source {
        base: source.base,
        leaf: vec![],
        inline: row.suffix.inline.clone(),
        retained_port: None,
        heap_parents: row.suffix.heap_parents.clone(),
        heap_element: row.suffix.heap_element.clone(),
        static_dependency: source.static_dependency.clone(),
    };
    store_paths::compose(&mut source, &relative, b)?;
    Ok(source)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(base: HoldRoot) -> Source {
        Source {
            base,
            leaf: vec![],
            inline: vec![],
            retained_port: None,
            heap_parents: vec![],
            heap_element: None,
            static_dependency: StaticProvenance::NotStatic,
        }
    }
    // These controls exercise actual snapshot and access-row components.
    // Original operation, modes, layouts and cleanup stay separate WF proofs.
    #[test]
    fn descriptor_access_keeps_beta_manager_and_relative_allocation_suffix_across_a_helper() {
        let (mut module, _) = crate::tests::baseline();
        let mut f = module.functions[0].clone();
        let descriptor = Type::Desc(Box::new(Type::OwnedStore {
            element: Box::new(Type::I32),
        }));
        let pointer = Type::Address {
            mutable: true,
            inner: Box::new(descriptor),
        };
        f.parameters = vec![LocalId(0), LocalId(1)];
        f.locals = vec![
            LocalDecl {
                id: LocalId(0),
                name: "slot".into(),
                ty: pointer.clone(),
                mutable: false,
                span: f.span,
            },
            LocalDecl {
                id: LocalId(1),
                name: "owner".into(),
                ty: Type::Managed(Box::new(Type::ByteBuffer)),
                mutable: false,
                span: f.span,
            },
        ];
        f.values = vec![pointer];
        module.functions[0] = f;
        let f = &module.functions[0];
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        let mut symbolic = source(HoldRoot::Parameter(0));
        symbolic.inline = vec![PlaceProjection::Field(5)];
        symbolic.heap_parents = vec![vec![PlaceProjection::Field(4)]];
        symbolic.heap_element = Some(vec![PlaceProjection::Field(6)]);
        let mut heads = vec![];
        let mut descriptions = vec![];
        record(
            &module,
            f,
            &mut heads,
            &mut descriptions,
            &Permission {
                sources: vec![symbolic.clone()],
                loans: vec![],
            },
            true,
            &mut b,
        )
        .unwrap();
        assert!(heads.is_empty());
        assert_eq!(descriptions.len(), 1);
        assert_eq!(descriptions[0].source, source(HoldRoot::Parameter(0)));
        assert_eq!(descriptions[0].suffix.inline, symbolic.inline);
        assert_eq!(descriptions[0].suffix.heap_parents, symbolic.heap_parents);
        assert_eq!(descriptions[0].suffix.heap_element, symbolic.heap_element);
        let values = [Payload {
            address: Permission {
                sources: vec![source(HoldRoot::Parameter(0))],
                loans: vec![],
            },
            ..Default::default()
        }];
        let state = Frame {
            external: vec![Payload {
                store_description: Permission {
                    sources: vec![source(HoldRoot::Parameter(0))],
                    loans: vec![],
                },
                ..Default::default()
            }],
            pointees: vec![(
                HoldRoot::Parameter(0),
                Payload {
                    store_description: Permission {
                        sources: vec![source(HoldRoot::Parameter(1))],
                        loans: vec![],
                    },
                    ..Default::default()
                },
            )],
            ..Default::default()
        };
        // direct() also authenticates both parameter types. The second input
        // is a genuine owner and is not converted to a descriptor occurrence.
        let mut actuals = values.to_vec();
        actuals.push(Payload::default());
        let arguments = [ValueId(0), ValueId(1)];
        let mut caller = f.clone();
        caller.values.push(caller.locals[1].ty.clone());
        let before =
            call_results::Before::direct(&module, &caller, f, &arguments, &actuals, &state, &mut b)
                .unwrap();
        let actual = resolve(&before, &descriptions[0], &mut b).unwrap();
        assert_eq!(actual.sources.len(), 1);
        assert_eq!(actual.sources[0].base, HoldRoot::Parameter(1));
        assert_eq!(actual.sources[0].inline, symbolic.inline);
        assert_eq!(actual.sources[0].heap_parents, symbolic.heap_parents);
        assert_eq!(actual.sources[0].heap_element, symbolic.heap_element);
        let mut caller_heads = vec![];
        let mut caller_descriptions = vec![];
        record(
            &module,
            &caller,
            &mut caller_heads,
            &mut caller_descriptions,
            &actual,
            true,
            &mut b,
        )
        .unwrap();
        assert!(caller_descriptions.is_empty());
        assert_eq!(caller_heads, vec![(actual.sources[0].clone(), true)]);
        let cloned = f.clone();
        assert!(
            call_results::Before::direct(
                &module, &caller, &cloned, &arguments, &actuals, &state, &mut b
            )
            .is_err(),
            "an equal clone is not the independently selected actual target"
        );
    }
    #[test]
    fn descriptor_selector_must_consume_its_actual_input_field_before_manager_suffix() {
        let (mut module, _) = crate::tests::baseline();
        let mut f = module.functions[0].clone();
        let ty = Type::Tuple(vec![Type::I32, Type::Desc(Box::new(Type::I32))]);
        f.parameters = vec![LocalId(0)];
        f.locals = vec![LocalDecl {
            id: LocalId(0),
            name: "input".into(),
            ty,
            mutable: false,
            span: f.span,
        }];
        module.functions[0] = f;
        let limits = Limits::default();
        let mut b = Budget::new(&limits);
        let mut actual = source(HoldRoot::Parameter(0));
        actual.leaf = vec![PlaceProjection::Field(1)];
        actual.inline = vec![PlaceProjection::Field(3)];
        let row = classify(&module, &module.functions[0], &actual, false, &mut b)
            .unwrap()
            .unwrap();
        assert_eq!(row.source.leaf, vec![PlaceProjection::Field(1)]);
        assert!(row.source.inline.is_empty());
        assert_eq!(row.suffix.inline, vec![PlaceProjection::Field(3)]);
        actual.leaf = vec![PlaceProjection::Field(1), PlaceProjection::Field(0)];
        assert!(
            classify(&module, &module.functions[0], &actual, false, &mut b).is_err(),
            "a saved-pointer namespace cannot silently become a manager field"
        );
    }
}
