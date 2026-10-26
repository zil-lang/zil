use super::*;
use std::collections::BTreeSet;
use zil_ir::*;
fn function<'a>(m: &'a TypedModule, name: &str) -> &'a TypedFunction {
    m.functions.iter().find(|f| f.name == name).unwrap()
}
fn all(m: &TypedModule, f: &TypedFunction) -> oracle::Reference {
    let o = oracle::solve(m, f, budget::LIMIT).unwrap();
    for method in [bdd::Method::Mono, bdd::Method::Cut, bdd::Method::Wire] {
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
    for method in [bdd::Method::Mono, bdd::Method::Cut, bdd::Method::Wire] {
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
    for method in [bdd::Method::Mono, bdd::Method::Cut, bdd::Method::Wire] {
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
            for method in [bdd::Method::Mono, bdd::Method::Cut, bdd::Method::Wire] {
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
    for method in [bdd::Method::Mono, bdd::Method::Cut, bdd::Method::Wire] {
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
    for method in [bdd::Method::Mono, bdd::Method::Cut, bdd::Method::Wire] {
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
    for method in [bdd::Method::Mono, bdd::Method::Cut, bdd::Method::Wire] {
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
