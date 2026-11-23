//! End-to-end dual-backend native execution and SafeReject tests for Wave 3 Batch U6:
//! 1. G5.2: Heterogeneous `OwnedDyn<dyn Trait>` (Copy + owning `Drop`/`ByteBuffer` types
//!    with deterministic drop order), parameterized `OwnedDyn<dyn Transform<T>>` &
//!    `OwnedDyn<dyn Producer<Item = T>>`, and object-safety + trait-impl `SafeReject` checks.
//! 2. G4.5: Compile-time `#{ statements; yield value }` (`MetaBlock` + `StmtKind::Yield`)
//!    with `var`, `while`, `if`/`else`, helper calls, record/tuple results, and `SafeReject`
//!    checks for `yield` outside meta block, `return` inside meta block, mismatched yield
//!    types, missing yield, and non-serializable / owning payloads escaping CT.
//! 3. G9.3: Dual-backend native consumers of all 5 owning stdlib modules:
//!    `std.owning_map`, `std.owning_hash_map`, `std.owning_hash_set`, `std.graph`,
//!    and `std.buffered_io`, plus the existing `owning-map-temporary-lending.zil` fixture.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use zil_base::ResultCategory;
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization, Stage,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zil-u6-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn write_source(dir: &Path, filename: &str, src: &str) -> PathBuf {
    let path = dir.join(filename);
    std::fs::write(&path, src).expect("write source");
    path
}

fn dual_configs() -> Vec<(&'static str, NativeBackend, Optimization)> {
    let mut configs = vec![
        ("clang-o0", NativeBackend::Clang, Optimization::O0),
        ("clang-o2", NativeBackend::Clang, Optimization::O2),
    ];
    if cfg!(feature = "llvm-api") {
        configs.push(("llvmapi-o0", NativeBackend::LlvmApi, Optimization::O0));
        configs.push(("llvmapi-o2", NativeBackend::LlvmApi, Optimization::O2));
    }
    configs
}

fn build_and_run_dual(dir: &Path, entry: &Path) -> (i32, String) {
    let mut expected: Option<(i32, String)> = None;
    for (label, backend, opt) in dual_configs() {
        let exe = dir.join(format!("bin-{label}"));
        let mut session = CompilationSession::new();
        let opts = CompileOptions {
            stop_after: Stage::Build,
            optimization: opt,
            backend,
            output: Some(exe.clone()),
            codegen: CodegenOptions { units: 1, jobs: 1 },
            ..CompileOptions::default()
        };
        let report = session.compile_path(entry, &opts);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "build failed for {} ({label}): {:?}",
            entry.display(),
            report.diagnostics
        );
        assert_eq!(report.verification_runs, 1);

        let out = Command::new(&exe)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .unwrap_or_else(|e| panic!("failed to run {}: {e}", exe.display()));
        assert!(
            out.stderr.is_empty(),
            "unexpected stderr ({label}): {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let code = out.status.code().unwrap_or(-1);
        let stdout = String::from_utf8(out.stdout).expect("utf8 stdout");
        let result = (code, stdout);
        if let Some(prev) = &expected {
            assert_eq!(&result, prev, "result mismatch on {label}");
        } else {
            expected = Some(result);
        }
    }
    expected.expect("at least one backend ran")
}

fn check_rejected(dir: &Path, name: &str, src: &str, expected_code_prefix: &str) {
    let entry = write_source(dir, &format!("{name}.zil"), src);
    let mut session = CompilationSession::new();
    let opts = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let report = session.compile_path(&entry, &opts);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "expected `{name}` to be SafeReject, got {:?} with diagnostics: {:?}",
        report.status,
        report.diagnostics
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code.starts_with(expected_code_prefix)),
        "expected diagnostic starting with `{expected_code_prefix}` for `{name}`, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn u6_heterogeneous_owned_dyn_and_parameterized_dyn_native() {
    let dir = temp_dir("owned-dyn");
    let src = r#"
extern "C" fn putchar(c: i32) -> i32;

trait Scorer {
    fn score(self: &Self, bonus: i64) -> i64;
}

trait Transform<T> {
    fn apply(self: &Self, x: T) -> T;
}

trait Producer {
    type Item;
    fn produce(self: &Self) -> Self.Item;
}

struct CopyScorer {
    base: i64,
}

impl Scorer for CopyScorer {
    fn score(self: &Self, bonus: i64) -> i64 {
        return self.base + bonus
    }
}

resource DropToken {
    tag: i32,
}

impl Drop for DropToken {
    fn drop(self: Self) {
        unsafe { putchar(.tag) }
    }
}

package DropScorer {
    manages token: DropToken,
    weight: i64,
}

impl Scorer for DropScorer {
    fn score(self: &Self, bonus: i64) -> i64 {
        return self.weight * 2 + bonus
    }
}

package BufScorer {
    manages buf: ByteBuffer,
    manages token: DropToken,
}

impl Scorer for BufScorer {
    fn score(self: &Self, bonus: i64) -> i64 {
        return bytes_len(self.buf) + bonus
    }
}

struct AddI64 {
    delta: i64,
}

impl Transform<i64> for AddI64 {
    fn apply(self: &Self, x: i64) -> i64 {
        return x + self.delta
    }
}

struct MulI64 {
    factor: i64,
}

impl Transform<i64> for MulI64 {
    fn apply(self: &Self, x: i64) -> i64 {
        return x * self.factor
    }
}

struct AddI32 {
    delta: i32,
}

impl Transform<i32> for AddI32 {
    fn apply(self: &Self, x: i32) -> i32 {
        return x + self.delta
    }
}

struct ConstProducer {
    val: i64,
}

impl Producer for ConstProducer {
    type Item = i64;
    fn produce(self: &Self) -> i64 {
        return self.val
    }
}

fn eval_scorer(s: &OwnedDyn<dyn Scorer>, bonus: i64) -> i64 {
    return s.score(bonus)
}

fn apply_i64(t: &OwnedDyn<dyn Transform<i64>>, v: i64) -> i64 {
    return t.apply(v)
}

fn apply_i32(t: &OwnedDyn<dyn Transform<i32>>, v: i32) -> i32 {
    return t.apply(v)
}

fn run_producer(p: &OwnedDyn<dyn Producer<Item = i64>>) -> i64 {
    return p.produce()
}

fn make_scorer(kind: i64) -> DynCreateError!OwnedDyn<dyn Scorer> {
    if kind == 0 {
        return owned_dyn::<dyn Scorer>(CopyScorer { base: 10 })
    }
    if kind == 1 {
        return owned_dyn::<dyn Scorer>(DropScorer {
            token: DropToken { tag: 65 },
            weight: 7,
        })
    }
    let buf = bytes_new()
    bytes_reserve(buf, 4)
    bytes_push(buf, u8(1))
    bytes_push(buf, u8(2))
    bytes_push(buf, u8(3))
    return owned_dyn::<dyn Scorer>(BufScorer {
        buf: give buf,
        token: DropToken { tag: 66 },
    })
}

fn run_all() -> DynCreateError!i32 {
    let s0 = make_scorer(0)?
    let s1 = make_scorer(1)?
    let s2 = make_scorer(2)?

    // 10+5 = 15, 7*2+5 = 19, 3+5 = 8 => 42
    let total = eval_scorer(&s0, 5) + eval_scorer(&s1, 5) + eval_scorer(&s2, 5)
    if total != 42 { return 10 }

    // Parameterized dyn Transform<i64> with two distinct concrete impls
    let t1 = owned_dyn::<dyn Transform<i64>>(AddI64 { delta: 7 })?
    let t2 = owned_dyn::<dyn Transform<i64>>(MulI64 { factor: 3 })?
    if apply_i64(&t2, apply_i64(&t1, 5)) != 36 { return 13 }

    // Distinct parameterized dyn Transform<i32> in the same module
    let t3 = owned_dyn::<dyn Transform<i32>>(AddI32 { delta: 9 })?
    if apply_i32(&t3, 33) != 42 { return 15 }

    // Associated-type bound dyn Producer<Item = i64>
    let p = owned_dyn::<dyn Producer<Item = i64>>(ConstProducer { val: 99 })?
    if run_producer(&p) != 99 { return 17 }

    // Reverse declaration order drops: s2 ('B'), then s1 ('A').
    return 0
}

fn main() -> i32 {
    match run_all() {
        .ok(code) => { return code }
        .err(_) => { return 99 }
    }
}
"#;
    let entry = write_source(&dir, "main.zil", src);
    let (code, stdout) = build_and_run_dual(&dir, &entry);
    assert_eq!(code, 0);
    assert_eq!(
        stdout, "BA",
        "expected deterministic LIFO OwnedDyn drop order"
    );
}

#[test]
fn u6_owned_dyn_object_safety_and_impl_rejections() {
    let dir = temp_dir("owned-dyn-reject");

    // 1. Concrete value does not implement trait.
    check_rejected(
        &dir,
        "non_impl",
        r#"
trait Ping {
    fn ping(self: &Self) -> i64;
}
struct Unrelated { x: i64 }
fn main() -> i32 {
    let _d = owned_dyn::<dyn Ping>(Unrelated { x: 1 })
    return 0
}
"#,
        "E-GENERIC-ARGUMENT",
    );

    // 2. Parameterized trait argument mismatch (`Transform<i32>` vs `dyn Transform<i64>`).
    check_rejected(
        &dir,
        "param_mismatch",
        r#"
trait Transform<T> {
    fn apply(self: &Self, x: T) -> T;
}
struct OnlyI32 { d: i32 }
impl Transform<i32> for OnlyI32 {
    fn apply(self: &Self, x: i32) -> i32 { return x + self.d }
}
fn main() -> i32 {
    let _d = owned_dyn::<dyn Transform<i64>>(OnlyI32 { d: 1 })
    return 0
}
"#,
        "E-GENERIC-ARGUMENT",
    );

    // 3. Static method with no `self` receiver is not object-safe.
    check_rejected(
        &dir,
        "static_method",
        r#"
trait HasStatic {
    fn make() -> i64;
}
struct S { x: i64 }
impl HasStatic for S {
    fn make() -> i64 { return 1 }
}
fn main() -> i32 {
    let _d = owned_dyn::<dyn HasStatic>(S { x: 1 })
    return 0
}
"#,
        "E-GENERIC-ARGUMENT",
    );

    // 4. Generic method is not object-safe.
    check_rejected(
        &dir,
        "generic_method",
        r#"
trait HasGeneric {
    fn map<U>(self: &Self, x: U) -> i64;
}
struct S { x: i64 }
impl HasGeneric for S {
    fn map<U>(self: &Self, _x: U) -> i64 { return self.x }
}
fn main() -> i32 {
    let _d = owned_dyn::<dyn HasGeneric>(S { x: 1 })
    return 0
}
"#,
        "E-GENERIC-ARGUMENT",
    );

    // 5. `Self` by value in non-receiver parameter is not object-safe.
    check_rejected(
        &dir,
        "self_param_arg",
        r#"
trait Combine {
    fn combine(self: &Self, other: Self) -> i64;
}
struct S { x: i64 }
impl Combine for S {
    fn combine(self: &Self, other: Self) -> i64 { return self.x + other.x }
}
fn main() -> i32 {
    let _d = owned_dyn::<dyn Combine>(S { x: 1 })
    return 0
}
"#,
        "E-GENERIC-ARGUMENT",
    );

    // 6. `Self` by value in return type is not object-safe.
    check_rejected(
        &dir,
        "self_return",
        r#"
trait Cloneable {
    fn clone_me(self: &Self) -> Self;
}
struct S { x: i64 }
impl Cloneable for S {
    fn clone_me(self: &Self) -> Self { return S { x: self.x } }
}
fn main() -> i32 {
    let _d = owned_dyn::<dyn Cloneable>(S { x: 1 })
    return 0
}
"#,
        "E-GENERIC-ARGUMENT",
    );
}

#[test]
fn u6_ct_meta_blocks_and_yield_native() {
    let dir = temp_dir("ct-meta-block");
    let src = r#"
struct Pair {
    sum: i64,
    prod: i64,
}

fn step_add(acc: i64, n: i64) -> i64 {
    return acc + n
}

fn main() -> i32 {
    // 1. var + while loop + helper call + yield scalar
    let tri: i64 = #{
        var acc: i64 = 0
        var i: i64 = 1
        while i <= 10 {
            acc = step_add(acc, i)
            i += 1
        }
        yield acc
    }
    if tri != 55 { return 1 }

    // 2. conditional branches yielding a record
    let pair: Pair = #{
        var s: i64 = 0
        var p: i64 = 1
        var k: i64 = 1
        while k <= 4 {
            s += k
            p *= k
            k += 1
        }
        if s == 10 {
            yield Pair { sum: s, prod: p }
        } else {
            yield Pair { sum: 0, prod: 0 }
        }
    }
    if pair.sum != 10 || pair.prod != 24 { return 2 }

    // 3. yielding a tuple without outer type annotation
    let tup = #{
        let a: i64 = 17
        let b: i64 = 25
        yield (a, b)
    }
    if tup.0 + tup.1 != 42 { return 3 }

    // 4. unit CT block (no yield)
    let _u = #{
        var tmp: i64 = 0
        tmp += 1
    }

    return 0
}
"#;
    let entry = write_source(&dir, "main.zil", src);
    let (code, _stdout) = build_and_run_dual(&dir, &entry);
    assert_eq!(code, 0);
}

#[test]
fn u6_ct_meta_block_rejections() {
    let dir = temp_dir("ct-meta-block-reject");

    // 1. `yield` outside `#{ ... }` meta block.
    check_rejected(
        &dir,
        "yield_outside",
        r#"
fn main() -> i32 {
    yield 42
    return 0
}
"#,
        "E-LOOP-CONTROL",
    );

    // 2. `return` inside `#{ ... }` meta block.
    check_rejected(
        &dir,
        "return_inside_meta",
        r#"
fn main() -> i32 {
    let _x: i64 = #{
        return 1
    }
    return 0
}
"#,
        "E-LOOP-CONTROL",
    );

    // 3. Conflicting yield types across branches.
    check_rejected(
        &dir,
        "yield_type_conflict",
        r#"
fn main() -> i32 {
    let _x = #{
        if true {
            yield 10
        } else {
            yield true
        }
    }
    return 0
}
"#,
        "E-TYPE-MISMATCH",
    );

    // 4. Missing yield when non-unit type is expected.
    check_rejected(
        &dir,
        "missing_yield_expected",
        r#"
fn main() -> i32 {
    let _x: i64 = #{
        var a: i64 = 1
        a += 1
    }
    return 0
}
"#,
        "E-MISSING-RETURN",
    );

    // 5. Partial-path yield (falls through without yielding).
    check_rejected(
        &dir,
        "partial_yield",
        r#"
fn main() -> i32 {
    let _x = #{
        if true {
            yield 10
        }
    }
    return 0
}
"#,
        "E-MISSING-RETURN",
    );

    // 6. Owning resource payload escaping from CT to runtime.
    check_rejected(
        &dir,
        "owning_escape",
        r#"
resource Token { id: i64 }
fn main() -> i32 {
    let _t = #{
        yield Token { id: 7 }
    }
    return 0
}
"#,
        "E-RESOURCE-TYPE",
    );
}

#[test]
fn u6_owning_map_temporary_lending_native() {
    let dir = temp_dir("owning-map-lending");
    let src = r#"
import std.owning_map
import std.owning_entries
extern "C" fn putchar(value: i32) -> i32;

resource Ticket { code: i32 }
impl Drop for Ticket {
    fn drop(self: Self) {
        unsafe { putchar(.code) }
    }
}

package Key { manages ticket: Ticket; id: i64 }
package Value { manages ticket: Ticket; number: i64 }

impl owning_map.KeyEq for Key {
    fn same(self: &Self, other: &Self) -> bool {
        return self.id == other.id
    }
}

fn inspect<life s>(found: Option<&'s Value>) -> i64 {
    match found {
        Option.Some(value) => { return value.number }
        Option.None() => { return -1 }
    }
}

fn main() -> i32 {
    var map = owning_map.new::<Key, Value>()
    let first = owning_entries.Entry::<Key, Value> {
        key: Key { ticket: Ticket { code: 65 }, id: 7 },
        value: Value { ticket: Ticket { code: 66 }, number: 42 },
    }
    match owning_map.insert_or_replace::<Key, Value>(&mut map, first) {
        .ok(_) => {}
        .err(_) => { return 11 }
    }
    let query = Key { ticket: Ticket { code: 67 }, id: 7 }
    let found = owning_map.get::<Key, Value>(&map, &query)
    let observed = inspect(found)
    if observed != 42 { return 12 }
    let second = owning_entries.Entry::<Key, Value> {
        key: Key { ticket: Ticket { code: 68 }, id: 8 },
        value: Value { ticket: Ticket { code: 69 }, number: 9 },
    }
    match owning_map.insert_or_replace::<Key, Value>(&mut map, second) {
        .ok(_) => {}
        .err(_) => { return 13 }
    }
    let moved = map
    if owning_map.len::<Key, Value>(&moved) != 2 { return 14 }
    return 0
}
"#;
    let entry = write_source(&dir, "main.zil", src);
    let (code, stdout) = build_and_run_dual(&dir, &entry);
    assert_eq!(code, 0);
    // All 5 Tickets ('A', 'B', 'C', 'D', 'E') must be dropped exactly once.
    assert_eq!(
        stdout.len(),
        5,
        "expected all 5 Ticket drops, got {stdout:?}"
    );
    for ch in ['A', 'B', 'C', 'D', 'E'] {
        assert!(stdout.contains(ch), "missing drop for {ch} in {stdout:?}");
    }
}

#[test]
fn u6_owning_collections_native() {
    // Dual-backend consumer exercising std.owning_map, std.owning_hash_map,
    // std.owning_hash_set, and std.graph with real owning payloads and Drop tracking.
    let col_dir = temp_dir("owning-collections");
    let col_src = r#"
import std.owning_entries
import std.owning_map
import std.owning_hash_map
import std.owning_hash_set
import std.graph

resource DropTag { id: i64 }
impl Drop for DropTag { fn drop(self: Self) {} }

package OwnKey {
    manages tag: DropTag,
    id: i64,
}

package OwnVal {
    manages tag: DropTag,
    score: i64,
}

impl owning_map.KeyEq for OwnKey {
    fn same(self: &Self, other: &Self) -> bool {
        return self.id == other.id
    }
}

impl owning_hash_map.KeyHash for OwnKey {
    fn hash(self: &Self) -> i64 {
        return self.id * 31 + 7
    }
}

fn test_owning_map() -> i32 {
    var m = owning_map.new::<OwnKey, OwnVal>()
    let e1 = owning_entries.Entry::<OwnKey, OwnVal> {
        key: OwnKey { tag: DropTag { id: 1 }, id: 10 },
        value: OwnVal { tag: DropTag { id: 2 }, score: 100 },
    }
    let e2 = owning_entries.Entry::<OwnKey, OwnVal> {
        key: OwnKey { tag: DropTag { id: 3 }, id: 20 },
        value: OwnVal { tag: DropTag { id: 4 }, score: 200 },
    }
    match owning_map.insert_or_replace::<OwnKey, OwnVal>(&mut m, e1) {
        .ok(replaced) => { if replaced { return 1 } },
        .err(_) => { return 2 },
    }
    match owning_map.insert_or_replace::<OwnKey, OwnVal>(&mut m, e2) {
        .ok(replaced) => { if replaced { return 3 } },
        .err(_) => { return 4 },
    }
    if owning_map.len::<OwnKey, OwnVal>(&m) != 2 { return 5 }

    let q10 = OwnKey { tag: DropTag { id: 5 }, id: 10 }
    if !owning_map.contains::<OwnKey, OwnVal>(&m, &q10) { return 6 }
    var got10: i64 = -1
    match owning_map.get::<OwnKey, OwnVal>(&m, &q10) {
        Option.Some(v) => { got10 = v.score }
        Option.None() => {}
    }
    if got10 != 100 { return 7 }

    // Replace existing key 10
    var did_replace = false
    let rep = owning_entries.Entry::<OwnKey, OwnVal> {
        key: OwnKey { tag: DropTag { id: 6 }, id: 10 },
        value: OwnVal { tag: DropTag { id: 7 }, score: 150 },
    }
    let old = owning_map.replace_existing::<OwnKey, OwnVal>(&mut m, rep, &mut did_replace)
    if !did_replace || old.value.score != 100 { return 8 }

    // Take key 20
    let q20 = OwnKey { tag: DropTag { id: 8 }, id: 20 }
    match owning_map.take::<OwnKey, OwnVal>(&mut m, &q20) {
        .ok(taken) => { if taken.value.score != 200 { return 9 } }
        .err(_) => { return 10 }
    }
    if owning_map.len::<OwnKey, OwnVal>(&m) != 1 { return 11 }
    return 0
}

fn test_owning_hash_map() -> i32 {
    var hm = owning_hash_map.new::<OwnKey, OwnVal>()
    match owning_hash_map.reserve::<OwnKey, OwnVal>(&mut hm, 8) {
        .ok(_) => {}
        .err(_) => { return 20 }
    }
    if owning_hash_map.capacity::<OwnKey, OwnVal>(&hm) < 8 { return 21 }

    var i: i64 = 1
    while i <= 5 {
        let e = owning_entries.Entry::<OwnKey, OwnVal> {
            key: OwnKey { tag: DropTag { id: i }, id: i * 10 },
            value: OwnVal { tag: DropTag { id: i + 100 }, score: i * 11 },
        }
        match owning_hash_map.insert::<OwnKey, OwnVal>(&mut hm, e) {
            .ok(replaced) => { if replaced { return 22 } }
            .err(_) => { return 23 }
        }
        i += 1
    }
    if owning_hash_map.len::<OwnKey, OwnVal>(&hm) != 5 { return 24 }

    let q30 = OwnKey { tag: DropTag { id: 99 }, id: 30 }
    var s30: i64 = -1
    match owning_hash_map.get::<OwnKey, OwnVal>(&hm, &q30) {
        Option.Some(v) => { s30 = v.score }
        Option.None() => {}
    }
    if s30 != 33 { return 25 }

    match owning_hash_map.take::<OwnKey, OwnVal>(&mut hm, &q30) {
        .ok(taken) => { if taken.value.score != 33 { return 26 } }
        .err(_) => { return 27 }
    }
    if owning_hash_map.contains::<OwnKey, OwnVal>(&hm, &q30) { return 28 }
    if owning_hash_map.clear::<OwnKey, OwnVal>(&mut hm) != 4 { return 29 }
    if owning_hash_map.len::<OwnKey, OwnVal>(&hm) != 0 { return 30 }
    return 0
}

fn test_owning_hash_set() -> i32 {
    var hs = owning_hash_set.new::<OwnKey>()
    match owning_hash_set.reserve::<OwnKey>(&mut hs, 4) {
        .ok(_) => {}
        .err(_) => { return 40 }
    }
    match owning_hash_set.insert::<OwnKey>(&mut hs, OwnKey { tag: DropTag { id: 1 }, id: 7 }) {
        .ok(inserted) => { if !inserted { return 41 } }
        .err(_) => { return 42 }
    }
    // Duplicate key returns false (replaced)
    match owning_hash_set.insert::<OwnKey>(&mut hs, OwnKey { tag: DropTag { id: 2 }, id: 7 }) {
        .ok(inserted) => { if inserted { return 43 } }
        .err(_) => { return 44 }
    }
    match owning_hash_set.insert::<OwnKey>(&mut hs, OwnKey { tag: DropTag { id: 3 }, id: 9 }) {
        .ok(inserted) => { if !inserted { return 45 } }
        .err(_) => { return 46 }
    }
    if owning_hash_set.len::<OwnKey>(&hs) != 2 { return 47 }

    let q7 = OwnKey { tag: DropTag { id: 4 }, id: 7 }
    if !owning_hash_set.contains::<OwnKey>(&hs, &q7) { return 48 }
    match owning_hash_set.take::<OwnKey>(&mut hs, &q7) {
        .ok(k) => { if k.id != 7 { return 49 } }
        .err(_) => { return 50 }
    }
    if owning_hash_set.len::<OwnKey>(&hs) != 1 { return 51 }
    return 0
}

fn test_graph_inner() -> graph.Error!i32 {
    var g = graph.with_capacity(8, 8, 8)?
    let p1 = bytes_new()
    bytes_reserve(p1, 2)
    bytes_push(p1, u8(42))
    bytes_push(p1, u8(99))
    var k1 = graph.NodeKey { ticket: -1 }
    match graph.add_with_payload(&mut g, 100, give p1) {
        .ok(k) => { k1 = k }
        .err(_) => { return 61 }
    }
    let k2 = graph.add(&mut g, 200)?
    if graph.node_count(&g) != 2 { return 63 }
    if !graph.contains(&g, k1) { return 64 }
    var b0: i64 = -1
    match graph.payload_byte(&g, k1, 0) {
        Option.Some(b) => { b0 = i64(b) }
        Option.None() => {}
    }
    if b0 != 42 { return 65 }

    graph.link(&mut g, k1, k2)?
    if graph.edge_count(&g) != 1 { return 67 }
    if !graph.remove(&mut g, k1) { return 68 }
    if graph.contains(&g, k1) { return 69 }
    let remaining = graph.collect(&mut g)?
    if remaining != 1 { return 70 }
    var snap: i64 = -1
    match graph.get(&g, k2) {
        Option.Some(s) => { snap = s.value }
        Option.None() => {}
    }
    if snap != 200 { return 72 }
    return 0
}

fn test_graph() -> i32 {
    match test_graph_inner() {
        .ok(code) => { return code }
        .err(_) => { return 79 }
    }
}

fn main() -> i32 {
    let r1 = test_owning_map()
    if r1 != 0 { return r1 }
    let r2 = test_owning_hash_map()
    if r2 != 0 { return r2 }
    let r3 = test_owning_hash_set()
    if r3 != 0 { return r3 }
    let r4 = test_graph()
    if r4 != 0 { return r4 }
    return 0
}
"#;
    let entry = write_source(&col_dir, "main.zil", col_src);
    let (code, _stdout) = build_and_run_dual(&col_dir, &entry);
    assert_eq!(code, 0);
}

#[test]
fn u6_buffered_io_dual_backend_native() {
    let dir = temp_dir("buffered-io");
    let data_file = dir.join("roundtrip.bin");
    let path_str = data_file.to_str().expect("utf8 temp path");
    let src = format!(
        r#"
import std.io
import std.buffered_io

fn roundtrip() -> io.Error!i32 {{
    let path = "{path_str}"
    let wf = io.create_write(path, i32(420))?
    var w = buffered_io.writer(give wf)
    let msg: [u8; 5] = [u8(72), u8(101), u8(108), u8(108), u8(111)]
    let wrote = buffered_io.write(&mut w, &msg[0..5])?
    if wrote != 5 {{ return 3 }}
    buffered_io.write_byte(&mut w, u8(33))?
    if buffered_io.pending(&w) != 6 {{ return 5 }}
    match buffered_io.finish(w) {{
        .ok(f) => {{
            let _closed = buffered_io.into_file(f)
        }}
        .err(_) => {{ return 6 }}
    }}

    let rf = io.open_read(path)?
    var r = buffered_io.reader(give rf)
    var out: [u8; 6] = [u8(0), u8(0), u8(0), u8(0), u8(0), u8(0)]
    let nread = buffered_io.read(&mut r, &mut out[0..6])?
    if nread != 6 {{ return 9 }}
    if out[0] != u8(72) || out[4] != u8(111) || out[5] != u8(33) {{ return 10 }}
    let eof_opt = buffered_io.read_byte(&mut r)?
    match eof_opt {{
        Option.None() => {{}}
        Option.Some(_) => {{ return 11 }}
    }}
    return 0
}}

fn main() -> i32 {{
    match roundtrip() {{
        .ok(code) => {{ return code }}
        .err(_) => {{ return 99 }}
    }}
}}
"#
    );
    let entry = write_source(&dir, "main.zil", &src);
    let (code, _stdout) = build_and_run_dual(&dir, &entry);
    assert_eq!(code, 0);
}
