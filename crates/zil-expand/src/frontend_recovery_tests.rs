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
