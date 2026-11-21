//! P0 safety regression tests: concurrent `Store` structural mutation
//! (`store_reserve`/`store_push`/`store_pop`) invalidating an active element
//! borrow across `thread_scope`, closures, helpers, and direct calls.
//!
//! Covers:
//! - Exact Source negatives (`thread-capture-invalidates-store.zil` and
//!   `thread-helper-invalidates-store.zil`)
//! - Same-function `store_at` + `store_reserve` + use negative
//! - Independent `zil-check` raw + wire forged-certificate rejection
//! - Legal non-conflicting controls (disjoint stores, read-only `store_len`,
//!   NLL dead borrow before `store_reserve`, unrelated reference passed to helper)

use std::path::PathBuf;
use zil_check::{CheckBudget, check_module};
use zil_ir::body::{
    Certificate, FunctionCertificate, decode_certificate, decode_module, encode_certificate,
    encode_module,
};
use zil_solve::{Budget as SolveBudget, solve_module};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn assert_rejected_source_raw_and_wire(name: &str, src: &str) {
    let lowered = zil_typeck::lower_source(name, src).unwrap_or_else(|e| {
        panic!("{name}: expected lowering to succeed before safety rejection, got {e:?}")
    });

    // 1. Solver must reject with a safety violation.
    let solve_err = solve_module(&lowered.module, SolveBudget { steps: 200_000 })
        .expect_err(&format!("{name}: solver must reject store invalidation"));
    assert!(
        format!("{solve_err:?}").contains("LoanConflict"),
        "{name}: expected LoanConflict from solver, got {solve_err:?}"
    );

    // 2. Independent checker must reject raw module with forged certificate.
    let forged = Certificate {
        functions: lowered
            .module
            .functions
            .iter()
            .enumerate()
            .map(|(i, _)| FunctionCertificate {
                function: zil_ir::body::FnId::from_index(i),
                invariants: Vec::new(),
            })
            .collect(),
    };
    check_module(
        lowered.module.clone(),
        &forged,
        CheckBudget { steps: 200_000 },
    )
    .expect_err(&format!(
        "{name}: independent checker must reject forged raw cert"
    ));

    // 3. Independent checker must reject wire-decoded module and certificate.
    let mod_bytes = encode_module(&lowered.module);
    let cert_bytes = encode_certificate(&forged);
    let wire_mod = decode_module(&mod_bytes).expect("wire module decodes");
    let wire_cert = decode_certificate(&cert_bytes).expect("wire cert decodes");
    check_module(wire_mod, &wire_cert, CheckBudget { steps: 200_000 }).expect_err(&format!(
        "{name}: independent checker must reject wire module + forged cert"
    ));
}

fn assert_admitted(name: &str, src: &str) {
    let lowered = zil_typeck::lower_source(name, src)
        .unwrap_or_else(|e| panic!("{name}: expected lowering to succeed, got {e:?}"));
    let cert = solve_module(&lowered.module, SolveBudget { steps: 200_000 })
        .unwrap_or_else(|e| panic!("{name}: expected solver to admit legal program, got {e:?}"));
    check_module(lowered.module, &cert, CheckBudget { steps: 200_000 }).unwrap_or_else(|e| {
        panic!("{name}: expected independent checker to admit legal program, got {e:?}")
    });
}

#[test]
fn thread_capture_invalidates_store_rejected_source_raw_wire() {
    let path = repo_root()
        .join("tests/language-readiness/thread-life-review/thread-capture-invalidates-store.zil");
    let src = std::fs::read_to_string(&path).expect("read fixture");
    assert_rejected_source_raw_and_wire("thread-capture-invalidates-store.zil", &src);
}

#[test]
fn thread_helper_invalidates_store_rejected_source_raw_wire() {
    let path = repo_root()
        .join("tests/language-readiness/thread-life-review/thread-helper-invalidates-store.zil");
    let src = std::fs::read_to_string(&path).expect("read fixture");
    assert_rejected_source_raw_and_wire("thread-helper-invalidates-store.zil", &src);
}

#[test]
fn direct_same_function_store_reserve_while_element_borrowed_rejected() {
    let src = r#"
resource R { code: i32 }
package P { manages r: R; n: i32 }
fn empty() -> Store<P> contract { result(create) } { return store_new() }
fn main() -> i32 {
    let s = empty()
    store_reserve(s, 1)
    store_push(s, P { r: R { code: 1 }, n: 7 })
    let item = store_at(s, 0)
    let p = &item.n
    store_reserve(s, 4)
    return *p
}
"#;
    assert_rejected_source_raw_and_wire("direct-store-reserve-invalidation.zil", src);
}

#[test]
fn legal_non_conflicting_thread_and_store_controls_admitted() {
    // 1. Disjoint stores: worker reads from s1 while parent grows s2.
    let disjoint_stores = r#"
resource R { code: i32 }
package P { manages r: R; n: i32 }
fn empty() -> Store<P> contract { result(create) } { return store_new() }
fn read(p: &i32) -> i32 { return *p }
fn main() -> i32 {
    let s1 = empty()
    let s2 = empty()
    store_reserve(s1, 1)
    store_push(s1, P { r: R { code: 1 }, n: 7 })
    let item = store_at(s1, 0)
    let p = &item.n
    let body = () -> i32 => { store_reserve(s2, 4); return 0 }
    let (created, child, parent) = thread_scope(read, p, body)
    return child + parent
}
"#;
    assert_admitted("disjoint-stores-thread-scope.zil", disjoint_stores);

    // 2. Read-only query (store_len) while worker reads element from same store.
    let readonly_query = r#"
resource R { code: i32 }
package P { manages r: R; n: i32 }
fn empty() -> Store<P> contract { result(create) } { return store_new() }
fn read(p: &i32) -> i32 { return *p }
fn main() -> i32 {
    let s = empty()
    store_reserve(s, 1)
    store_push(s, P { r: R { code: 1 }, n: 7 })
    let item = store_at(s, 0)
    let p = &item.n
    let body = () -> i32 => { let n = store_len(s); if n > 0 { return 1 } return 0 }
    let (created, child, parent) = thread_scope(read, p, body)
    return child + parent
}
"#;
    assert_admitted("readonly-store-len-thread-scope.zil", readonly_query);

    // 3. NLL: element borrow ends before store_reserve.
    let nll_sequential = r#"
resource R { code: i32 }
package P { manages r: R; n: i32 }
fn empty() -> Store<P> contract { result(create) } { return store_new() }
fn main() -> i32 {
    let s = empty()
    store_reserve(s, 1)
    store_push(s, P { r: R { code: 1 }, n: 7 })
    let item = store_at(s, 0)
    let v = item.n
    store_reserve(s, 4)
    return v
}
"#;
    assert_admitted("nll-sequential-store-reserve.zil", nll_sequential);

    // 4. Helper receives unrelated &i32 alongside mutated Store<P>.
    let unrelated_ref_helper = r#"
resource R { code: i32 }
package P { manages r: R; n: i32 }
fn empty() -> Store<P> contract { result(create) } { return store_new() }
fn read(p: &i32) -> i32 { return *p }
fn run(p: &i32, s: Store<P>) -> i32 {
    let body = () -> i32 => { store_reserve(s, 4); return 0 }
    let (created, child, parent) = thread_scope(read, p, body)
    return child + parent
}
fn main() -> i32 {
    let s = empty()
    let x: i32 = 42
    return run(&x, s)
}
"#;
    assert_admitted(
        "unrelated-ref-helper-thread-scope.zil",
        unrelated_ref_helper,
    );
}
