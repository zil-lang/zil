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

#[test]
fn u6_owned_dyn_by_value_self_zero_leak_and_drop_native() {
    let dir = temp_dir("owned-dyn-byval");
    let src = r#"
extern "C" fn putchar(c: i32) -> i32;

trait Consumer {
    fn consume(self: Self) -> i64;
}

struct CopyItem {
    val: i64,
}

impl Consumer for CopyItem {
    fn consume(self: Self) -> i64 {
        return self.val + 10
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

package OwnedItem {
    manages token: DropToken,
    manages buf: ByteBuffer,
}

impl Consumer for OwnedItem {
    fn consume(self: Self) -> i64 {
        return bytes_len(self.buf) + 100
    }
}

struct EmptyItem {}

impl Consumer for EmptyItem {
    fn consume(self: Self) -> i64 {
        return 7
    }
}

fn run_consumer(c: OwnedDyn<dyn Consumer>) -> i64 {
    return c.consume()
}

fn run_all() -> DynCreateError!i32 {
    let b = bytes_new()
    bytes_reserve(b, 4)
    bytes_push(b, u8(65))
    bytes_push(b, u8(66))

    let d1 = owned_dyn::<dyn Consumer>(CopyItem { val: 32 })?
    let d2 = owned_dyn::<dyn Consumer>(OwnedItem {
        token: DropToken { tag: 68 },
        buf: give b,
    })?
    let d3 = owned_dyn::<dyn Consumer>(EmptyItem {})?

    let r1 = run_consumer(d1)
    let r2 = run_consumer(d2)
    let r3 = run_consumer(d3)
    if r1 != 42 { return 11 }
    if r2 != 102 { return 12 }
    if r3 != 7 { return 13 }
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

    // 1. Dual-backend (Clang O0/O2 + LLVM C-API O0/O2) functional + Drop-once check.
    // OwnedItem has DropToken tag 68 ('D'), so Drop must run exactly once ("D").
    let (code, stdout) = build_and_run_dual(&dir, &entry);
    assert_eq!(code, 0);
    assert_eq!(
        stdout, "D",
        "OwnedItem Drop must execute exactly once on by-value OwnedDyn consumption"
    );

    // 2. Allocator-wrap zero-leak & double-free verification (-Wl,--wrap=calloc -Wl,--wrap=free)
    // across Clang -O0 and -O2: every OwnedDyn box (Copy, Owning, ZST) must be freed
    // with zero live allocations and zero double-frees at process exit.
    let probe_c = dir.join("alloc_probe.c");
    std::fs::write(
        &probe_c,
        r#"
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>

void *__real_calloc(size_t count, size_t size);
void __real_free(void *ptr);

static void *live_ptrs[256];
static void *freed_ptrs[256];
static int calloc_count = 0;
static int free_count = 0;
static int live_count = 0;
static int double_free = 0;

void *__wrap_calloc(size_t count, size_t size) {
    void *p = __real_calloc(count, size);
    if (p) {
        /* Clear from freed_ptrs if allocator reuses a previously freed address. */
        for (int i = 0; i < 256; i++) {
            if (freed_ptrs[i] == p) {
                freed_ptrs[i] = NULL;
                break;
            }
        }
        calloc_count++;
        live_count++;
        for (int i = 0; i < 256; i++) {
            if (!live_ptrs[i]) {
                live_ptrs[i] = p;
                break;
            }
        }
    }
    return p;
}

void __wrap_free(void *ptr) {
    if (!ptr) {
        __real_free(ptr);
        return;
    }
    int found = 0;
    for (int i = 0; i < 256; i++) {
        if (live_ptrs[i] == ptr) {
            live_ptrs[i] = NULL;
            found = 1;
            break;
        }
    }
    if (found) {
        free_count++;
        live_count--;
        for (int i = 0; i < 256; i++) {
            if (!freed_ptrs[i]) {
                freed_ptrs[i] = ptr;
                break;
            }
        }
        __real_free(ptr);
        return;
    }
    for (int i = 0; i < 256; i++) {
        if (freed_ptrs[i] == ptr) {
            double_free++;
            fprintf(stderr, "ALLOC_STATS calloc=%d free=%d live=%d double_free=%d\n",
                    calloc_count, free_count, live_count, double_free);
            _Exit(98);
        }
    }
    __real_free(ptr);
}

__attribute__((destructor))
static void report_allocs(void) {
    fprintf(stderr, "ALLOC_STATS calloc=%d free=%d live=%d double_free=%d\n",
            calloc_count, free_count, live_count, double_free);
    if (calloc_count < 3 || calloc_count != free_count || live_count != 0 || double_free != 0) {
        _Exit(97);
    }
}
"#,
    )
    .expect("write alloc_probe.c");

    let mut session = CompilationSession::new();
    let emit_exe = dir.join("emit-bin");
    let emit_opts = CompileOptions {
        stop_after: Stage::Build,
        output: Some(emit_exe),
        codegen: CodegenOptions { units: 1, jobs: 1 },
        ..CompileOptions::default()
    };
    let report = session.compile_path(&entry, &emit_opts);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "Build failed: {:?}",
        report.diagnostics
    );
    let ll_path = report
        .artifacts
        .get("llvm")
        .expect("llvm artifact path must be populated")
        .clone();

    let clang = std::env::var("ZIL_CLANG").unwrap_or_else(|_| "clang".to_string());
    for opt_flag in ["-O0", "-O2"] {
        let probed_exe = dir.join(format!("probed-{opt_flag}"));
        let link = Command::new(&clang)
            .arg(opt_flag)
            .arg(&ll_path)
            .arg(&probe_c)
            .arg("-Wl,--wrap=calloc")
            .arg("-Wl,--wrap=free")
            .arg("-o")
            .arg(&probed_exe)
            .output()
            .expect("invoke clang for allocator probe");
        assert!(
            link.status.success(),
            "clang link failed ({opt_flag}): {}",
            String::from_utf8_lossy(&link.stderr)
        );
        let run = Command::new(&probed_exe)
            .output()
            .expect("run probed executable");
        let stderr = String::from_utf8_lossy(&run.stderr);
        let stdout = String::from_utf8_lossy(&run.stdout);
        assert!(
            run.status.success(),
            "allocator probe failed ({opt_flag}, code={:?}): stderr={stderr}",
            run.status.code()
        );
        assert_eq!(stdout, "D");
        assert!(
            stderr.contains("live=0 double_free=0"),
            "expected zero leaks ({opt_flag}), got: {stderr}"
        );
    }

    // 2b. Controlled double-free self-test: prove that alloc_probe.c actively
    // catches a double-free (exit code 98, double_free=1) rather than silently passing.
    let df_ctrl_c = dir.join("double_free_control.c");
    std::fs::write(
        &df_ctrl_c,
        r#"
#include <stdlib.h>
int main(void) {
    void *volatile p = calloc(1, 16);
    free(p);
    free(p);
    return 0;
}
"#,
    )
    .expect("write double_free_control.c");
    let df_ctrl_exe = dir.join("double-free-control");
    let df_link = Command::new(&clang)
        .arg("-O0")
        .arg(&df_ctrl_c)
        .arg(&probe_c)
        .arg("-Wl,--wrap=calloc")
        .arg("-Wl,--wrap=free")
        .arg("-o")
        .arg(&df_ctrl_exe)
        .output()
        .expect("invoke clang for double-free control");
    assert!(
        df_link.status.success(),
        "double-free control link failed: {}",
        String::from_utf8_lossy(&df_link.stderr)
    );
    let df_run = Command::new(&df_ctrl_exe)
        .output()
        .expect("run double-free control");
    let df_stderr = String::from_utf8_lossy(&df_run.stderr);
    assert_eq!(
        df_run.status.code(),
        Some(98),
        "probe must catch controlled double-free with exit code 98, got {:?}, stderr={df_stderr}",
        df_run.status.code()
    );
    assert!(
        df_stderr.contains("double_free=1"),
        "probe stderr must report double_free=1, got: {df_stderr}"
    );

    // 3. Safety negative: using OwnedDyn after by-value consume MUST be SafeReject.
    check_rejected(
        &dir,
        "owned_dyn_double_consume",
        r#"
trait Consumer {
    fn consume(self: Self) -> i64;
}
struct Item { v: i64 }
impl Consumer for Item {
    fn consume(self: Self) -> i64 { return self.v }
}
fn bad() -> DynCreateError!i64 {
    let d = owned_dyn::<dyn Consumer>(Item { v: 1 })?
    let a = d.consume()
    let b = d.consume()
    return a + b
}
fn main() -> i32 {
    match bad() {
        .ok(v) => { return i32(v) }
        .err(_) => { return 1 }
    }
}
"#,
        "E-SAFETY",
    );
}

#[test]
fn raw_wire_valid_cert_owned_dyn_double_byval_consume_rejected() {
    use zil_base::{FileId, Span};
    use zil_check::{CheckBudget, CheckErrorKind, Violation, ViolationKind, check_module};
    use zil_ir::body::{
        Block, BlockId, Body, Callee, Certificate, ConstValue, DynDef, DynSlotSig, FnId, FnRole,
        FnSig, Function, FunctionCertificate, Instruction, InstructionKind, IntTy, Local,
        LocalDecl, Module, Operand, ParamEnv, Place, ResumeMode, Rvalue, Terminator, TyKind,
        TyTable, decode_module, encode_module,
    };
    use zil_solve::{Budget, SolveError};

    let sp = Span::new(FileId(0), 0, 1);
    let mut tys = TyTable::new();
    let i64_ty = tys.intern(TyKind::Int(IntTy::I64));
    let dyn_id = tys.add_dyn(DynDef {
        name: "Consumer".into(),
        slots: vec![DynSlotSig {
            name: "consume".into(),
            receiver: None,
            params: Vec::new(),
            result: i64_ty,
        }],
    });
    let owned_dyn_ty = tys.intern(TyKind::OwnedDyn {
        def: dyn_id,
        context: None,
    });

    // fn bad(d: OwnedDyn<dyn Consumer>) -> i64:
    //   r1 = DynSlot(Move(d), slot 0)
    //   r2 = DynSlot(Move(d), slot 0)   // UseOfUninit!
    //   return r2
    let locals = vec![
        LocalDecl {
            ty: owned_dyn_ty,
            name: Some("d".into()),
        },
        LocalDecl {
            ty: i64_ty,
            name: Some("r1".into()),
        },
        LocalDecl {
            ty: i64_ty,
            name: Some("r2".into()),
        },
    ];
    let body = Body {
        locals,
        param_count: 1,
        result: i64_ty,
        captures: Vec::new(),
        closure: None,
        param_env: ParamEnv::default(),
        drop_flags: Vec::new(),
        blocks: vec![
            Block {
                instructions: Vec::new(),
                terminator: Terminator::Call {
                    callee: Callee::DynSlot {
                        object: Operand::Move(Place::local(Local(0))),
                        slot: 0,
                    },
                    args: Vec::new(),
                    dest: Place::local(Local(1)),
                    target: BlockId(1),
                    cancel: None,
                },
                terminator_span: sp,
            },
            Block {
                instructions: Vec::new(),
                terminator: Terminator::Call {
                    callee: Callee::DynSlot {
                        object: Operand::Move(Place::local(Local(0))),
                        slot: 0,
                    },
                    args: Vec::new(),
                    dest: Place::local(Local(2)),
                    target: BlockId(2),
                    cancel: None,
                },
                terminator_span: sp,
            },
            Block {
                instructions: vec![Instruction {
                    kind: InstructionKind::Assign {
                        dest: Place::local(Local(2)),
                        value: Rvalue::Use(Operand::Const(ConstValue::Int {
                            ty: IntTy::I64,
                            bits: 0,
                        })),
                    },
                    span: sp,
                }],
                terminator: Terminator::Return {
                    value: Operand::Copy(Place::local(Local(2))),
                },
                terminator_span: sp,
            },
        ],
    };

    let module = Module {
        tys,
        functions: vec![Function {
            name: "bad_double_consume".into(),
            sig: FnSig {
                type_params: 0,
                region_params: 0,
                params: vec![owned_dyn_ty],
                result: i64_ty,
                role: FnRole::Normal,
                resume_mode: ResumeMode::Local,
                state_contracts: Vec::new(),
            },
            body,
        }],
        foreigns: Vec::new(),
        statics: Vec::new(),
        dyn_impls: Vec::new(),
    };

    let bytes = encode_module(&module);
    let decoded = decode_module(&bytes).expect("wire roundtrip must decode");
    let solve_errs = zil_solve::solve_module(&decoded, Budget { steps: 200_000 })
        .expect_err("solver must reject second by-value DynSlot call on moved OwnedDyn");
    assert!(
        solve_errs.iter().any(|e| matches!(
            e,
            SolveError::Violation {
                violation: Violation {
                    kind: ViolationKind::UseOfUninit { .. },
                    ..
                },
                ..
            }
        )),
        "expected UseOfUninit from solver, got: {solve_errs:?}"
    );

    let valid_cert = Certificate {
        functions: vec![FunctionCertificate {
            function: FnId(0),
            invariants: Vec::new(),
        }],
    };
    let check_err = check_module(decoded, &valid_cert, CheckBudget { steps: 200_000 })
        .expect_err("zil-check must reject second by-value DynSlot call on moved OwnedDyn");
    assert!(
        matches!(
            check_err.kind,
            CheckErrorKind::Violation(Violation {
                kind: ViolationKind::UseOfUninit { .. },
                ..
            })
        ),
        "expected exact ViolationKind::UseOfUninit from zil-check, got: {check_err:?}"
    );
}
