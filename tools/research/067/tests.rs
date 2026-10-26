use super::*;
use encoding::Representation;
use std::collections::BTreeMap;
use zil_ir::*;

fn named<'a>(m: &'a TypedModule, name: &str) -> &'a TypedFunction {
    m.functions.iter().find(|f| f.name == name).unwrap()
}
fn both(name: &str, m: &TypedModule, f: &TypedFunction) -> oracle::Reference {
    let reference = oracle::solve(m, f, budget::LIMIT).unwrap();
    let mut encodings = Vec::new();
    let directory = std::env::var_os("ZIL_067_TEST_EVIDENCE").map(|root| {
        let path = std::path::PathBuf::from(root).join(name);
        assert!(
            !path.exists(),
            "counterexample evidence must never be overwritten"
        );
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("raw-typed-ir.txt"), format!("{m:#?}")).unwrap();
        let raw = report::canonical(
            &reference.schema,
            &reference.labels,
            &reference.relations,
            reference.status,
            &mut budget::Work::new(budget::LIMIT),
        )
        .unwrap();
        std::fs::write(path.join("oracle.json"), raw).unwrap();
        path
    });
    for representation in [Representation::Dense, Representation::Location] {
        for method in [bdd::Method::Mono, bdd::Method::Fused] {
            let mut s = symbolic::solve_encoded(
                m,
                f,
                method,
                budget::LIMIT,
                &mut telemetry::Telemetry::disabled(),
                "owner",
                true,
                representation,
            )
            .unwrap();
            assert_eq!(compare(&mut s, &reference).unwrap(), reference.status);
            let bytes = report::materialize(&mut s, &reference).unwrap();
            if let Some(path) = &directory {
                std::fs::write(
                    path.join(format!("{}-{}.json", representation.name(), method.name())),
                    &bytes,
                )
                .unwrap();
            }
            encodings.push(bytes);
        }
    }
    assert!(encodings.windows(2).all(|p| p[0] == p[1]));
    reference
}
const CLOSURE: &str = "resource T{n:i32} fn consume(give x:T){} fn f(){let a=T{n:1};let b=T{n:2};let cb=move()=>{consume(give a);consume(give b)};cb()} fn main()->i32{return 0}";

#[test]
fn complete_correlated_relations_decode_identically_in_both_representations() {
    let m = typed(
        "correlated",
        include_str!("../../../tests/perf/impl059/correlated.zil"),
    )
    .unwrap();
    let reference = both("correlated", &m, named(&m, "correlated"));
    assert!(!reference.relations.keys().any(|k| k.starts_with("fault:")));
}

#[test]
fn distinct_tokens_may_share_one_environment_location() {
    let m = typed("closure", CLOSURE).unwrap();
    let reference = both("shared_environment", &m, named(&m, "f"));
    assert!(!reference.relations.keys().any(|k| k.starts_with("fault:")));
    let f = named(&m, "f");
    let layout = extract::extract(&m, f, &mut budget::Work::new(budget::LIMIT)).unwrap();
    let encoding = encoding::Encoding::new(
        &layout,
        Representation::Location,
        &mut budget::Work::new(budget::LIMIT),
    )
    .unwrap();
    assert!(encoding.location_bits < encoding.incidence_bits);
    assert!(encoding.physical.bits < layout.schema.bits);
    let witnessed = reference
        .relations
        .values()
        .flat_map(|r| r.iter())
        .any(|row| {
            layout.owners.iter().any(|owner| {
                layout
                    .tokens
                    .iter()
                    .filter(|token| {
                        reference
                            .schema
                            .get(row, &format!("i:{owner}:{token}"))
                            .unwrap()
                            == 1
                    })
                    .count()
                    > 1
            })
        });
    assert!(witnessed, "shared carrier must not mean one-token capacity");
}

#[test]
fn duplicate_capture_occurrences_preserve_exact_fault_prestate() {
    let mut m = typed("duplicate_capture", CLOSURE).unwrap();
    let f = m.functions.iter_mut().find(|f| f.name == "f").unwrap();
    let captures = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find_map(|i| match &mut i.kind {
            InstructionKind::MakeClosure { captures, .. } => Some(captures),
            _ => None,
        })
        .unwrap();
    assert_eq!(captures.len(), 2);
    captures[1] = captures[0];
    let reference = both("duplicate_capture", &m, named(&m, "f"));
    assert!(
        reference
            .relations
            .iter()
            .any(|(label, rows)| label.ends_with("duplicate-transfer") && !rows.is_empty())
    );
    // Each original fault relation must retain the full point prestate, not
    // a collapsed invalid marker or a sequentially updated second operand.
    for (label, rows) in &reference.relations {
        if label.starts_with("fault:") && label.ends_with("duplicate-transfer") {
            let point = label.split(':').nth(1).unwrap();
            assert_eq!(
                Some(rows),
                reference.relations.get(&format!("point:{point}"))
            );
        }
    }
}

#[test]
fn duplicate_call_operands_are_observed_before_transfer() {
    let mut m = typed("duplicate_call", "resource T{n:i32} fn consume(give a:T,give b:T){} fn f(){let a=T{n:1};let b=T{n:2};consume(give a,give b)} fn main()->i32{return 0}").unwrap();
    let f = m.functions.iter_mut().find(|f| f.name == "f").unwrap();
    let arguments = f
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find_map(|i| match &mut i.kind {
            InstructionKind::Call { arguments, .. } => Some(arguments),
            _ => None,
        })
        .unwrap();
    assert_eq!(arguments.len(), 2);
    arguments[1] = arguments[0];
    let reference = both("duplicate_call", &m, named(&m, "f"));
    let faults = reference
        .relations
        .iter()
        .filter(|(label, _)| label.starts_with("fault:") && label.ends_with("duplicate-transfer"))
        .collect::<Vec<_>>();
    assert!(!faults.is_empty());
    for (label, rows) in faults {
        assert!(!rows.is_empty());
        let point = label.split(':').nth(1).unwrap();
        assert_eq!(
            Some(rows),
            reference.relations.get(&format!("point:{point}"))
        );
    }
}

#[test]
fn invalid_codec_input_is_rejected_instead_of_normalized() {
    let m = typed("codec", CLOSURE).unwrap();
    let layout =
        extract::extract(&m, named(&m, "f"), &mut budget::Work::new(budget::LIMIT)).unwrap();
    let mut work = budget::Work::new(budget::LIMIT);
    let encoding = encoding::Encoding::new(&layout, Representation::Location, &mut work).unwrap();
    let token = &layout.tokens[0];
    let mut row = vec![0; layout.schema.bits.div_ceil(64)];
    for owner in &layout.owners[..2] {
        layout
            .schema
            .set(&mut row, &format!("i:{owner}:{token}"), 1)
            .unwrap();
    }
    assert!(
        encoding
            .encode(&row, &mut work)
            .unwrap_err()
            .contains("multiplicity")
    );
    let mut values = BTreeMap::new();
    for bit in 0..layout.schema.bits {
        values.insert(bit, Some(row[bit / 64] >> (bit % 64) & 1 != 0));
    }
    let mut bdd = bdd::Bdd::new(encoding.physical.bits, budget::LIMIT);
    assert!(
        encoding
            .initial(&values, &mut bdd)
            .unwrap_err()
            .contains("multiplicity")
    );
}

#[test]
fn normal_transfer_multiplicity_is_an_observed_invariant_violation() {
    let m = typed("invariant", CLOSURE).unwrap();
    let layout =
        extract::extract(&m, named(&m, "f"), &mut budget::Work::new(budget::LIMIT)).unwrap();
    let encoding = encoding::Encoding::new(
        &layout,
        Representation::Location,
        &mut budget::Work::new(budget::LIMIT),
    )
    .unwrap();
    let mut bdd = bdd::Bdd::new(encoding.physical.bits, budget::LIMIT);
    let mut changes = BTreeMap::new();
    for owner in &layout.owners[..2] {
        let (bit, _) = layout
            .schema
            .field(&format!("i:{owner}:{}", layout.tokens[0]))
            .unwrap();
        changes.insert(bit, (1, 0));
    }
    let (_, invalid) = encoding.equations(changes, &mut bdd).unwrap();
    assert_eq!(
        invalid, 1,
        "unconditional duplicate normal image must fail, not be silently represented"
    );
}

#[test]
fn every_observable_oracle_row_roundtrips_without_losing_other_coordinates() {
    let m = typed(
        "roundtrip",
        include_str!("../../../tests/perf/impl059/correlated.zil"),
    )
    .unwrap();
    let f = named(&m, "correlated");
    let reference = oracle::solve(&m, f, budget::LIMIT).unwrap();
    let mut work = budget::Work::new(budget::LIMIT);
    let layout = extract::extract(&m, f, &mut work).unwrap();
    let encoding = encoding::Encoding::new(&layout, Representation::Location, &mut work).unwrap();
    for rows in reference.relations.values() {
        for row in rows {
            let physical = encoding.encode(row, &mut work).unwrap();
            assert_eq!(&encoding.decode(&physical, &mut work).unwrap(), row);
        }
    }
}

#[test]
fn unknown_new_schema_is_not_accepted_by_the_frozen_model() {
    let mut m = typed("schema", CLOSURE).unwrap();
    m.schema_version = 11;
    let f = named(&m, "f");
    assert!(
        oracle::solve(&m, f, budget::LIMIT)
            .unwrap_err_string()
            .contains("Unsupported")
    );
    assert!(
        symbolic::solve_encoded(
            &m,
            f,
            bdd::Method::Fused,
            budget::LIMIT,
            &mut telemetry::Telemetry::disabled(),
            "owner",
            true,
            Representation::Location
        )
        .unwrap_err_string()
        .contains("Unsupported")
    );
}

#[test]
fn sparse_full_materialization_obeys_the_same_exact_work_limit() {
    let m = typed("work_boundary", CLOSURE).unwrap();
    let f = named(&m, "f");
    let reference = oracle::solve(&m, f, budget::LIMIT).unwrap();
    let run = |limit| -> Result<usize, String> {
        let mut s = symbolic::solve_encoded(
            &m,
            f,
            bdd::Method::Fused,
            limit,
            &mut telemetry::Telemetry::disabled(),
            "owner",
            true,
            Representation::Location,
        )?;
        compare(&mut s, &reference)?;
        for (key, root) in &s.relations {
            if key.starts_with("point:") {
                s.bdd.shape(*root)?;
            }
        }
        let bytes = report::materialize(&mut s, &reference)?;
        s.bdd.work.charge(bytes.len())?;
        Ok(s.bdd.work.used)
    };
    let n = run(budget::LIMIT).unwrap();
    assert_eq!(run(n).unwrap(), n);
    assert!(run(n - 1).unwrap_err().starts_with("ResourceLimit:"));
    println!("067 exact sparse complete budget n={n}");
}

// Keep assertions readable without adding Debug to the independent oracle.
trait ErrorString {
    fn unwrap_err_string(self) -> String;
}
impl<T> ErrorString for Result<T, String> {
    fn unwrap_err_string(self) -> String {
        match self {
            Err(e) => e,
            Ok(_) => panic!("expected error"),
        }
    }
}
