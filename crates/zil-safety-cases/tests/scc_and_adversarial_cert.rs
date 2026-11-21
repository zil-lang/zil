//! P0-B: Mutual recursion SCC inductive contract verification and adversarial
//! certificate forgery tests.

use zil_check::{CheckBudget, call_graph_sccs, check_module};
use zil_ir::body::{BlockInvariant, Certificate, FnId, FunctionCertificate, LoanOrigin};
use zil_solve::{Budget as SolveBudget, solve_module};

#[test]
fn mutual_recursion_scc_matching_region_contract_is_admitted() {
    let src = r#"
fn even<life a, life b>(n: i32, x: &'a i32, y: &'b i32) -> &'a i32 {
    if n <= 0 {
        return &*x
    }
    return odd(n - 1, x, y)
}

fn odd<life a, life b>(n: i32, x: &'a i32, y: &'b i32) -> &'a i32 {
    if n <= 0 {
        return &*x
    }
    return even(n - 1, x, y)
}

fn main() -> i32 {
    let a: i32 = 10
    let b: i32 = 20
    return *even(3, &a, &b)
}
"#;
    let lowered = zil_typeck::lower_source("mutual_scc_ok.zil", src).expect("lowers");
    let sccs = call_graph_sccs(&lowered.module);
    assert!(
        sccs.iter().any(|s| s.recursive && s.members.len() == 2),
        "expected a 2-function mutually recursive SCC, got {sccs:?}"
    );
    let cert = solve_module(&lowered.module, SolveBudget { steps: 100_000 }).expect("solves");
    check_module(lowered.module, &cert, CheckBudget { steps: 100_000 })
        .expect("mutually recursive SCC passes checker");
}

#[test]
fn mutual_recursion_scc_region_escape_is_rejected_even_with_forged_certificate() {
    let src = r#"
fn even<life a, life b>(n: i32, x: &'a i32, y: &'b i32) -> &'a i32 {
    if n <= 0 {
        return &*x
    }
    return odd(n - 1, x, y)
}

fn odd<life a, life b>(n: i32, x: &'a i32, y: &'b i32) -> &'a i32 {
    if n <= 0 {
        return &*y
    }
    return even(n - 1, x, y)
}

fn main() -> i32 {
    let a: i32 = 10
    let b: i32 = 20
    return *even(3, &a, &b)
}
"#;
    let lowered = zil_typeck::lower_source("mutual_scc_bad.zil", src).expect("lowers");
    // 1. Solver must reject with RegionEscape.
    let err = solve_module(&lowered.module, SolveBudget { steps: 100_000 })
        .expect_err("mutual SCC returning wrong region must be rejected by solver");
    assert!(
        format!("{err:?}").contains("RegionEscape"),
        "expected RegionEscape, got {err:?}"
    );

    // 2. Even if an attacker forges a certificate claiming empty/External loans
    // for every block in every function, zil-check MUST still reject.
    let forged = Certificate {
        functions: (0..lowered.module.functions.len())
            .map(|i| {
                let f = &lowered.module.functions[i];
                FunctionCertificate {
                    function: FnId::from_index(i),
                    invariants: (0..f.body.blocks.len())
                        .map(|bi| BlockInvariant {
                            block: zil_ir::body::BlockId::from_index(bi),
                            init: Vec::new(),
                            variants: Vec::new(),
                            tags: Vec::new(),
                            flags: Vec::new(),
                            loans: Vec::new(),
                        })
                        .collect(),
                }
            })
            .collect(),
    };
    assert!(
        check_module(
            lowered.module.clone(),
            &forged,
            CheckBudget { steps: 100_000 }
        )
        .is_err(),
        "forged empty certificate must not bypass checker"
    );

    // 3. Also test forging a certificate that swaps Region(1) to External on an
    // honest SCC module's stolen certificate shape: checker must reject.
    let mut forged2 = forged;
    for fc in &mut forged2.functions {
        for inv in &mut fc.invariants {
            inv.loans.push(zil_ir::body::LoanClaim {
                holder: zil_ir::body::Local(0),
                origin: LoanOrigin::External,
            });
        }
    }
    assert!(
        check_module(lowered.module, &forged2, CheckBudget { steps: 100_000 }).is_err(),
        "forged External loan certificate must not bypass checker"
    );
}

#[test]
fn undeclared_region_parameter_in_signature_is_rejected_by_checker() {
    let src = r#"
fn id<life a>(x: &'a i32) -> &'a i32 {
    return &*x
}
"#;
    let mut lowered = zil_typeck::lower_source("bad_region_sig.zil", src).expect("lowers");
    let cert = solve_module(&lowered.module, SolveBudget { steps: 100_000 }).expect("solves");
    // Maliciously reduce declared region_params to 0 while types still carry Region::Param(0).
    lowered.module.functions[0].sig.region_params = 0;
    let err = check_module(lowered.module, &cert, CheckBudget { steps: 100_000 })
        .expect_err("undeclared region parameter in signature must be rejected");
    assert!(
        format!("{err:?}").contains("undeclared region parameter"),
        "expected undeclared region parameter error, got {err:?}"
    );
}
