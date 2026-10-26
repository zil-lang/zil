use super::*;
use std::collections::BTreeSet;
use zil_ir::*;
fn function<'a>(m: &'a TypedModule, name: &str) -> &'a TypedFunction {
    m.functions.iter().find(|f| f.name == name).unwrap()
}
fn all(m: &TypedModule, f: &TypedFunction) -> oracle::Reference {
    let o = oracle::solve(m, f, budget::LIMIT).unwrap();
    for method in [
        bdd::Method::Mono,
        bdd::Method::Cut,
        bdd::Method::Wire,
        bdd::Method::Fused,
    ] {
        let mut s = symbolic::solve(m, f, method, budget::LIMIT).unwrap();
        assert_eq!(compare(&mut s, &o).unwrap(), schema::QueryStatus::Complete);
    }
    o
}
fn production(m: TypedModule) -> R<()> {
    let lim = zil_base::Limits::default();
    let p = zil_analyze::analyze(&m, &lim).map_err(|e| format!("{e:?}"))?;
    zil_verify::verify_module(m, p, &zil_base::TargetProfile::host_llvm(), &lim)
        .map_err(|e| format!("{e:?}"))?;
    Ok(())
}
fn faults(o: &oracle::Reference, label: &str) -> bool {
    o.relations
        .iter()
        .any(|(k, v)| k.starts_with("fault:") && k.ends_with(label) && !v.is_empty())
}
const SIMPLE: &str = "resource T{n:i32} fn f(){let a=T{n:1};let b=T{n:2}} fn main()->i32{return 0}";

#[test]
fn complete_joint_rows_reject_the_cartesian_marginal_product() {
    let m = typed(
        "correlated",
        include_str!("../../../tests/perf/impl059/correlated.zil"),
    )
    .unwrap();
    let f = function(&m, "correlated");
    let o = all(&m, f);
    assert!(!o.relations.keys().any(|k| k.starts_with("fault:")));
    let a = f.locals.iter().find(|l| l.name == "a").unwrap().id.0;
    let b = f.locals.iter().find(|l| l.name == "b").unwrap().id.0;
    let found = o
        .relations
        .iter()
        .filter(|(k, _)| k.starts_with("point:"))
        .any(|(_, rows)| {
            let pairs = rows
                .iter()
                .map(|row| {
                    (
                        o.schema.get(row, &format!("s:L{a}")).unwrap(),
                        o.schema.get(row, &format!("s:L{b}")).unwrap(),
                    )
                })
                .collect::<BTreeSet<_>>();
            pairs == BTreeSet::from([(1, 2), (2, 1)])
        });
    assert!(
        found,
        "two real rows, with both-live/neither-live absent, must exist"
    );
    // This is a precision example, not permission to override production Verify.
    let result = production(m);
    eprintln!("correlated production outcome: {result:?}");
}

#[test]
fn one_changed_shared_guard_exposes_double_transfer() {
    let mut m = typed(
        "correlated",
        include_str!("../../../tests/perf/impl059/correlated.zil"),
    )
    .unwrap();
    let fi = m
        .functions
        .iter()
        .position(|f| f.name == "correlated")
        .unwrap();
    let f = &mut m.functions[fi];
    let flag = f.parameters[0];
    let mut first = None;
    let mut changed = false;
    for block in &mut f.blocks {
        for ins in &mut block.instructions {
            if ins.kind == InstructionKind::Load(flag) {
                if let Some(operand) = first {
                    ins.kind = InstructionKind::Unary {
                        op: UnaryOp::Not,
                        operand,
                    };
                    changed = true;
                    break;
                }
                first = ins.result;
            }
        }
        if changed {
            break;
        }
    }
    assert!(changed);
    let o = all(&m, &m.functions[fi]);
    assert!(faults(&o, "use-after-move"));
    assert!(production(m).is_err());
}

#[test]
fn unique_location_does_not_authorize_duplicate_cleanup() {
    let mut m = typed("duplicate", SIMPLE).unwrap();
    production(m.clone()).unwrap();
    let f = m.functions.iter_mut().find(|f| f.name == "f").unwrap();
    let block = &mut f.blocks[0];
    let at = block
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::DropManaged { .. }))
        .unwrap();
    block
        .instructions
        .insert(at + 1, block.instructions[at].clone());
    let o = all(&m, function(&m, "f"));
    assert!(faults(&o, "duplicate-cleanup"));
    assert!(production(m).is_err());
}

#[test]
fn cleanup_order_is_part_of_the_interface() {
    let mut m = typed("order", SIMPLE).unwrap();
    let f = m.functions.iter_mut().find(|f| f.name == "f").unwrap();
    let block = &mut f.blocks[0];
    let drops = block
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(i, v)| matches!(v.kind, InstructionKind::DropManaged { .. }).then_some(i))
        .collect::<Vec<_>>();
    assert_eq!(drops.len(), 2);
    block.instructions.swap(drops[0], drops[1]);
    let o = all(&m, function(&m, "f"));
    assert!(faults(&o, "cleanup-order"));
    assert!(production(m).is_err());
}

#[test]
fn description_future_use_prevents_owner_invalidation() {
    let mut m=typed("description","resource T{n:i32} fn f()->i32{let a=T{n:1};let alias=a;return alias.n} fn main()->i32{return 0}").unwrap();
    production(m.clone()).unwrap();
    let f = m.functions.iter_mut().find(|f| f.name == "f").unwrap();
    let local = f.locals.iter().find(|l| l.name == "a").unwrap().clone();
    let result = ValueId(f.values.len() as u32);
    f.values.push(local.ty);
    let mut inserted = false;
    for b in &mut f.blocks {
        if let Some(at) = b
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::ReadDesc { .. }))
        {
            let span = b.instructions[at].span;
            b.instructions.insert(
                at,
                Instruction {
                    result: Some(result),
                    kind: InstructionKind::TakeLocal { local: local.id },
                    span,
                },
            );
            inserted = true;
            break;
        }
    }
    assert!(inserted);
    let o = all(&m, function(&m, "f"));
    assert!(faults(&o, "live-description-invalidation"));
    assert!(production(m).is_err());
}

#[test]
fn owned_closure_keeps_every_captured_token() {
    let m = typed(
        "closure",
        include_str!("../../../tests/perf/impl059/closure.zil"),
    )
    .unwrap();
    production(m.clone()).unwrap();
    for f in &m.functions {
        if f.foreign.is_none() {
            all(&m, f);
        }
    }
    let f = function(&m, "owned");
    let o = all(&m, f);
    let owned = f.locals.iter().find(|l| l.name == "read").unwrap().id.0;
    let prefix = format!("i:L{owned}:");
    let keys = o
        .schema
        .fields
        .keys()
        .filter(|k| k.starts_with(&prefix))
        .collect::<Vec<_>>();
    assert!(o.relations.values().flatten().any(|row| {
        keys.iter()
            .filter(|k| o.schema.get(row, k).unwrap() != 0)
            .count()
            == 3
    }));
}

#[test]
fn error_scope_returns_and_abort_debt_are_both_retained() {
    let m = typed(
        "error",
        include_str!("../../../tests/perf/impl059/error.zil"),
    )
    .unwrap();
    production(m.clone()).unwrap();
    let f = function(&m, "transaction");
    let o = all(&m, f);
    let mut p = 0;
    let mut reached_error = false;
    for b in &f.blocks {
        for ins in &b.instructions {
            if matches!(
                ins.kind,
                InstructionKind::ScopeExit {
                    kind: CleanupExitKind::Error,
                    ..
                }
            ) && !o.relations[&format!("point:{p}")].is_empty()
            {
                reached_error = true;
            }
            p += 1;
        }
        p += 1;
    }
    assert!(reached_error);
    assert!(
        o.relations
            .keys()
            .filter(|k| k.starts_with("exit:"))
            .count()
            >= 2
    );
    let mut m = typed("abort", SIMPLE).unwrap();
    let f = m.functions.iter_mut().find(|f| f.name == "f").unwrap();
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::DropManaged { .. }))
        .unwrap();
    f.blocks[0].instructions.truncate(at);
    f.blocks[0].terminator = Terminator::Trap;
    let o = all(&m, function(&m, "f"));
    let live = o
        .schema
        .fields
        .keys()
        .filter(|k| k.starts_with("s:"))
        .collect::<Vec<_>>();
    assert!(
        o.relations
            .iter()
            .filter(|(k, _)| k.ends_with(":abort") && k.starts_with("exit:"))
            .flat_map(|(_, r)| r)
            .any(|row| live.iter().any(|k| o.schema.get(row, k).unwrap() == 1))
    );
    assert!(!faults(&o, "normal-exit-debt"));
}

#[test]
fn loop_redefinition_is_a_current_value_not_a_global_fixed_guard() {
    let m = typed("loop", include_str!("../../../tests/perf/impl059/loop.zil")).unwrap();
    production(m.clone()).unwrap();
    let f = function(&m, "looped");
    let o = all(&m, f);
    assert!(!o.relations.keys().any(|k| k.starts_with("fault:")));
    let local = f.locals.iter().find(|l| l.name == "again").unwrap().id.0;
    let mut p = 0;
    let mut seen_both = false;
    for b in &f.blocks {
        p += b.instructions.len();
        if matches!(b.terminator, Terminator::Branch { .. }) {
            let values = o.relations[&format!("point:{p}")]
                .iter()
                .map(|r| o.schema.get(r, &format!("b:L{local}")).unwrap())
                .collect::<BTreeSet<_>>();
            seen_both |= values == BTreeSet::from([0, 1]);
        }
        p += 1;
    }
    assert!(seen_both);
}

#[test]
fn overlapping_site_incarnation_is_not_collapsed_into_one_owner() {
    let mut m = typed(
        "multiplicity",
        "resource T{n:i32} fn f(){let a=T{n:1}} fn main()->i32{return 0}",
    )
    .unwrap();
    let f = m.functions.iter_mut().find(|f| f.name == "f").unwrap();
    let old = std::mem::take(&mut f.blocks[0].instructions);
    let make = old
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::MakeManaged { .. }))
        .unwrap();
    let store = old
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::Store { value, .. } if Some(value) == old[make].result))
        .unwrap();
    let mut prefix = old[..make].to_vec();
    let body = old[make..=store]
        .iter()
        .filter(|i| !matches!(i.kind, InstructionKind::StorageLive(_)))
        .cloned()
        .collect::<Vec<_>>();
    for ins in &old[make..=store] {
        if matches!(ins.kind, InstructionKind::StorageLive(_)) {
            prefix.push(ins.clone());
        }
    }
    f.blocks[0].instructions = prefix;
    f.blocks[0].terminator = Terminator::Jump(BlockId(1));
    f.blocks.push(BasicBlock {
        id: BlockId(1),
        instructions: body,
        terminator: Terminator::Jump(BlockId(1)),
    });
    let f = function(&m, "f");
    let o = oracle::solve(&m, f, budget::LIMIT).unwrap();
    assert_eq!(o.status, schema::QueryStatus::UnsupportedMultiplicity);
    for method in [
        bdd::Method::Mono,
        bdd::Method::Cut,
        bdd::Method::Wire,
        bdd::Method::Fused,
    ] {
        let mut s = symbolic::solve(&m, f, method, budget::LIMIT).unwrap();
        assert_eq!(
            compare(&mut s, &o).unwrap(),
            schema::QueryStatus::UnsupportedMultiplicity
        );
    }
    assert!(o.relations.keys().any(|k| k.starts_with("unsupported:")));
}

#[test]
fn actual_solver_and_oracle_work_boundaries_are_exact() {
    let m = typed(
        "boundary",
        "resource T{n:i32} fn f(flag:bool){let a=T{n:1};if flag{return}} fn main()->i32{return 0}",
    )
    .unwrap();
    let f = function(&m, "f");
    let o = oracle::solve(&m, f, budget::LIMIT).unwrap();
    let n = o.work;
    assert!(
        oracle::solve(&m, f, n - 1)
            .err()
            .unwrap()
            .starts_with("ResourceLimit:")
    );
    let exact = oracle::solve(&m, f, n).unwrap();
    let above = oracle::solve(&m, f, n + 1).unwrap();
    assert_eq!(exact.relations, above.relations);
    for method in [
        bdd::Method::Mono,
        bdd::Method::Cut,
        bdd::Method::Wire,
        bdd::Method::Fused,
    ] {
        let s = symbolic::solve(&m, f, method, budget::LIMIT).unwrap();
        let n = s.bdd.work.used;
        assert!(
            symbolic::solve(&m, f, method, n - 1)
                .err()
                .unwrap()
                .starts_with("ResourceLimit:")
        );
        assert_eq!(symbolic::solve(&m, f, method, n).unwrap().bdd.work.used, n);
        assert_eq!(
            symbolic::solve(&m, f, method, n + 1).unwrap().bdd.work.used,
            n
        );
    }
}

#[test]
fn exact_cardinality_overflow_is_never_agreement() {
    let mut b = bdd::Bdd::new(128, budget::LIMIT);
    assert!(b.count(1).unwrap_err().starts_with("ResourceLimit:"));
    assert_eq!(b.count(0).unwrap(), 0);
}

#[test]
fn relational_images_exhaust_every_three_bit_input_relation() {
    // All 256 sets of 3-bit states; swaps, fan-out, a last-use equality, constants,
    // and havoc. Reference enumerates truth-table images without BDD operations.
    for mask in 0u16..256 {
        for case in 0..5 {
            let mut b = bdd::Bdd::new(3, budget::LIMIT);
            let mut input = 0;
            for value in 0..8 {
                if mask >> value & 1 != 0 {
                    let mut term = 1;
                    for bit in 0..3 {
                        let v = b.var(bit, false).unwrap();
                        let lit = if value >> bit & 1 == 0 {
                            b.not(v).unwrap()
                        } else {
                            v
                        };
                        term = b.and(term, lit).unwrap();
                    }
                    input = b.or(input, term).unwrap();
                }
            }
            let vars = (0..3).map(|i| b.var(i, false).unwrap()).collect::<Vec<_>>();
            let not_z = b.not(vars[2]).unwrap();
            let xor = b.xor(vars[0], vars[1]).unwrap();
            let rhs = match case {
                0 => vec![vars[1], vars[0], 0],
                1 => vec![0, vars[0], vars[2]],
                2 => vec![vars[1], vars[1], vars[0]],
                3 => vec![0, xor, not_z],
                _ => vec![vars[2], vars[0], vars[1]],
            };
            let mut factors = vec![];
            for (bit, rhs) in rhs.into_iter().enumerate() {
                let post = b.var(bit, true).unwrap();
                let factor = if case == 3 && bit == 0 {
                    1
                } else {
                    b.equal(post, rhs).unwrap()
                };
                factors.push((bit, factor));
            }
            let mut expected = BTreeSet::new();
            for state in 0u32..8 {
                if mask >> state & 1 == 0 {
                    continue;
                }
                let x = state & 1;
                let y = state >> 1 & 1;
                let z = state >> 2 & 1;
                match case {
                    0 => {
                        expected.insert(y | x << 1);
                    }
                    1 => {
                        expected.insert(x << 1 | z << 2);
                    }
                    2 => {
                        expected.insert(y | y << 1 | x << 2);
                    }
                    3 => {
                        expected.insert((x ^ y) << 1 | (1 - z) << 2);
                        expected.insert(1 | (x ^ y) << 1 | (1 - z) << 2);
                    }
                    _ => {
                        expected.insert(z | x << 1 | y << 2);
                    }
                }
            }
            for method in [
                bdd::Method::Mono,
                bdd::Method::Cut,
                bdd::Method::Wire,
                bdd::Method::Fused,
            ] {
                let plan = b.plan(&factors, method).unwrap();
                let actual = b.image(input, &plan).unwrap();
                assert_eq!(b.count(actual).unwrap(), expected.len() as u128);
                let rows = b.rows(actual, 8).unwrap();
                assert_eq!(rows, expected.iter().map(|v| vec![u64::from(*v)]).collect());
                for value in 0..8 {
                    assert_eq!(
                        b.contains(actual, &[value]).unwrap(),
                        expected.contains(&(value as u32)),
                        "mask={mask} case={case} method={method:?} state={value}"
                    );
                }
            }
        }
    }
}

#[test]
fn post_ports_must_be_unique_and_absent_from_other_factors() {
    for method in [
        bdd::Method::Mono,
        bdd::Method::Cut,
        bdd::Method::Wire,
        bdd::Method::Fused,
    ] {
        let mut b = bdd::Bdd::new(2, budget::LIMIT);
        let px = b.var(0, true).unwrap();
        let py = b.var(1, true).unwrap();
        let cross = b.equal(px, py).unwrap();
        assert!(
            b.plan(&[(0, cross)], method)
                .err()
                .unwrap()
                .starts_with("Unsupported:")
        );
        assert!(b.plan(&[(0, px), (0, px)], method).is_err());
    }
}

#[test]
fn wire_waits_for_last_incidence_and_does_not_substitute_non_equalities() {
    let mut b = bdd::Bdd::new(2, budget::LIMIT);
    let x = b.var(0, false).unwrap();
    let y = b.var(1, false).unwrap();
    let px = b.var(0, true).unwrap();
    let py = b.var(1, true).unwrap();
    let shared = b.equal(x, y).unwrap();
    let first = b.equal(px, x).unwrap();
    let later = b.equal(py, x).unwrap();
    let plan = b
        .plan(&[(0, first), (1, later)], bdd::Method::Wire)
        .unwrap();
    let out = b.image(shared, &plan).unwrap();
    assert_eq!(
        b.wire_substitutions, 1,
        "the reverse-first fan-out factor may not retire x"
    );
    assert_eq!(b.count(out).unwrap(), 2);
    assert!(b.contains(out, &[0]).unwrap());
    assert!(b.contains(out, &[3]).unwrap());
    let not_equal_factor = b.or(px, x).unwrap();
    let before = b.wire_substitutions;
    let plan = b.plan(&[(0, not_equal_factor)], bdd::Method::Wire).unwrap();
    let cut = b.image(shared, &plan).unwrap();
    assert_eq!(before, b.wire_substitutions);
    let plan = b.plan(&[(0, not_equal_factor)], bdd::Method::Mono).unwrap();
    let mono = b.image(shared, &plan).unwrap();
    assert_eq!(mono, cut);
}

#[test]
fn scope_labels_are_audited_even_when_state_transfer_is_identity() {
    let m = typed(
        "error",
        include_str!("../../../tests/perf/impl059/error.zil"),
    )
    .unwrap();
    let original = oracle::solve(&m, function(&m, "transaction"), budget::LIMIT).unwrap();
    assert!(
        original
            .relations
            .keys()
            .any(|k| k.ends_with(":error-return"))
    );
    assert!(
        original
            .relations
            .keys()
            .any(|k| k.ends_with(":success-return"))
    );
    let mut changed = m.clone();
    let f = changed
        .functions
        .iter_mut()
        .find(|f| f.name == "transaction")
        .unwrap();
    let mut edited = false;
    for ins in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let InstructionKind::ScopeExit { kind, .. } = &mut ins.kind
            && *kind == CleanupExitKind::Error
        {
            *kind = CleanupExitKind::Return;
            edited = true;
            break;
        }
    }
    assert!(edited);
    let mut s = symbolic::solve(
        &changed,
        function(&changed, "transaction"),
        bdd::Method::Wire,
        budget::LIMIT,
    )
    .unwrap();
    assert_eq!(
        compare(&mut s, &original).unwrap_err(),
        "Mismatch: raw event labels/CFG"
    );
    // The label audit is distinct from independently rechecking scope-stack validity.
    let mutated = all(&changed, function(&changed, "transaction"));
    assert_eq!(original.relations, mutated.relations);
    assert_ne!(original.labels, mutated.labels);
}

#[test]
fn guard_insensitive_future_description_use_is_explicitly_conservative() {
    let m = typed("guarded_desc", "resource T{n:i32} fn consume(give x:T){} fn f(flag:bool){let a=T{n:1};let alias=a;if flag{consume(give a)}if !flag{let n=alias.n}} fn main()->i32{return 0}").unwrap();
    let o = all(&m, function(&m, "f"));
    assert!(faults(&o, "live-description-invalidation"));
    // The use is on !flag, so this is deliberately NOT evidence of a concrete
    // execution error. It is a false positive of the admitted may-future query.
}

#[test]
fn give_result_different_nominal_type_is_unsupported() {
    let m = typed("give_types", "resource A{n:i32} resource B{n:i32} fn convert(give a:A)->B contract{result(give)}{return B{n:2}} fn f(){let a=A{n:1};let b=convert(give a)} fn main()->i32{return 0}").unwrap();
    let f = function(&m, "f");
    assert!(
        oracle::solve(&m, f, budget::LIMIT)
            .err()
            .unwrap()
            .starts_with("Unsupported:")
    );
    for method in [
        bdd::Method::Mono,
        bdd::Method::Cut,
        bdd::Method::Wire,
        bdd::Method::Fused,
    ] {
        assert!(
            symbolic::solve(&m, f, method, budget::LIMIT)
                .err()
                .unwrap()
                .starts_with("Unsupported:")
        );
    }
}

#[test]
fn metadata_and_point_bounds_precede_cross_product_allocation() {
    let mut m = typed("size", SIMPLE).unwrap();
    let fi = m.functions.iter().position(|f| f.name == "f").unwrap();
    m.functions[fi]
        .values
        .resize(budget::MAX_SLOTS + 1, Type::Bool);
    assert!(
        oracle::solve(&m, &m.functions[fi], budget::LIMIT)
            .err()
            .unwrap()
            .contains("slot metadata")
    );
    assert!(
        symbolic::solve(&m, &m.functions[fi], bdd::Method::Wire, budget::LIMIT)
            .err()
            .unwrap()
            .contains("slot metadata")
    );
    let mut m = typed("points", SIMPLE).unwrap();
    let fi = m.functions.iter().position(|f| f.name == "f").unwrap();
    let ins = m.functions[fi].blocks[0].instructions[0].clone();
    m.functions[fi].blocks[0]
        .instructions
        .resize(budget::MAX_POINTS + 1, ins);
    assert!(
        oracle::solve(&m, &m.functions[fi], budget::LIMIT)
            .err()
            .unwrap()
            .contains("points")
    );
    assert!(
        symbolic::solve(&m, &m.functions[fi], bdd::Method::Wire, budget::LIMIT)
            .err()
            .unwrap()
            .contains("points")
    );
    let source = format!(
        "resource T{{n:i32}} fn f(){{{}}} fn main()->i32{{return 0}}",
        (0..50)
            .map(|i| format!("let v{i}=T{{n:1}};"))
            .collect::<String>()
    );
    let m = typed("cross", &source).unwrap();
    let f = function(&m, "f");
    assert!(
        oracle::solve(&m, f, budget::LIMIT)
            .err()
            .unwrap()
            .contains("before cross products")
    );
    assert!(
        symbolic::solve(&m, f, bdd::Method::Wire, budget::LIMIT)
            .err()
            .unwrap()
            .contains("before cross-product")
    );
}

#[test]
fn a_reinitialized_position_does_not_revive_an_old_description_or_borrowed_closure() {
    for use_closure in [false, true] {
        let definition = if use_closure {
            "let alias=()=>a.n"
        } else {
            "let alias=a"
        };
        let access = if use_closure { "alias()" } else { "alias.n" };
        let text = format!(
            "resource T{{n:i32}} fn consume(give a:T){{}} fn f(flag:bool){{let a=T{{n:1}};{definition};var again=flag;while again{{consume(give a);let replacement=T{{n:2}};let n={access};again=false}}}} fn main()->i32{{return 0}}"
        );
        let mut m = typed("stale_activation", &text).unwrap();
        let f = m.functions.iter_mut().find(|f| f.name == "f").unwrap();
        let old = f.locals.iter().find(|l| l.name == "a").unwrap().id;
        let replacement = f
            .locals
            .iter()
            .find(|l| l.name == "replacement")
            .unwrap()
            .id;
        // Source replacement is closed in schema 8. Redirect the real new
        // constructor's Store to the old position in this raw-IR negative probe.
        for ins in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if let InstructionKind::Store { local, .. } = &mut ins.kind
                && *local == replacement
            {
                *local = old;
            }
        }
        let o = all(&m, function(&m, "f"));
        assert!(
            faults(&o, "live-description-invalidation"),
            "borrowed closure = {use_closure}"
        );
        assert_eq!(o.status, schema::QueryStatus::Complete);
    }
}

#[test]
fn a_unique_carrier_can_still_leak_at_normal_return() {
    let mut m = typed("leak", SIMPLE).unwrap();
    let f = m.functions.iter_mut().find(|f| f.name == "f").unwrap();
    // No duplicated owner, move or release: omit the entire terminal cleanup.
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::DropManaged { .. }))
        .unwrap();
    f.blocks[0].instructions.truncate(at);
    f.blocks[0].terminator = Terminator::Return(None);
    let o = all(&m, function(&m, "f"));
    assert!(faults(&o, "normal-exit-debt"));
    assert!(!faults(&o, "duplicate-transfer"));
    assert!(production(m).is_err());
}

#[test]
fn complete_check_census_and_materialization_share_the_candidate_budget() {
    let m = typed("full_budget", SIMPLE).unwrap();
    let f = function(&m, "f");
    let o = oracle::solve(&m, f, budget::LIMIT).unwrap();
    let full = |limit, method| -> R<usize> {
        let mut s = symbolic::solve(&m, f, method, limit)?;
        compare(&mut s, &o)?;
        for root in s.relations.values() {
            s.bdd.shape(*root)?;
        }
        let data = report::materialize(&mut s, &o)?;
        s.bdd.work.charge(data.len())?;
        std::hint::black_box(zil_base::Fingerprint::of(data.as_bytes()));
        Ok(s.bdd.work.used)
    };
    for method in [
        bdd::Method::Mono,
        bdd::Method::Cut,
        bdd::Method::Wire,
        bdd::Method::Fused,
    ] {
        let n = full(budget::LIMIT, method).unwrap();
        assert!(
            full(n - 1, method)
                .unwrap_err()
                .starts_with("ResourceLimit:")
        );
        assert_eq!(full(n, method).unwrap(), n);
        assert_eq!(full(n + 1, method).unwrap(), n);
    }
}

#[test]
fn fused_quantification_matches_independent_truth_enumeration() {
    let mut b = bdd::Bdd::new(4, budget::LIMIT);
    let vars = (0..4).map(|i| b.var(i, false).unwrap()).collect::<Vec<_>>();
    // A family with sharing, complement, equality, and absent variables.
    let mut functions = vec![0, 1];
    for &x in &vars {
        functions.push(x);
        functions.push(b.not(x).unwrap());
    }
    for &x in &vars {
        for &y in &vars {
            functions.push(b.and(x, y).unwrap());
            functions.push(b.xor(x, y).unwrap());
            functions.push(b.equal(x, y).unwrap());
        }
    }
    for &x in &functions {
        for &y in &functions {
            for mask in 0u16..16 {
                let abstracted = (0..4)
                    .filter(|i| mask & (1 << i) != 0)
                    .map(|i| i * 2)
                    .collect::<BTreeSet<_>>();
                let result = b.and_exists(x, y, &abstracted).unwrap();
                for assignment in 0u64..16 {
                    let expected = (0u64..16).any(|witness| {
                        if ((assignment ^ witness) & !u64::from(mask)) != 0 {
                            return false;
                        }
                        b.contains(x, &[witness]).unwrap() && b.contains(y, &[witness]).unwrap()
                    });
                    assert_eq!(b.contains(result, &[assignment]).unwrap(), expected);
                }
            }
        }
    }
}

#[test]
fn reordered_rank_preserves_canonical_materialization_and_all_images() {
    let m = typed(
        "correlated",
        include_str!("../../../tests/perf/impl059/correlated.zil"),
    )
    .unwrap();
    for f in m.functions.iter().filter(|f| f.foreign.is_none()) {
        let o = oracle::solve(&m, f, budget::LIMIT).unwrap();
        for order in ["owner", "token"] {
            for method in [
                bdd::Method::Mono,
                bdd::Method::Cut,
                bdd::Method::Wire,
                bdd::Method::Fused,
            ] {
                let mut s = symbolic::solve_diagnostic(
                    &m,
                    f,
                    method,
                    budget::LIMIT,
                    &mut telemetry::Telemetry::disabled(),
                    order,
                    false,
                )
                .unwrap();
                assert_eq!(compare(&mut s, &o).unwrap(), schema::QueryStatus::Complete);
                report::materialize(&mut s, &o).unwrap();
            }
        }
    }
}

#[test]
fn permutation_and_joint_root_census_do_not_hide_shared_nodes() {
    let mut b = bdd::Bdd::new(4, budget::LIMIT);
    assert!(b.set_order(vec![0, 0, 2, 3]).is_err());
    b.set_order(vec![3, 1, 0, 2]).unwrap();
    let x = b.var(0, false).unwrap();
    let y = b.var(2, false).unwrap();
    let xy = b.and(x, y).unwrap();
    let mut diagnostic = budget::Work::new(budget::LIMIT);
    let count = b.reachable_nodes(&[xy, x, xy], &mut diagnostic).unwrap();
    assert_eq!(count, 3); // xy has y as child; separate x shares its variable only.
    let rows = b.rows(xy, budget::MAX_ROWS).unwrap();
    assert_eq!(rows, [5, 7, 13, 15].map(|v| vec![v]).into_iter().collect());
    assert!(b.set_order(vec![0, 1, 2, 3]).is_err());
}

#[test]
fn collection_preserves_roots_and_clears_recycled_id_cache_entries() {
    let mut b = bdd::Bdd::new(4, budget::LIMIT);
    let x = b.var(0, false).unwrap();
    let y = b.var(1, false).unwrap();
    let z = b.var(2, false).unwrap();
    let xy = b.xor(x, y).unwrap();
    let doomed = b.and(xy, z).unwrap();
    assert!(
        b.node_count()
            > b.reachable_nodes(&[xy], &mut budget::Work::new(budget::LIMIT))
                .unwrap()
    );
    let rows = b.rows(xy, budget::MAX_ROWS).unwrap();
    b.collect(&[xy]).unwrap();
    assert!(b.collected_nodes > 0);
    assert!(b.gc_work > 0);
    // Build different expressions after slot reuse, then recompute a result
    // formerly cached using IDs which collection made available for reuse.
    let u = b.var(3, false).unwrap();
    let nu = b.not(u).unwrap();
    let nxy = b.not(xy).unwrap();
    let noise = b.and(nxy, nu).unwrap();
    let z = b.var(2, false).unwrap();
    let new_result = b.and(xy, z).unwrap();
    let _old_id_is_not_a_handle = doomed;
    assert_eq!(b.rows(xy, budget::MAX_ROWS).unwrap(), rows);
    for assignment in 0u64..16 {
        assert_eq!(
            b.contains(new_result, &[assignment]).unwrap(),
            (assignment & 1 != (assignment >> 1) & 1) && assignment & 4 != 0
        );
        assert_eq!(
            b.contains(noise, &[assignment]).unwrap(),
            (assignment & 1 == (assignment >> 1) & 1) && assignment & 8 == 0
        );
    }
}

#[test]
fn four_independent_guards_fit_same_arena_with_explicit_safe_boundary_collection() {
    let m = typed(
        "independent",
        include_str!("../../../tests/perf/impl059/independent.zil"),
    )
    .unwrap();
    let f = function(&m, "independent");
    let o = oracle::solve(&m, f, budget::LIMIT).unwrap();
    for method in [
        bdd::Method::Mono,
        bdd::Method::Cut,
        bdd::Method::Wire,
        bdd::Method::Fused,
    ] {
        let mut s = symbolic::solve_diagnostic(
            &m,
            f,
            method,
            budget::LIMIT,
            &mut telemetry::Telemetry::disabled(),
            "canonical",
            true,
        )
        .unwrap();
        assert!(s.bdd.gc_count > 0);
        assert!(s.bdd.node_count() <= budget::MAX_NODES);
        assert!(s.bdd.created_nodes > budget::MAX_NODES);
        assert_eq!(compare(&mut s, &o).unwrap(), schema::QueryStatus::Complete);
        report::materialize(&mut s, &o).unwrap();
    }
}

#[test]
fn joined_conditional_cleanup_exposes_stricter_research_event_contract() {
    let mut m = typed(
        "gc_fault",
        include_str!("../../../tests/perf/impl059/independent.zil"),
    )
    .unwrap();
    let f = m
        .functions
        .iter_mut()
        .find(|f| f.name == "independent")
        .unwrap();
    let block = f
        .blocks
        .iter_mut()
        .rev()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.kind, InstructionKind::DropManaged { .. }))
        })
        .unwrap();
    let at = block
        .instructions
        .iter()
        .rposition(|i| matches!(i.kind, InstructionKind::DropManaged { .. }))
        .unwrap();
    block
        .instructions
        .insert(at + 1, block.instructions[at].clone());
    let f = function(&m, "independent");
    let o = oracle::solve(&m, f, budget::LIMIT).unwrap();
    assert!(faults(&o, "duplicate-cleanup"));
    let mut s = symbolic::solve_diagnostic(
        &m,
        f,
        bdd::Method::Fused,
        budget::LIMIT,
        &mut telemetry::Telemetry::disabled(),
        "owner",
        true,
    )
    .unwrap();
    assert!(s.bdd.gc_count > 0);
    assert_eq!(compare(&mut s, &o).unwrap(), schema::QueryStatus::Complete);
    let canonical = report::materialize(&mut s, &o).unwrap();
    let actual_production = production(m.clone());
    counterexample_receipt(
        "joined-conditional-duplicate",
        include_str!("../../../tests/perf/impl059/independent.zil"),
        &m,
        &o,
        &canonical,
        &actual_production,
    );
    // Production only rejects an exactly-DONE abstract cell. A MOVED|DONE
    // join is admitted and lowering uses a runtime responsibility flag. This
    // stricter research event rejection is not a claim of a machine double free.
    assert!(actual_production.is_ok());
}

#[test]
fn collection_scanning_and_rebuilding_share_the_complete_candidate_budget() {
    let m = typed(
        "gc_budget",
        include_str!("../../../tests/perf/impl059/independent.zil"),
    )
    .unwrap();
    let f = function(&m, "independent");
    let o = oracle::solve(&m, f, budget::LIMIT).unwrap();
    let full = |limit| -> R<usize> {
        let mut s = symbolic::solve_diagnostic(
            &m,
            f,
            bdd::Method::Fused,
            limit,
            &mut telemetry::Telemetry::disabled(),
            "owner",
            true,
        )?;
        assert!(s.bdd.gc_work > 0);
        compare(&mut s, &o)?;
        for (key, root) in &s.relations {
            if key.starts_with("point:") {
                s.bdd.shape(*root)?;
            }
        }
        let data = report::materialize(&mut s, &o)?;
        s.bdd.work.charge(data.len())?;
        std::hint::black_box(zil_base::Fingerprint::of(data.as_bytes()));
        Ok(s.bdd.work.used)
    };
    let n = full(budget::LIMIT).unwrap();
    println!("063 deterministic GC budget n={n}");
    assert!(full(n - 1).unwrap_err().starts_with("ResourceLimit:"));
    assert_eq!(full(n).unwrap(), n);
    assert_eq!(full(n + 1).unwrap(), n);
}

#[test]
fn collection_preserves_definite_duplicate_cleanup_rejected_by_production() {
    let source = include_str!("../../../tests/perf/impl059/independent.zil").replace(
        "if d { consume(give r3) }",
        "if d { consume(give r3) }\nlet retained=Token{value:5}",
    );
    let mut m = typed("gc_definite_fault", &source).unwrap();
    let f = m
        .functions
        .iter_mut()
        .find(|f| f.name == "independent")
        .unwrap();
    let local = f.locals.iter().find(|l| l.name == "retained").unwrap().id;
    let block = f
        .blocks
        .iter_mut()
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| i.kind == InstructionKind::DropManaged { local })
        })
        .unwrap();
    let at = block
        .instructions
        .iter()
        .position(|i| i.kind == InstructionKind::DropManaged { local })
        .unwrap();
    block
        .instructions
        .insert(at + 1, block.instructions[at].clone());
    let f = function(&m, "independent");
    let o = oracle::solve(&m, f, budget::LIMIT).unwrap();
    assert!(faults(&o, "duplicate-cleanup"));
    let mut s = symbolic::solve_diagnostic(
        &m,
        f,
        bdd::Method::Fused,
        budget::LIMIT,
        &mut telemetry::Telemetry::disabled(),
        "owner",
        true,
    )
    .unwrap();
    assert!(s.bdd.gc_count > 0);
    assert_eq!(compare(&mut s, &o).unwrap(), schema::QueryStatus::Complete);
    let canonical = report::materialize(&mut s, &o).unwrap();
    let actual_production = production(m.clone());
    counterexample_receipt(
        "definite-duplicate",
        &source,
        &m,
        &o,
        &canonical,
        &actual_production,
    );
    assert!(actual_production.is_err());
}

fn counterexample_receipt(
    name: &str,
    source: &str,
    raw: &TypedModule,
    reference: &oracle::Reference,
    candidate: &str,
    production: &R<()>,
) {
    let Some(directory) = std::env::var_os("ZIL_063_TEST_EVIDENCE") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory).join(name);
    assert!(
        !directory.exists(),
        "never overwrite an earlier counterexample receipt"
    );
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("source-before-raw-mutation.zil"), source).unwrap();
    std::fs::write(directory.join("mutated-typed-ir.txt"), format!("{raw:#?}")).unwrap();
    std::fs::write(directory.join("candidate-canonical.json"), candidate).unwrap();
    let oracle = report::canonical(
        &reference.schema,
        &reference.labels,
        &reference.relations,
        reference.status,
        &mut budget::Work::new(budget::LIMIT),
    )
    .unwrap();
    assert_eq!(oracle, candidate);
    std::fs::write(directory.join("oracle-canonical.json"), oracle).unwrap();
    std::fs::write(
        directory.join("production.txt"),
        format!("{production:?}\n"),
    )
    .unwrap();
}
