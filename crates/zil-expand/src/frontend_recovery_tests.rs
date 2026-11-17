use super::*;
use zil_syntax::attributes::AttributeContext;

type Output = Option<Vec<(ProgramModule, Arc<SourceMap>)>>;
fn parsed(id: u32, text: &str) -> ParsedSource {
    ParsedSource::parse(
        Arc::new(SourceUnit::new(
            FileId(id),
            ModuleId(id),
            format!("m{id}.zil"),
            text,
        )),
        &Limits::default(),
        &AttributeContext::default(),
    )
}
fn inputs(body: &str) -> Vec<ParsedSource> {
    vec![
        parsed(0, body),
        parsed(
            1,
            "pub fn dep(x:i32)->i32{let y=x+3;if y>0{return y;}else{return x;}}",
        ),
    ]
}
fn run(
    session: &mut ExpandSession,
    inputs: &[ParsedSource],
    limits: &Limits,
    fused: bool,
) -> CompileResult<Output> {
    let aliases = BTreeMap::new();
    let context = source_context::SourceIdentityContext::ClosedSnapshot {
        root: ModuleId(0),
        aliases: &aliases,
        declarations: source_context::DeclarationPlacement::Original,
    };
    let limits = if fused {
        for input in inputs {
            session.parsed_scalar(input, limits);
        }
        let (origins, remaining) = session.capture_original_parsed(inputs, context, limits)?;
        if origins.is_some() {
            return Ok(None);
        }
        remaining
    } else {
        limits.clone()
    };
    session
        .expand_parsed(
            inputs,
            &vec![vec![]; inputs.len()],
            &limits,
            &TargetProfile::default(),
            context,
        )
        .map(|p| p.map(|p| p.modules))
}
fn seed(fused: bool) -> FrontendProducts {
    let mut session = ExpandSession::default();
    assert!(
        run(
            &mut session,
            &inputs("fn main()->i32=>1;"),
            &Limits::default(),
            fused
        )
        .unwrap()
        .is_some()
    );
    let products = session
        .encode_frontend_products(&Limits::default())
        .expect("raw frontend products");
    assert_eq!(products.modules.len(), 2);
    products
}

#[test]
fn restart_before_first_edit_avoids_unchanged_normalize_restore_and_core() {
    for fused in [false, true] {
        let products = seed(fused);
        let current = inputs("fn main()->i32=>2;");
        let mut restored = ExpandSession::default();
        assert!(restored.restore_frontend_products(products, &Limits::default()));
        let actual = run(&mut restored, &current, &Limits::default(), fused).unwrap();
        let reference = run(
            &mut ExpandSession::default(),
            &current,
            &Limits::default(),
            fused,
        )
        .unwrap();
        assert_eq!(actual, reference);
        assert_eq!(restored.last_stats().recovered_frontend_modules, 1);
        assert_eq!(restored.last_stats().normalize_modules, 1);
        assert_eq!(restored.last_stats().restore_modules, 1);
        assert_eq!(restored.last_stats().lower_modules, 1);
        assert_eq!(restored.last_stats().normalized_materializations, 0);
        assert_eq!(restored.last_stats().reused_lower_modules, 1);
        // A newly published recovered epoch is ordinary raw input for another
        // process, and still needs current admission and independent consumers.
        let again = restored
            .encode_frontend_products(&Limits::default())
            .unwrap();
        let mut next = ExpandSession::default();
        assert!(next.restore_frontend_products(again, &Limits::default()));
        assert_eq!(
            run(&mut next, &current, &Limits::default(), fused).unwrap(),
            reference
        );
        assert_eq!(next.last_stats().recovered_frontend_modules, 2);
        assert_eq!(next.last_stats().lower_modules, 0);
    }
}

#[test]
fn recovered_continuation_keeps_305_raw_function_owners_after_parser_eviction() {
    const MODULE: &str =
        "fn private(x:i32)->i32=>x+1; fn other(x:i32)->i32=>x+2; pub fn f(x:i32)->i32=>x+3;";
    let fixture = |edited| {
        (0..102)
            .map(|id| {
                if edited && id == 0 {
                    parsed(id, &MODULE.replace("x+1", "x+7"))
                } else {
                    parsed(id, MODULE)
                }
            })
            .collect::<Vec<_>>()
    };
    for fused in [false, true] {
        let limits = Limits::default();
        let mut writer = ExpandSession::default();
        run(&mut writer, &fixture(false), &limits, fused)
            .unwrap()
            .unwrap();
        let products = writer.encode_frontend_products(&limits).unwrap();
        assert_eq!(products.modules.len(), 102);
        drop(writer);
        let mut session = ExpandSession::default();
        assert!(session.restore_frontend_products(products, &limits));
        let before = run(&mut session, &fixture(false), &limits, fused)
            .unwrap()
            .unwrap();
        assert_eq!(session.last_stats().recovered_frontend_modules, 102);
        assert_eq!(session.last_stats().lower_modules, 0);
        // Reparse every module to force the real parser-retention boundary.
        // Pointer inequality must not invalidate exact source/parser equality.
        let current = fixture(true);
        assert!(current.iter().all(|p| {
            !session.entries[&p.source().module_id]
                .parsed
                .same_revision(p)
        }));
        let after = run(&mut session, &current, &limits, fused)
            .unwrap()
            .unwrap();
        let reference = run(&mut ExpandSession::default(), &current, &limits, fused)
            .unwrap()
            .unwrap();
        assert_eq!(after, reference);
        assert_eq!(session.last_stats().lower_modules, 1);
        assert_eq!(session.last_stats().reused_lower_modules, 101);
        assert_eq!(session.last_stats().normalize_modules, 1);
        let mut retained = 0;
        for (index, ((before, _), (after, _))) in before.iter().zip(&after).enumerate() {
            assert_eq!(Arc::ptr_eq(&before.core, &after.core), index != 0);
            for function in 0..3 {
                let same = Arc::ptr_eq(
                    before.core.functions.owner(function).unwrap(),
                    after.core.functions.owner(function).unwrap(),
                );
                assert_eq!(same, index != 0 || function != 0);
                retained += usize::from(same);
            }
        }
        assert_eq!(retained, 305);
        // The successful continuation remains usable on a second edit, after
        // normal Lower has installed its real item receipts for that module.
        let mut next = fixture(true);
        next[0] = parsed(0, &MODULE.replace("x+1", "x+8"));
        let next = run(&mut session, &next, &limits, fused).unwrap().unwrap();
        let retained = after
            .iter()
            .zip(&next)
            .map(|((before, _), (after, _))| {
                (0..3)
                    .filter(|&i| {
                        Arc::ptr_eq(
                            before.core.functions.owner(i).unwrap(),
                            after.core.functions.owner(i).unwrap(),
                        )
                    })
                    .count()
            })
            .sum::<usize>();
        assert_eq!(retained, 305);
    }
}

#[test]
fn recovered_continuation_rechecks_changed_parser_context_with_identical_source() {
    let limits = Limits::default();
    let mut session = ExpandSession::default();
    assert!(session.restore_frontend_products(seed(false), &limits));
    let before = run(&mut session, &inputs("fn main()->i32=>1;"), &limits, false)
        .unwrap()
        .unwrap();
    let mut current = inputs("fn main()->i32=>1;");
    let mut context = AttributeContext::default();
    context.configuration.insert(
        "unused".into(),
        zil_syntax::attributes::ConfigValue::Bool(true),
    );
    current[1] = ParsedSource::parse(Arc::new(current[1].source().clone()), &limits, &context);
    assert_eq!(
        current[1].source(),
        session.entries[&ModuleId(1)].parsed.source()
    );
    let after = run(&mut session, &current, &limits, false)
        .unwrap()
        .unwrap();
    let reference = run(&mut ExpandSession::default(), &current, &limits, false)
        .unwrap()
        .unwrap();
    assert_eq!(after, reference);
    assert!(Arc::ptr_eq(&before[0].0.core, &after[0].0.core));
    assert!(!Arc::ptr_eq(&before[1].0.core, &after[1].0.core));
    assert_eq!(session.last_stats().lower_modules, 1);
    assert_eq!(session.last_stats().reused_lower_modules, 1);
}

#[test]
fn body_growth_before_later_declaration_keeps_other_modules_recoverable() {
    for fused in [false, true] {
        let original = inputs("fn main()->i32=>1; fn later()->i32=>7;");
        let mut cold = ExpandSession::default();
        run(&mut cold, &original, &Limits::default(), fused)
            .unwrap()
            .unwrap();
        let products = cold.encode_frontend_products(&Limits::default()).unwrap();
        let current = inputs("fn main()->i32=>100000+2; fn later()->i32=>7;");
        let mut restored = ExpandSession::default();
        assert!(restored.restore_frontend_products(products, &Limits::default()));
        assert_eq!(
            run(&mut restored, &current, &Limits::default(), fused),
            run(
                &mut ExpandSession::default(),
                &current,
                &Limits::default(),
                fused
            )
        );
        assert_eq!(restored.last_stats().recovered_frontend_modules, 1);
        assert_eq!(restored.last_stats().lower_modules, 1);
        assert_eq!(restored.last_stats().normalize_modules, 1);
    }
}

#[test]
fn recovered_fees_keep_original_budget_failure_and_diagnostic_order() {
    for fused in [false, true] {
        let products = seed(fused);
        let current = inputs("fn main()->i32=>2;");
        for nodes in [false, true] {
            // Every prefix of this small reference crosses original Preflight,
            // Normalize, Probe, Lifetime, Header, Restore and Lower boundaries.
            for allowance in 0..600 {
                let mut limits = Limits::default();
                if nodes {
                    limits.max_nodes = allowance;
                } else {
                    limits.max_analysis_steps = allowance;
                }
                let mut restored = ExpandSession::default();
                assert!(restored.restore_frontend_products(products.clone(), &Limits::default()));
                let actual = run(&mut restored, &current, &limits, fused);
                let reference = run(&mut ExpandSession::default(), &current, &limits, fused);
                assert_eq!(
                    actual, reference,
                    "fused={fused}, nodes={nodes}, allowance={allowance}"
                );
                if actual.is_err() {
                    assert!(restored.entries.is_empty());
                }
            }
        }
    }
}

#[test]
fn malformed_products_are_misses_without_partial_fee_or_output_publication() {
    let products = seed(false);
    let bytes = products.modules[&ModuleId(1)].clone();
    let current = inputs("fn main()->i32=>2;");
    let reference = run(
        &mut ExpandSession::default(),
        &current,
        &Limits::default(),
        false,
    )
    .unwrap();
    for length in (0..bytes.len())
        .step_by(17)
        .chain(std::iter::once(bytes.len() - 1))
    {
        let mut corrupted = products.clone();
        corrupted
            .modules
            .insert(ModuleId(1), bytes[..length].into());
        let mut restored = ExpandSession::default();
        assert!(restored.restore_frontend_products(corrupted, &Limits::default()));
        assert_eq!(
            run(&mut restored, &current, &Limits::default(), false).unwrap(),
            reference
        );
        assert_eq!(restored.last_stats().recovered_frontend_modules, 0);
    }
    let mut extra = bytes.to_vec();
    extra.push(0);
    let mut corrupted = products;
    corrupted.modules.insert(ModuleId(1), extra.into());
    let mut restored = ExpandSession::default();
    assert!(restored.restore_frontend_products(corrupted, &Limits::default()));
    assert_eq!(
        run(&mut restored, &current, &Limits::default(), false).unwrap(),
        reference
    );
    assert_eq!(restored.last_stats().recovered_frontend_modules, 0);
}

#[test]
fn namespace_placement_and_original_refusals_are_current_inputs() {
    let products = seed(false);
    for current in [
        vec![
            parsed(0, "fn main()->i32=>2;"),
            parsed(1, "pub fn renamed(x:i32)->i32=>x;"),
        ],
        vec![
            parsed(0, "fn main()->i32=>2;"),
            parsed(1, "fn dep(x:i32)->i32=>x;"),
        ],
        vec![
            parsed(0, "fn main()->i32=>2;"),
            parsed(
                1,
                "// shifted\npub fn dep(x:i32)->i32{let y=x+3;if y>0{return y;}else{return x;}}",
            ),
        ],
        vec![
            parsed(0, "fn main()->i32=>2; fn main()->i32=>3;"),
            parsed(1, "pub fn dep(x:i32)->i32=>x;"),
        ],
        vec![
            parsed(0, "fn main()->i32=>2;"),
            parsed(
                1,
                "#[cfg(false)] fn hidden()->i32=>0;pub fn dep(x:i32)->i32=>x;",
            ),
        ],
        vec![parsed(0, "fn main()->i32=>2;"), parsed(1, "fn dep( -> ;")],
    ] {
        let mut restored = ExpandSession::default();
        assert!(restored.restore_frontend_products(products.clone(), &Limits::default()));
        assert_eq!(
            run(&mut restored, &current, &Limits::default(), false),
            run(
                &mut ExpandSession::default(),
                &current,
                &Limits::default(),
                false
            )
        );
        assert_eq!(restored.last_stats().recovered_frontend_modules, 0);
    }
}

#[test]
fn source_and_context_bytes_are_authority_not_record_selector() {
    let products = seed(false);
    let mut wrong = products.clone();
    wrong.context.push(0);
    let mut restored = ExpandSession::default();
    assert!(restored.restore_frontend_products(wrong, &Limits::default()));
    let current = inputs("fn main()->i32=>1;");
    assert_eq!(
        run(&mut restored, &current, &Limits::default(), false),
        run(
            &mut ExpandSession::default(),
            &current,
            &Limits::default(),
            false
        )
    );
    assert_eq!(restored.last_stats().recovered_frontend_modules, 0);
    let mut swapped = products;
    let a = swapped.modules[&ModuleId(0)].clone();
    let b = swapped.modules[&ModuleId(1)].clone();
    swapped.modules.insert(ModuleId(0), b);
    swapped.modules.insert(ModuleId(1), a);
    let mut restored = ExpandSession::default();
    assert!(restored.restore_frontend_products(swapped, &Limits::default()));
    assert_eq!(
        run(&mut restored, &current, &Limits::default(), false),
        run(
            &mut ExpandSession::default(),
            &current,
            &Limits::default(),
            false
        )
    );
    assert_eq!(restored.last_stats().recovered_frontend_modules, 0);
}

fn namespace_environment(text: &str) -> Environment {
    let input = parsed(0, text);
    assert!(
        input.parsed().diagnostics.is_empty(),
        "{:?}",
        input.parsed().diagnostics
    );
    assert!(ExpandSession::default().parsed_scalar(&input, &Limits::default()));
    Environment {
        sources: vec![(ModuleId(0), FileId(0), "m0.zil".into())],
        headers: vec![HeaderInput(Arc::new(module_headers(
            &input.parsed().module,
        )))],
        imports: vec![vec![]],
        limits: Limits::default(),
        target: TargetProfile::default(),
        context: "closed:0:[]".into(),
        fused_distribution: false,
    }
}

#[test]
fn canonical_namespace_bytes_match_the_existing_closed_products_predicate() {
    let declarations = [
        "fn f(x:i32)->i32=>x;",
        "fn f(y:i64,z:bool)->i64=>y+123456;",
        "pub fn f(x:i32)->i32=>x;",
        "#[test] fn f(x:i32)->i32=>x;",
        "fn renamed(x:i32)->i32=>x;",
        "extern \"C\" fn f(x:i32)->i32;",
        "extern \"other\" fn f(y:i64)->i64;",
        "import dep as a; fn f(x:i32)->i32=>x;",
        "import dep as b; fn f(x:i32)->i32=>x;",
        "import other as a; fn f(x:i32)->i32=>x;",
    ]
    .map(namespace_environment);
    for left in &declarations {
        for right in &declarations {
            assert_eq!(
                context(left) == context(right),
                left.scalar_products_compatible(right)
            );
        }
    }
    let baseline = namespace_environment("fn f(x:i32)->i32=>x;");
    let mut current = baseline.clone();
    current.limits.max_nodes -= 1;
    current.limits.max_analysis_steps -= 1;
    assert_eq!(context(&current), context(&baseline));
    current = baseline.clone();
    current.target.triple.push('x');
    assert_ne!(context(&current), context(&baseline));
    current = baseline.clone();
    current.limits.max_call_depth -= 1;
    assert_ne!(context(&current), context(&baseline));
    current = baseline.clone();
    current.sources[0].1 = FileId(1);
    assert_ne!(context(&current), context(&baseline));
    current = baseline.clone();
    current.fused_distribution = true;
    assert_ne!(context(&current), context(&baseline));
    current = baseline.clone();
    current.imports[0].push(CoreImport {
        alias: "dep".into(),
        module: ModuleId(1),
        span: Span::default(),
    });
    let with_import = context(&current);
    current.imports[0][0].span.end += 1;
    assert_ne!(
        context(&current),
        with_import,
        "exact current import diagnostic binding"
    );
    // An unclassified attribute cannot be represented as an admitted marker.
    current = namespace_environment("#[test] fn f()->i32=>1;");
    Arc::make_mut(&mut current.headers[0].0)[0].attributes[0].path = vec!["unknown".into()];
    assert!(context(&current).is_none());
}

#[test]
fn namespace_encoding_is_bounded_and_shared_only_under_its_exact_dependency_projection() {
    let mut environment = namespace_environment("fn f(x:i32)->i32=>x;");
    let length = context(&environment).unwrap().len();
    environment.limits.max_source_bytes = length;
    assert_eq!(context(&environment).unwrap().len(), length);
    environment.limits.max_source_bytes -= 1;
    assert!(context(&environment).is_none());

    let limits = Limits::default();
    let mut session = ExpandSession::default();
    run(
        &mut session,
        &inputs("fn main(x:i32)->i32=>x;"),
        &limits,
        false,
    )
    .unwrap()
    .unwrap();
    let products = session.encode_frontend_products(&limits).unwrap();
    let memo = session.frontend_context.clone();
    assert!(memo.get().unwrap().is_some());
    // Signature, body length and the later function's placement are separately
    // current inputs; none is a remote raw Normalize/Restore/Lower lookup.
    run(
        &mut session,
        &inputs("fn main(y:i64,z:bool)->i64=>y+123456;"),
        &limits,
        false,
    )
    .unwrap()
    .unwrap();
    assert!(Arc::ptr_eq(&memo, &session.frontend_context));
    assert_eq!(
        session.encode_frontend_products(&limits).unwrap().context,
        products.context
    );
    run(
        &mut session,
        &inputs("pub fn main(y:i64,z:bool)->i64=>y+123456;"),
        &limits,
        false,
    )
    .unwrap()
    .unwrap();
    assert!(!Arc::ptr_eq(&memo, &session.frontend_context));
    assert_ne!(
        session.encode_frontend_products(&limits).unwrap().context,
        products.context
    );
}

#[test]
fn projected_namespace_still_binds_complete_local_source_before_recovery() {
    let limits = Limits::default();
    let products = seed(false);
    let current = inputs("fn main(x:i64,y:bool)->i64=>x+123456;");
    let mut session = ExpandSession::default();
    assert!(session.restore_frontend_products(products, &limits));
    let actual = run(&mut session, &current, &limits, false).unwrap();
    assert_eq!(
        actual,
        run(&mut ExpandSession::default(), &current, &limits, false).unwrap()
    );
    assert_eq!(session.last_stats().recovered_frontend_modules, 1);
    assert_eq!(session.last_stats().lower_modules, 1);
}
