//! Independent Core metadata attacks and preserved source obligations for 237.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    Diagnostic, FileId, Fingerprint, Limits, ModuleId, NodeId, Phase, ResultCategory, SourceUnit,
    TargetProfile,
};
use zil_driver::{
    CompileOptions, NativeBackend, Optimization, ProjectSources, Stage, compile_project,
    compile_source,
};
use zil_ir::*;

const ORIGINAL: &str =
    include_str!("generic_defer_obligation_fixtures/product_store_replacement_cleanup.zil");
const SIMPLE: &str = r#"
resource Ticket{code:i32}
package Item{manages ticket:Ticket;value:i32}
package Bag<T:StoreElement>{manages items:Store<T>}
package Root<T:StoreElement>{child:Bag<T>;tag:i32}
fn wrap<T:StoreElement>(give items:Store<T>)->Root<T>{return Root::<T>{child:Bag::<T>{items:give items},tag:7}}
fn again<T:StoreElement>(give items:Store<T>)->Root<T>{return wrap(give items)}
fn main()->i32{let first:Store<Item>=store_new();let second:Store<Item>=store_new();let a=wrap(give first);let b=again(give second);return a.tag+b.tag-14}
"#;
const TWO_VALUE_ENTRY_FAILURE: &str = r#"
extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages ticket:Ticket}
package Entry<K:StoreElement,V:StoreElement>{key:K;value:V}
fn fail()->bool!i32{return true}
fn work()->bool!i32{
 let entry=Entry::<Item,Item>{key:Item{ticket:Ticket{code:65}},value:Item{ticket:Ticket{code:66}}};
 return fail()?
}
fn main()->i32{match work(){.err(_)=>{return 0},.ok(_)=>{return 1}}}
"#;

#[test]
fn two_independent_owned_value_fields_cleanup_on_error() {
    native("two-value-entry-error-cleanup", TWO_VALUE_ENTRY_FAILURE, "AB");
}

#[test]
fn store_of_two_independent_owned_value_fields_cleans_each_child() {
    native(
        "two-value-entry-store-cleanup",
        r#"
extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages ticket:Ticket}
package Entry<K:StoreElement,V:StoreElement>{key:K;value:V}
fn main()->i32{
 let values:Store<Entry<Item,Item>>=store_new();
 store_reserve(values,1);
 store_push(values,Entry::<Item,Item>{key:Item{ticket:Ticket{code:65}},value:Item{ticket:Ticket{code:66}}});
 return i32(store_len(values))-1
}
"#,
        "AB",
    );
}

#[test]
fn two_copy_actuals_have_no_cleanup_debt_but_package_stays_affine() {
    native(
        "two-copy-value-entry",
        "package Entry<K:StoreElement,V:StoreElement>{key:K;value:V}fn main()->i32{let entry=Entry::<i32,u64>{key:7,value:9};return entry.key-7}",
        "",
    );
    native(
        "two-copy-value-entry-store",
        "package Entry<K:StoreElement,V:StoreElement>{key:K;value:V}fn main()->i32{let values:Store<Entry<i32,u64>>=store_new();if !store_reserve(values,1){return 1}let pushed=store_push(values,Entry::<i32,u64>{key:7,value:9});return i32(store_len(values))-1}",
        "",
    );
    let duplicate = "package Entry<K:StoreElement,V:StoreElement>{key:K;value:V}fn main()->i32{let entry=Entry::<i32,u64>{key:7,value:9};let first=entry;let second=entry;return first.key+second.key}";
    let input = unit(duplicate);
    let failure = compile_source(
        &input,
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert!(
        matches!(failure.status, ResultCategory::SafeReject | ResultCategory::Unsupported),
        "a Copy actual must not make Package copyable: {failure:#?}"
    );
}

#[test]
fn nested_copy_value_package_retains_outer_owned_cleanup() {
    native(
        "nested-two-copy-value-entries",
        "package Entry<K:StoreElement,V:StoreElement>{key:K;value:V}fn main()->i32{let entry=Entry::<Entry<i32,u64>,Entry<i32,u64>>{key:Entry::<i32,u64>{key:7,value:9},value:Entry::<i32,u64>{key:8,value:10}};return entry.key.key+entry.value.key-15}",
        "",
    );
    native(
        "ordinary-copy-package-remains-affine",
        "package Pair{first:i32;second:u64}fn main()->i32{let value=Pair{first:7,second:9};return value.first-7}",
        "",
    );
    native(
        "ordinary-copy-package-store-element",
        "package Pair{first:i32;second:u64}fn main()->i32{let values:Store<Pair>=store_new();if !store_reserve(values,1){return 1}store_push(values,Pair{first:7,second:9});return i32(store_len(values))-1}",
        "",
    );
    native(
        "deep-mixed-value-entry-cleanup",
        r#"
extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages ticket:Ticket}
package Entry<K:StoreElement,V:StoreElement>{key:K;value:V}
fn main()->i32{
 let entry=Entry::<Entry<i32,u64>,Entry<Item,Item>>{
  key:Entry::<i32,u64>{key:7,value:9},
  value:Entry::<Item,Item>{key:Item{ticket:Ticket{code:65}},value:Item{ticket:Ticket{code:66}}}
 };
 return 0
}
"#,
        "AB",
    );
    native(
        "nested-copy-value-entry",
        r#"
extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages ticket:Ticket}
package Entry<K:StoreElement,V:StoreElement>{key:K;value:V}
fn main()->i32{
 let entry=Entry::<Entry<i32,u64>,Item>{key:Entry::<i32,u64>{key:7,value:9},value:Item{ticket:Ticket{code:65}}};
 return entry.key.key+entry.value.ticket.code-72
}
"#,
        "A",
    );
    native(
        "nested-copy-value-entry-store",
        r#"
extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages ticket:Ticket}
package Entry<K:StoreElement,V:StoreElement>{key:K;value:V}
fn main()->i32{
 let values:Store<Entry<Entry<i32,u64>,Item>>=store_new();
 if !store_reserve(values,1){return 1}
 let entry=Entry::<Entry<i32,u64>,Item>{key:Entry::<i32,u64>{key:7,value:9},value:Item{ticket:Ticket{code:65}}};
 store_push(values,entry);
 return i32(store_len(values))-1
}
"#,
        "A",
    );
}

#[test]
fn zero_debt_packages_still_require_one_whole_move() {
    for (label, text) in [
        (
            "ordinary-zero-debt-double-move",
            "package Pair{first:i32;second:u64}fn main()->i32{let value=Pair{first:7,second:9};let first=value;let second=value;return first.first+second.first}",
        ),
        (
            "nested-zero-debt-double-move",
            "package Entry<K:StoreElement,V:StoreElement>{key:K;value:V}fn main()->i32{let value=Entry::<Entry<i32,u64>,Entry<i32,u64>>{key:Entry::<i32,u64>{key:7,value:9},value:Entry::<i32,u64>{key:8,value:10}};let first=value;let second=value;return first.key.key+second.key.key}",
        ),
    ] {
        let result = compile_source(
            &unit(text),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert!(
            matches!(result.status, ResultCategory::SafeReject | ResultCategory::Unsupported),
            "{label}: {}",
            result.to_json()
        );
    }
}

#[test]
fn zero_debt_package_shape_is_rechecked_from_raw_and_wire() {
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    for text in [
        "package Pair{first:i32;second:u64}fn main()->i32{let value=Pair{first:7,second:9};return value.first-7}",
        "package Entry<K:StoreElement,V:StoreElement>{key:K;value:V}fn main()->i32{let value=Entry::<Entry<i32,u64>,Entry<i32,u64>>{key:Entry::<i32,u64>{key:7,value:9},value:Entry::<i32,u64>{key:8,value:10}};return value.key.key+value.value.key-15}",
    ] {
        let input = core(text);
        let module = zil_types::check(&input, &target, &limits).unwrap();
        let plan = zil_analyze::analyze(&module, &limits).unwrap();
        zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    }
    let input = core(
        "package Pair{first:i32;second:u64}fn main()->i32{let value=Pair{first:7,second:9};return value.first-7}",
    );
    let module = zil_types::check(&input, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();

    for case in ["borrowed-description", "duplicate-field"] {
        let mut changed = module.clone();
        let pair = changed
            .functions
            .iter_mut()
            .find(|function| function.name == "main")
            .unwrap()
            .locals
            .iter_mut()
            .find_map(|local| match &mut local.ty {
                Type::Package { fields, .. } if fields.len() == 2 => Some(fields),
                _ => None,
            })
            .unwrap();
        match case {
            "borrowed-description" => pair[0].1 = Type::Desc(Box::new(Type::I32)),
            "duplicate-field" => pair[1].0 = pair[0].0.clone(),
            _ => unreachable!(),
        }
        let mut candidate = plan.clone();
        candidate.module_fingerprint = changed.fingerprint();
        let encoded = zil_ir::wire::encode_module_candidate(&changed, &candidate, &limits).unwrap();
        for result in [
            zil_verify::verify_module(changed, candidate, &target, &limits),
            zil_verify::verify_module_bytes(&encoded, &target, &limits),
        ] {
            let errors = result.expect_err(case);
            assert!(
                errors.iter().any(|error| error.phase == Phase::Verify
                    && matches!(error.category, ResultCategory::SafeReject | ResultCategory::Unsupported)
                    && error.message == match case {
                        "borrowed-description" => "non-Copy type requires another checker",
                        "duplicate-field" => "package fields must have unique nonempty names",
                        _ => unreachable!(),
                    }),
                "{case}: {errors:#?}"
            );
        }
    }
}

#[test]
fn no_resource_zero_debt_package_rejects_raw_double_take_and_call() {
    let input = core(
        "package Pair{first:i32;second:u64}fn consume(value:Pair)->i32{return value.first}fn main()->i32{let first=Pair{first:7,second:9};let second=Pair{first:8,second:10};return consume(first)+consume(second)-15}",
    );
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let module = zil_types::check(&input, &target, &limits).unwrap();
    assert!(module.resources.is_empty());
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    for case in ["double-take", "same-ssa-double-call"] {
        let mut changed = module.clone();
        let function = changed.functions.iter_mut().find(|function| function.name == "main").unwrap();
        let values = &function.values;
        let calls: Vec<_> = function.blocks.iter().enumerate().flat_map(|(block_index, block)| {
            block.instructions.iter().enumerate().filter_map(move |(instruction_index, instruction)| {
                let InstructionKind::Call { arguments, .. } = &instruction.kind else { return None };
                (arguments.len() == 1 && matches!(&values[arguments[0].0 as usize], Type::Package { .. }))
                    .then_some((block_index, instruction_index, arguments[0]))
            })
        }).collect();
        assert_eq!(calls.len(), 2);
        let first_argument = calls[0].2;
        let (block_index, instruction_index, second_argument) = calls[1];
        match case {
            "double-take" => {
                let first_local = function.blocks.iter().flat_map(|block| &block.instructions)
                    .find_map(|instruction| match (&instruction.kind, instruction.result) {
                        (InstructionKind::TakeLocal { local }, Some(result)) if result == first_argument => Some(*local),
                        _ => None,
                    }).unwrap();
                let second_take = function.blocks[block_index].instructions.iter_mut()
                    .find(|instruction| instruction.result == Some(second_argument)).unwrap();
                let InstructionKind::TakeLocal { local } = &mut second_take.kind else { panic!("expected call argument to be a local transfer") };
                *local = first_local;
            }
            "same-ssa-double-call" => {
                let InstructionKind::Call { arguments, .. } = &mut function.blocks[block_index].instructions[instruction_index].kind else { unreachable!() };
                arguments[0] = first_argument;
            }
            _ => unreachable!(),
        }
        let analyzed = zil_analyze::analyze(&changed, &limits);
        let analyze_errors = analyzed.expect_err("Analyze must reject a duplicate affine transfer");
        assert!(analyze_errors.iter().any(|error| error.phase == Phase::Analyze
            && error.code == match case {
                "double-take" => "E-UNINITIALIZED",
                "same-ssa-double-call" => "E-ANALYZE-IR",
                _ => unreachable!(),
            }), "{case}: {analyze_errors:#?}");
        let mut candidate = plan.clone();
        candidate.module_fingerprint = changed.fingerprint();
        let wire = zil_ir::wire::encode_module_candidate(&changed, &candidate, &limits).unwrap();
        for verified in [
            zil_verify::verify_module(changed, candidate, &target, &limits),
            zil_verify::verify_module_bytes(&wire, &target, &limits),
        ] {
            let verify_errors = verified.expect_err("Verify and Wire must reject a duplicate affine transfer");
            assert!(verify_errors.iter().any(|error| error.phase == Phase::Verify
                && error.code == match case {
                    "double-take" => "E-VERIFY-INITIALIZATION",
                    "same-ssa-double-call" => "E-VERIFY-RESOURCE",
                    _ => unreachable!(),
                }), "{case}: {verify_errors:#?}");
        }
    }
}

#[test]
fn two_bound_ordinary_helpers_move_distinct_values_and_cleanup_on_error() {
    native(
        "two-bound-value-relay-error",
        r#"
extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages ticket:Ticket}
package Entry<K:StoreElement,V:StoreElement>{key:K;value:V}
fn flip<K:StoreElement,V:StoreElement>(entry:Entry<K,V>)->Entry<V,K>{
 return Entry::<V,K>{key:entry.value,value:entry.key}
}

fn relay<K:StoreElement,V:StoreElement>(entry:Entry<K,V>)->Entry<V,K>{return flip(entry)}
fn fail()->bool!i32{return true}
fn work()->bool!i32{
 let entry=relay(Entry::<Item,Managed<Ticket>>{key:Item{ticket:Ticket{code:65}},value:Ticket{code:66}});
 return fail()?
}
fn main()->i32{match work(){.err(_)=>{return 0},.ok(_)=>{return 1}}}
"#,
        "BA",
    );
}

#[test]
fn managed_value_field_take_has_one_debt_and_raw_descriptor_cannot_replace_it() {
    let source = r#"
resource Ticket{code:i32}
package Item{manages ticket:Ticket}
package Entry<K:StoreElement,V:StoreElement>{key:K;value:V}
fn main()->i32{
 let entry=Entry::<Managed<Ticket>,Item>{key:Ticket{code:65},value:Item{ticket:Ticket{code:66}}};
 let key:Managed<Ticket>=entry.key;
 return key.code-65
}
"#;
    native("managed-value-field-take", source, "");
    let input = core(source);
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let mut module = zil_types::check(&input, &target, &limits).unwrap();
    let mut plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    let take = module
        .functions
        .iter_mut()
        .find(|function| function.name == "main")
        .unwrap()
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.instructions)
        .find(|instruction| matches!(instruction.kind, InstructionKind::TakePackageField { .. }))
        .unwrap();
    let (local, index) = match &take.kind {
        InstructionKind::TakePackageField { local, index } => (*local, *index),
        _ => unreachable!(),
    };
    take.kind = InstructionKind::DescribePackageField { local, index };
    plan.module_fingerprint = module.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    for result in [
        zil_verify::verify_module(module, plan, &target, &limits),
        zil_verify::verify_module_bytes(&wire, &target, &limits),
    ] {
        let errors = result.expect_err("a descriptor cannot discharge the owned Value field");
        assert!(
            errors.iter().any(|error| error.phase == Phase::Verify
                && matches!(error.category, ResultCategory::SafeReject | ResultCategory::Unsupported)),
            "{errors:#?}"
        );
    }
    let duplicate = source.replace(
        "return key.code-65",
        "let second:Managed<Ticket>=entry.key;return key.code+second.code-130",
    );
    let rejected = compile_source(
        &unit(&duplicate),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert!(
        matches!(rejected.status, ResultCategory::SafeReject | ResultCategory::Unsupported),
        "{}",
        rejected.to_json()
    );
}

#[test]
fn mixed_bounds_and_complete_child_still_reject_duplicate_value_moves() {
    for (label, source) in [
        (
            "mixed-bound-formal",
            "package Entry<K:Copy+StoreElement,V:StoreElement>{key:K;value:V}fn dup<V:StoreElement>(entry:Entry<i32,V>)->V{let first=entry.value;return entry.value}fn main()->i32{return 0}",
        ),
        (
            "mixed-bound-complete-child",
            "package Item{manages bytes:ByteBuffer}package Parent<T:Copy+StoreElement>{child:Item;tag:T}fn dup<T:Copy+StoreElement>(p:Parent<T>)->Item{let first=p.child;return p.child}fn main()->i32{return 0}",
        ),
        (
            "mixed-bound-partial-whole-parent",
            "package Item{manages bytes:ByteBuffer}package Parent<T:Copy+StoreElement>{child:Item;tag:T}fn dup<T:Copy+StoreElement>(p:Parent<T>)->Parent<T>{let first=p.child;return p}fn main()->i32{return 0}",
        ),
        (
            "mixed-bound-explicit-managed-child",
            "resource Ticket{code:i32}package Parent<T:Copy+StoreElement>{child:Managed<Ticket>;tag:T}fn dup<T:Copy+StoreElement>(p:Parent<T>)->Managed<Ticket>{let first=p.child;return p.child}fn main()->i32{return 0}",
        ),
        (
            "mixed-bound-owned-enum-child",
            "resource Ticket{code:i32}enum Choice{None,Some(manages Ticket)}package Parent<T:Copy+StoreElement>{child:Choice;tag:T}fn dup<T:Copy+StoreElement>(p:Parent<T>)->Choice{let first=p.child;return p.child}fn main()->i32{return 0}",
        ),
    ] {
        let report = compile_source(
            &unit(source),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(report.status, ResultCategory::SafeReject, "{label}: {}", report.to_json());
        assert!(
            report.diagnostics.iter().any(|diagnostic| {
                diagnostic.phase == Phase::Expand
                    && diagnostic.message.contains(match label {
                        "mixed-bound-partial-whole-parent" => "partial callable State must be refilled",
                        _ => "owning Package field used after its Value moved",
                    })
            }),
            "{label}: {}",
            report.to_json()
        );
    }
}

#[test]
fn explicit_managed_value_argument_is_owned_without_manages_formal() {
    native(
        "two-value-entry-managed-argument",
        r#"
extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages ticket:Ticket}
package Entry<K:StoreElement,V:StoreElement>{key:K;value:V}
fn main()->i32{
 let entry=Entry::<Managed<Ticket>,Item>{key:Ticket{code:65},value:Item{ticket:Ticket{code:66}}};
 return 0
}
"#,
        "AB",
    );
    native(
        "two-value-entry-managed-store-element",
        r#"
extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages ticket:Ticket}
package Entry<K:StoreElement,V:StoreElement>{key:K;value:V}
fn main()->i32{
 let values:Store<Entry<Managed<Ticket>,Item>>=store_new();
 if !store_reserve(values,1){return 1}
 store_push(values,Entry::<Managed<Ticket>,Item>{key:Ticket{code:65},value:Item{ticket:Ticket{code:66}}});
 return i32(store_len(values))-1
}
"#,
        "AB",
    );
}

#[test]
fn two_byte_buffer_owners_remain_independent_value_fields() {
    native(
        "two-value-entry-byte-buffers",
        r#"
package Bytes{manages data:ByteBuffer}
package Entry<K:StoreElement,V:StoreElement>{key:K;value:V}
fn main()->i32{
 let left=bytes_new();let right=bytes_new();
 if !bytes_reserve(left,3){return 1}
 if !bytes_reserve(right,5){return 2}
 if !bytes_push(left,65u8){return 3}
 if !bytes_push(right,66u8){return 4}
 let entry=Entry::<Bytes,Bytes>{key:Bytes{data:give left},value:Bytes{data:give right}};
 return i32(bytes_len(entry.key.data))+i32(bytes_len(entry.value.data))-2
}
"#,
        "",
    );
}

#[test]
fn imported_two_value_entry_preserves_both_actuals_and_cleanup() {
    let project = ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "main.zil",
                r#"import library;
extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages ticket:Ticket}
fn main()->i32{
 let entry=library.Entry::<Item,Item>{key:Item{ticket:Ticket{code:65}},value:Item{ticket:Ticket{code:66}}};
 return 0
}"#,
            ),
            SourceUnit::new(
                FileId(1),
                ModuleId(1),
                "library.zil",
                "pub package Entry<K:StoreElement,V:StoreElement>{pub key:K;pub value:V}",
            ),
        ],
        paths: [(vec!["library".into()], ModuleId(1))].into(),
    };
    let out = dir("imported-two-value-entry");
    let report = compile_project(
        &project,
        &CompileOptions {
            stop_after: Stage::Build,
            backend: if cfg!(feature = "llvm-api") {
                NativeBackend::LlvmApi
            } else {
                NativeBackend::Clang
            },
            output: Some(out.join("program")),
            ..CompileOptions::default()
        },
    );
    assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
    let run = Command::new(&report.artifacts["executable"]).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert_eq!(run.stdout, b"AB");
}

#[test]
fn concrete_map_reserves_before_owned_entry_commit_and_borrows_its_key() {
    native(
        "two-value-map-insert-and-lookup",
        r#"resource Ticket{code:i32}
package Item{manages ticket:Ticket}
package Entry<K:StoreElement,V:StoreElement>{key:K;value:V}
package Map<K:StoreElement,V:StoreElement>{manages items:Store<Entry<K,V>>}
fn insert(map:&mut Map<Item,Item>,entry:Entry<Item,Item>)->Entry<Item,Item>!void{
 if !store_reserve(map.items,store_len(map.items)+1){return entry}
 return store_push(map.items,entry)
}

fn main()->i32{
 let items:Store<Entry<Item,Item>>=store_new();
 var map=Map::<Item,Item>{items:give items};
 let entry=Entry::<Item,Item>{key:Item{ticket:Ticket{code:65}},value:Item{ticket:Ticket{code:66}}};
 match insert(&mut map,entry){.err(failed)=>{return failed.key.ticket.code+failed.value.ticket.code-131},.ok(_)=>{if !contains(&map,65){return 1}return i32(store_len(map.items))-1}}
}
fn contains(map:&Map<Item,Item>,code:i32)->bool{
 var i:i64=0;
 while i<store_len(map.items){
  let entry=store_at(map.items,i);
  if entry.key.ticket.code==code{return true}
  i+=1
 }
 return false
}
"#,
        "",
    );
}

#[test]
fn ordinary_store_helper_borrows_actual_generic_package_field() {
    let source = r#"
import std.store
extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages ticket:Ticket}
package Entry<K:StoreElement,V:StoreElement>{key:K;value:V}
package Map<K:StoreElement,V:StoreElement>{manages items:Store<Entry<K,V>>}
fn remove_at<K:StoreElement,V:StoreElement>(map:&mut Map<K,V>,index:i64)->void!Entry<K,V>{
 return store.remove::<Entry<K,V>>(map.items,index)
}
fn main()->i32{
 let items:Store<Entry<Item,Managed<Ticket>>>=store_new();
 var map=Map::<Item,Managed<Ticket>>{items:give items};
 if !store_reserve(map.items,1){return 1}
 store_push(map.items,Entry::<Item,Managed<Ticket>>{key:Item{ticket:Ticket{code:65}},value:Ticket{code:66}});
 match remove_at(&mut map,0){.err(_)=>{return 1},.ok(old)=>{return old.key.ticket.code+old.value.code-131}}
}
"#;
    let project = |text: &str| ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(FileId(0), ModuleId(0), "main.zil", text),
            SourceUnit::new(FileId(1), ModuleId(1), "std/store.zil", include_str!("../../../lib/std/store.zil")),
        ],
        paths: [(vec!["std".into(), "store".into()], ModuleId(1))].into(),
    };
    for optimization in [Optimization::O0, Optimization::O2] {
        let out = dir("ordinary-store-remove-generic-map-field");
        let report = compile_project(&project(source), &CompileOptions {
            stop_after: Stage::Build,
            backend: if cfg!(feature = "llvm-api") { NativeBackend::LlvmApi } else { NativeBackend::Clang },
            optimization,
            output: Some(out.join("program")),
            ..CompileOptions::default()
        });
        assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
        let run = Command::new(&report.artifacts["executable"]).output().unwrap();
        assert_eq!(run.status.code(), Some(0), "{run:?}");
        assert_eq!(run.stdout, b"AB");
    }
    let shared = source.replace("map:&mut Map<K,V>", "map:&Map<K,V>");
    let report = compile_project(&project(&shared), &CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    });
    assert_eq!(report.status, ResultCategory::SafeReject, "{}", report.to_json());
    assert!(report.diagnostics.iter().any(|diagnostic|
        diagnostic.phase == Phase::Expand && diagnostic.message.contains("shared Package reference cannot write its Store field")),
        "{}", report.to_json());
}

#[test]
fn static_store_write_port_stays_held_through_later_arguments() {
    let prefix = "package Map<T:StoreElement>{manages items:Store<T>}fn mutate<T:Copy+StoreElement>(items:Store<T>,peek:&T)->T{store_pop(items);return *peek}fn mutate_rev<T:Copy+StoreElement>(peek:&T,items:Store<T>)->T{store_pop(items);return *peek}";
    for body in [
        "fn bad<T:Copy+StoreElement>(map:&mut Map<T>)->T{return mutate(map.items,store_at(map.items,0))}",
        "fn bad<T:Copy+StoreElement>(map:&mut Map<T>)->T{return mutate_rev(store_at(map.items,0),map.items)}",
    ] {
        let report = compile_source(&unit(&format!("{prefix}{body}fn main()->i32{{return 0}}")), &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        });
        assert_eq!(report.status, ResultCategory::SafeReject, "{}", report.to_json());
        assert!(report.diagnostics.iter().any(|diagnostic| diagnostic.phase == Phase::Expand
            && diagnostic.message.contains("conflicts with a prepared callee or argument window")), "{}", report.to_json());
    }
    let good = format!("{prefix}fn good(left:&mut Map<i32>,right:&Map<i32>)->i32{{return mutate(left.items,store_at(right.items,0))}}fn good_rev(left:&mut Map<i32>,right:&Map<i32>)->i32{{return mutate_rev(store_at(right.items,0),left.items)}}fn main()->i32{{let a:Store<i32>=store_new();let b:Store<i32>=store_new();store_reserve(a,1);store_reserve(b,1);store_push(a,7);store_push(b,9);var first=Map::<i32>{{items:give a}};let second=Map::<i32>{{items:give b}};return good(&mut first,&second)-9}}");
    let report = compile_source(&unit(&good), &CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    });
    assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
    let reversed = good.replace("good(&mut first,&second)", "good_rev(&mut first,&second)");
    let report = compile_source(&unit(&reversed), &CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    });
    assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
}

#[test]
fn owning_entries_insert_and_remove_complete_generic_values() {
    let main = r#"
import std.owning_entries
extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages ticket:Ticket}
fn main()->i32{
 var entries=owning_entries.new::<Item,Managed<Ticket>>()
 if !owning_entries.reserve::<Item,Managed<Ticket>>(&mut entries,2){return 1}
 let first=owning_entries.Entry::<Item,Managed<Ticket>>{key:Item{ticket:Ticket{code:65}},value:Ticket{code:66}}
 match owning_entries.push::<Item,Managed<Ticket>>(&mut entries,first){.err(_)=>{return 2},.ok(_)=>{}}
 let second=owning_entries.Entry::<Item,Managed<Ticket>>{key:Item{ticket:Ticket{code:67}},value:Ticket{code:68}}
 match owning_entries.insert_at::<Item,Managed<Ticket>>(&mut entries,0,second){.err(_)=>{return 3},.ok(_)=>{}}
 if owning_entries.len::<Item,Managed<Ticket>>(&entries)!=2{return 4}
 match owning_entries.remove_at::<Item,Managed<Ticket>>(&mut entries,0){.err(_)=>{return 5},.ok(old)=>{return old.key.ticket.code+old.value.code-135}}
}
"#;
    let project = ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(FileId(0), ModuleId(0), "main.zil", main),
            SourceUnit::new(FileId(1), ModuleId(1), "std/owning_entries.zil", include_str!("../../../lib/std/owning_entries.zil")),
            SourceUnit::new(FileId(2), ModuleId(2), "std/store.zil", include_str!("../../../lib/std/store.zil")),
        ],
        paths: [
            (vec!["std".into(), "owning_entries".into()], ModuleId(1)),
            (vec!["std".into(), "store".into()], ModuleId(2)),
        ].into(),
    };
    let failure = main.replace("(&mut entries,0,second)", "(&mut entries,99,second)");
    assert_ne!(failure, main);
    for (source, exit) in [(main, 0), (failure.as_str(), 3)] {
        let mut project = project.clone();
        project.modules[0] = SourceUnit::new(FileId(0), ModuleId(0), "main.zil", source);
        for optimization in [Optimization::O0, Optimization::O2] {
            let out = dir("owning-entries-complete-values");
            let report = compile_project(&project, &CompileOptions {
                stop_after: Stage::Build,
                backend: if cfg!(feature = "llvm-api") { NativeBackend::LlvmApi } else { NativeBackend::Clang },
                optimization,
                output: Some(out.join("program")),
                ..CompileOptions::default()
            });
            assert_eq!(report.status, ResultCategory::Success, "{}", report.to_json());
            let run = Command::new(&report.artifacts["executable"]).output().unwrap();
            assert_eq!(run.status.code(), Some(exit), "{run:?}");
            assert_eq!(run.stdout, b"CDAB", "{run:?}");
        }
    }
}

#[test]
fn symbolic_key_move_preserves_the_other_actual_owner() {
    native(
        "two-value-entry-partial-move",
        r#"extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages ticket:Ticket}
package Entry<K:StoreElement,V:StoreElement>{key:K;value:V}
fn take<T:StoreElement>(entry:Entry<T,Item>)->T{return entry.key}
fn main()->i32{
 let item=take(Entry::<Item,Item>{key:Item{ticket:Ticket{code:65}},value:Item{ticket:Ticket{code:66}}});
 return item.ticket.code-65
}
"#,
        "BA",
    );
}

#[test]
fn symbolic_field_refill_retires_old_value_once() {
    native(
        "two-value-entry-refill",
        r#"extern "C" fn putchar(n:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages ticket:Ticket}
package Entry<K:StoreElement,V:StoreElement>{key:K;value:V}
fn refill<T:StoreElement>(entry:Entry<T,i32>,replacement:T)->Entry<T,i32>{
 var current=entry;
 let old=current.key;
 current.key=replacement;
 return current
}
fn main()->i32{
 let entry=refill(Entry::<Item,i32>{key:Item{ticket:Ticket{code:65}},value:7},Item{ticket:Ticket{code:66}});
 return 0
}
"#,
        "AB",
    );
}

#[test]
fn two_formal_recipe_indices_and_argument_arity_are_independently_checked() {
    let original = core(TWO_VALUE_ENTRY_FAILURE);
    let template = original
        .aggregate_templates
        .iter()
        .position(|template| template.name == "Entry")
        .unwrap();
    for case in ["duplicate-formal", "gap", "out-of-range-recipe", "missing-actual"] {
        let mut raw = original.clone();
        let template_id = raw.aggregate_templates[template].id;
        match case {
            "duplicate-formal" => raw.aggregate_templates[template].parameters[1].name = "K".into(),
            "gap" => raw.aggregate_templates[template].parameters[1].index = 3,
            "out-of-range-recipe" => raw.aggregate_templates[template].fields[1].ty =
                CoreAggregateTypeRecipe::Parameter { index: 2 },
            "missing-actual" => raw
                .aggregate_applications
                .iter_mut()
                .find(|application| application.template == template_id)
                .unwrap()
                .arguments
                .pop()
                .map(|_| ())
                .unwrap(),
            _ => unreachable!(),
        }
        all_type_entries_reject(case, &raw);
    }
}

#[test]
fn a_borrowed_description_cannot_satisfy_an_owned_value_argument() {
    let mut raw = core(TWO_VALUE_ENTRY_FAILURE);
    let entry = raw
        .aggregate_templates
        .iter()
        .find(|template| template.name == "Entry")
        .unwrap()
        .id;
    let application = raw
        .aggregate_applications
        .iter_mut()
        .find(|application| application.template == entry)
        .unwrap();
    application.arguments[0] = CoreAggregateArgument::Concrete(Type::Desc(Box::new(
        Type::Record {
            name: "@module0:Ticket".into(),
            fields: vec![("code".into(), Type::I32)],
        },
    )));
    let diagnostics = zil_types::check(&raw, &TargetProfile::host_llvm(), &Limits::default())
        .unwrap_err();
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == "E-PRODUCT-TEMPLATE"
            && diagnostic.message.contains("StoreElement")
    }), "{diagnostics:#?}");
}

#[test]
fn unused_symbolic_value_field_cannot_be_moved_twice() {
    for body in [
        "fn duplicate<T:StoreElement>(entry:Entry<T,i32>)->Entry<T,T>{return Entry::<T,T>{key:entry.key,value:entry.key}}",
        "fn partial<T:StoreElement>(entry:Entry<T,i32>)->Entry<T,i32>{let key=entry.key;return entry}",
    ] {
        let source = format!(
            "package Entry<K:StoreElement,V:StoreElement>{{key:K;value:V}}{body}fn main()->i32{{return 0}}"
        );
        let report = compile_source(
            &unit(&source),
            &CompileOptions {
                stop_after: Stage::Core,
                ..CompileOptions::default()
            },
        );
        assert_eq!(report.status, ResultCategory::SafeReject, "{report:#?}");
        assert_eq!(report.phase, Phase::Expand);
    }
}
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn dir(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let out = root.join(format!(
        "review237-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&out).unwrap();
    out
}
fn unit(text: &str) -> SourceUnit {
    SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "generic_defer_obligations_contracts.zil",
        text,
    )
}
fn core(text: &str) -> CoreModule {
    let u = unit(text);
    let parsed = zil_syntax::parse(&u).into_result().unwrap();
    zil_expand::expand(&u, &parsed).unwrap()
}
fn program(input: &CoreModule) -> CoreProgram {
    CoreProgram {
        root: input.module_id,
        source_fingerprint: input.source_fingerprint,
        modules: vec![ProgramModule {
            core: input.clone(),
            imports: vec![],
            exports: vec![],
        }],
    }
}
fn rejected(result: Result<impl Sized, Vec<Diagnostic>>, label: &str) {
    let errors = match result {
        Ok(_) => panic!("{label}: accepted malformed metadata"),
        Err(e) => e,
    };
    assert!(
        errors.iter().any(|d| d.phase == Phase::Types
            && matches!(
                d.category,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            )),
        "{label}: {errors:#?}"
    );
}
fn all_type_entries_reject(label: &str, input: &CoreModule) {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let p = program(input);
    let out = dir(label);
    fs::write(out.join("core.txt"), format!("{input:#?}")).unwrap();
    let inferred = zil_types::infer_source_closures(input, &limits);
    let single = zil_types::check(input, &target, &limits);
    let shapes = zil_types::resolve_program_shapes(&p, &limits);
    let checked = zil_types::check_program(&p, &target, &limits);
    for (name, text) in [
        ("infer", format!("{inferred:#?}")),
        ("single", format!("{single:#?}")),
        ("shapes", format!("{shapes:#?}")),
        ("program", format!("{checked:#?}")),
    ] {
        fs::write(out.join(format!("{name}.txt")), text).unwrap();
    }
    // This public preparation API returns untrusted Core. It may defer a
    // semantic rejection, but cannot erase the attacker-controlled tables.
    match inferred {
        Ok(prepared) => {
            assert_eq!(prepared.aggregate_templates, input.aggregate_templates);
            assert_eq!(
                prepared.aggregate_applications.len(),
                input.aggregate_applications.len()
            );
            for (after, before) in prepared
                .aggregate_applications
                .iter()
                .zip(&input.aggregate_applications)
            {
                assert_eq!(after.aggregate, before.aggregate);
                assert_eq!(after.template_module, before.template_module);
                assert_eq!(after.template, before.template);
                assert_eq!(after.owner, before.owner);
                assert_eq!(after.arguments.len(), before.arguments.len());
                // This preparation may resolve Concrete Named types. It must
                // preserve the actual slots and the exact lexical owner.
                for (a, b) in after.arguments.iter().zip(&before.arguments) {
                    assert!(
                        matches!(
                            (a, b),
                            (
                                CoreAggregateArgument::Concrete(_),
                                CoreAggregateArgument::Concrete(_)
                            )
                        ) || a == b
                    );
                }
            }
            rejected(
                zil_types::check(&prepared, &target, &limits),
                &format!("{label}/prepared-single"),
            );
        }
        Err(errors) => rejected(Err::<CoreModule, _>(errors), &format!("{label}/infer")),
    }
    rejected(single, &format!("{label}/single"));
    rejected(shapes, &format!("{label}/shapes"));
    rejected(checked, &format!("{label}/program"));
}
fn native(label: &str, text: &str, expected: &str) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let out = dir(label);
        fs::write(out.join("source.zil"), text).unwrap();
        let backend = if cfg!(feature = "llvm-api") {
            NativeBackend::LlvmApi
        } else {
            NativeBackend::Clang
        };
        let r = compile_source(
            &unit(text),
            &CompileOptions {
                stop_after: Stage::Build,
                backend,
                optimization,
                output: Some(out.join("program")),
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), r.to_json()).unwrap();
        assert_eq!(
            r.status,
            ResultCategory::Success,
            "{label}: {}",
            r.to_json()
        );
        let summary = r.native_codegen.as_ref().unwrap();
        assert_eq!(summary.backend, backend);
        assert!(summary.fallback.is_none());
        let result = Command::new(&r.artifacts["executable"]).output().unwrap();
        fs::write(out.join("stdout"), &result.stdout).unwrap();
        fs::write(out.join("stderr"), &result.stderr).unwrap();
        assert_eq!(result.status.code(), Some(0), "{label}: {result:?}");
        assert_eq!(result.stdout, expected.as_bytes(), "{label}: {result:?}");
        assert!(result.stderr.is_empty());
    }
}

#[test]
fn original_success_obligation_is_preserved_byte_for_byte_and_executes() {
    native("product_store_replacement_cleanup", ORIGINAL, "AZ")
}

fn retained_source() -> String {
    ORIGINAL.replace("let root=wrap(give items)", "let root=again(give items)")
        + "\nfn again<T:StoreElement>(give items:Store<T>)->Root<T>{return wrap(give items)}\n"
}

#[test]
fn direct_and_retained_applications_share_family_identity_across_function_owners() {
    let text = retained_source();
    native("two-owners", &text, "AZ");
    let input = core(&text);
    assert!(!input.aggregate_templates.is_empty());
    assert!(!input.aggregate_applications.is_empty());
    let t = TargetProfile::host_llvm();
    let l = Limits::default();
    zil_types::check(&input, &t, &l).unwrap();
    zil_types::check_program(&program(&input), &t, &l).unwrap();
}

#[test]
fn unused_template_and_application_tables_cannot_override_the_generated_header() {
    let original = core(&retained_source());
    assert!(!original.aggregate_templates.is_empty());
    assert!(!original.aggregate_applications.is_empty());
    for mode in [
        "duplicate-template",
        "empty-family-name",
        "wrong-formal-index",
        "empty-formal-name",
        "duplicate-application",
        "missing-family",
        "foreign-family-module",
        "missing-header",
        "header-field-name",
        "header-field-permission",
        "header-field-ownership",
        "header-field-type",
        "missing-argument",
        "extra-argument",
    ] {
        let mut input = original.clone();
        match mode {
            "duplicate-template" => input
                .aggregate_templates
                .push(input.aggregate_templates[0].clone()),
            "empty-family-name" => input.aggregate_templates[0].name.clear(),
            "wrong-formal-index" => input.aggregate_templates[0].parameters[0].index = 1,
            "empty-formal-name" => input.aggregate_templates[0].parameters[0].name.clear(),
            "duplicate-application" => input
                .aggregate_applications
                .push(input.aggregate_applications[0].clone()),
            "missing-family" => input.aggregate_applications[0].template = NodeId(u32::MAX),
            "foreign-family-module" => {
                input.aggregate_applications[0].template_module = ModuleId(73)
            }
            "missing-header" => input.aggregate_applications[0].aggregate = NodeId(u32::MAX),
            "missing-argument" => input.aggregate_applications[0].arguments.clear(),
            "extra-argument" => input.aggregate_applications[0]
                .arguments
                .push(CoreAggregateArgument::Concrete(Type::I32)),
            _ => {
                let id = input.aggregate_applications[0].aggregate;
                let header = input.aggregates.iter_mut().find(|a| a.id == id).unwrap();
                match mode {
                    "header-field-name" => header.fields[0].name.push_str("_forged"),
                    "header-field-permission" => header.fields[0].public = !header.fields[0].public,
                    "header-field-ownership" => {
                        header.fields[0].manages = !header.fields[0].manages
                    }
                    "header-field-type" => header.fields[0].ty = Type::I64,
                    _ => unreachable!(),
                }
            }
        }
        all_type_entries_reject(mode, &input);
    }
}

#[test]
fn a_same_spelled_parameter_in_another_function_cannot_supply_application_authority() {
    let original = core(&retained_source());
    let index = original
        .aggregate_applications
        .iter()
        .position(|a| {
            a.owner.is_some()
                && a.arguments
                    .iter()
                    .any(|a| matches!(a, CoreAggregateArgument::Parameter { .. }))
        })
        .expect("retained ordinary owner application");
    let owning = original.aggregate_applications[index].owner.unwrap();
    let other = original
        .functions
        .iter()
        .find(|f| f.id != owning.function && !f.type_parameters.is_empty())
        .expect("another T owner")
        .id;
    for mode in [
        "absent-owner",
        "wrong-module",
        "non-template-owner",
        "same-spelled-other-owner",
        "parameter-outside-owner",
    ] {
        let mut input = original.clone();
        let ordinary = input
            .functions
            .iter()
            .find(|f| f.name == "main")
            .unwrap()
            .id;
        let application = &mut input.aggregate_applications[index];
        match mode {
            "absent-owner" => application.owner = None,
            "wrong-module" => application.owner.as_mut().unwrap().module = ModuleId(61),
            "non-template-owner" => application.owner.as_mut().unwrap().function = ordinary,
            "same-spelled-other-owner" => application.owner.as_mut().unwrap().function = other,
            "parameter-outside-owner" => {
                application.arguments[0] = CoreAggregateArgument::Parameter { index: 1 }
            }
            _ => unreachable!(),
        }
        all_type_entries_reject(mode, &input);
    }
}

#[test]
fn concrete_arguments_with_free_f_cannot_borrow_another_functions_same_f_binder() {
    let original = core(ORIGINAL);
    let caller = original
        .functions
        .iter()
        .find(|f| f.name == "run")
        .expect("complete-State caller F owner")
        .id;
    let other = original
        .functions
        .iter()
        .find(|f| f.id != caller && f.callable_parameters.iter().any(|b| b.generic_name == "F"))
        .expect("different callable F owner")
        .id;
    let index = original
        .aggregate_applications
        .iter()
        .position(|a| {
            a.owner.is_some_and(|o| o.function == caller)
                && a.arguments
                    .iter()
                    .any(|a| matches!(a, CoreAggregateArgument::Concrete(_)))
        })
        .expect("concrete nominal argument containing caller F");
    for mode in ["unowned-free-F", "same-F-wrong-owner"] {
        let mut input = original.clone();
        input.aggregate_applications[index].owner = if mode == "unowned-free-F" {
            None
        } else {
            Some(CoreAggregateOwner {
                module: input.module_id,
                function: other,
            })
        };
        all_type_entries_reject(mode, &input);
    }
}

#[test]
fn runtime_ct_folding_preserves_both_tables_and_exact_owner_ids() {
    let input = core(&format!("{SIMPLE}fn folded()->i32{{return #(6*7)}}"));
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let p = program(&input);
    let context = zil_ct::SourceProgramContext::new(&p, &limits).unwrap();
    for output in [
        zil_ct::evaluate_source_constants(&input, &target, &limits).unwrap(),
        zil_ct::evaluate_source_constants_with_context(&input, &target, &limits, &context)
            .unwrap()
            .0,
    ] {
        assert_eq!(output.aggregate_templates, input.aggregate_templates);
        assert_eq!(output.aggregate_applications, input.aggregate_applications);
    }
}

#[test]
fn unused_recursive_recipe_is_rejected_without_relying_on_a_value_use() {
    let mut input = core("resource Ticket{code:i32}fn main(){}");
    let span = input.functions[0].span;
    let family = NodeId(9000);
    input.aggregate_templates.push(CoreAggregateTemplate {
        id: family,
        name: "UnusedCycle".into(),
        parameters: vec![CoreTypeParameter {
            index: 0,
            name: "T".into(),
            bound: CoreTypeBound::StoreElement,
        }],
        fields: vec![
            CoreAggregateFieldRecipe {
                name: "child".into(),
                public: false,
                manages: false,
                ty: CoreAggregateTypeRecipe::Apply {
                    module: input.module_id,
                    template: family,
                    arguments: vec![CoreAggregateTypeRecipe::Parameter { index: 0 }],
                },
                span,
            },
            CoreAggregateFieldRecipe {
                name: "marker".into(),
                public: false,
                manages: true,
                ty: CoreAggregateTypeRecipe::Concrete(Type::Named("Ticket".into())),
                span,
            },
        ],
        span,
    });
    all_type_entries_reject("unused-recursive-recipe", &input);
}

const LIBRARY: &str = r#"
pub package Bag<T:StoreElement>{manages items:Store<T>}
pub package Root<T:StoreElement>{pub child:Bag<T>}
pub fn wrap<T:StoreElement>(give items:Store<T>)->Root<T>{return Root::<T>{child:Bag::<T>{items:give items}}}
pub fn unwrap<T:StoreElement>(root:Root<T>)->Store<T> contract{result(give)}{let child=root.child;return give child.items}
"#;
fn imported_project() -> ProjectSources {
    let client=ORIGINAL.replace("package Bag<T:StoreElement>{manages items:Store<T>}\n", "")
        .replace("package Root<T:StoreElement>{child:Bag<T>}\n", "")
        .replace("fn wrap<T:StoreElement>(give items:Store<T>)->Root<T>{return Root::<T>{child:Bag::<T>{items:give items}}}\n", "")
        .replace("let root=wrap(give items);let child=root.child;", "let root=library.wrap(give items);let unwrapped=library.unwrap::<Item<F>>(root);")
        .replace("store_at_mut(child.items,0)","store_at_mut(unwrapped,0)");
    ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "main.zil",
                format!("import library;\n{client}"),
            ),
            SourceUnit::new(FileId(1), ModuleId(1), "library.zil", LIBRARY),
        ],
        paths: [(vec!["library".into()], ModuleId(1))].into(),
    }
}
fn project_core(p: &ProjectSources) -> CoreProgram {
    let asts = p
        .modules
        .iter()
        .map(|s| zil_syntax::parse(s).into_result().unwrap())
        .collect::<Vec<_>>();
    let imports = vec![
        vec![CoreImport {
            alias: "library".into(),
            module: ModuleId(1),
            span: asts[0].items[0].span,
        }],
        vec![],
    ];
    let expanded = zil_expand::expand_program_with_limits(
        &p.modules,
        &asts.iter().collect::<Vec<_>>(),
        &imports,
        &Limits::default(),
    )
    .unwrap();
    CoreProgram {
        root: p.root,
        source_fingerprint: Fingerprint::ZERO,
        modules: expanded.into_iter().map(|(m, _)| m).collect(),
    }
}

#[test]
fn imported_family_preserves_the_callers_lexical_callable_owner_through_ct() {
    let project = imported_project();
    let input = project_core(&project);
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    zil_types::check_program(&input, &target, &limits).unwrap();
    let context = zil_ct::SourceProgramContext::new(&input, &limits).unwrap();
    let mut normalized = input.clone();
    for module in &mut normalized.modules {
        let (output, _) = zil_ct::evaluate_source_constants_with_context(
            &module.core,
            &target,
            &limits,
            &context,
        )
        .unwrap();
        assert_eq!(output.aggregate_templates, module.core.aggregate_templates);
        assert_eq!(
            output.aggregate_applications,
            module.core.aggregate_applications
        );
        module.core = output;
    }
    zil_types::check_program(&normalized, &target, &limits).unwrap();
    let mut forged = input.clone();
    let foreign_function = forged
        .modules
        .iter()
        .find(|m| m.core.module_id == ModuleId(1))
        .unwrap()
        .core
        .functions
        .iter()
        .find(|f| !f.type_parameters.is_empty())
        .unwrap()
        .id;
    let application = forged
        .modules
        .iter_mut()
        .flat_map(|m| &mut m.core.aggregate_applications)
        .find(|a| {
            a.template_module == ModuleId(1) && a.owner.is_some_and(|o| o.module == ModuleId(0))
        })
        .expect("imported recipe used by caller's F");
    application.owner = Some(CoreAggregateOwner {
        module: ModuleId(1),
        function: foreign_function,
    });
    rejected(
        zil_types::resolve_program_shapes(&forged, &limits),
        "foreign-owner/shapes",
    );
    rejected(
        zil_types::check_program(&forged, &target, &limits),
        "foreign-owner/program",
    );
    for optimization in [Optimization::O0, Optimization::O2] {
        let out = dir("cross-module");
        for source in &project.modules {
            fs::write(
                out.join(format!("module{}.zil", source.module_id.0)),
                &source.text,
            )
            .unwrap();
        }
        let backend = if cfg!(feature = "llvm-api") {
            NativeBackend::LlvmApi
        } else {
            NativeBackend::Clang
        };
        let report = compile_project(
            &project,
            &CompileOptions {
                stop_after: Stage::Build,
                backend,
                optimization,
                output: Some(out.join("program")),
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        let summary = report.native_codegen.as_ref().unwrap();
        assert_eq!(summary.backend, backend);
        assert!(summary.fallback.is_none());
        let result = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        fs::write(out.join("stdout"), &result.stdout).unwrap();
        fs::write(out.join("stderr"), &result.stderr).unwrap();
        assert_eq!(result.status.code(), Some(0), "{result:?}");
        assert_eq!(result.stdout, b"AZ");
        assert!(result.stderr.is_empty());
    }
}

fn phantom_metadata(name: &str) -> CoreModule {
    let mut input = core("resource Ticket{code:i32}fn main(){}");
    let span = input.functions[0].span;
    input.aggregate_templates.push(CoreAggregateTemplate {
        id: NodeId(9100),
        name: name.into(),
        parameters: vec![CoreTypeParameter {
            index: 0,
            name: "T".into(),
            bound: CoreTypeBound::StoreElement,
        }],
        fields: vec![CoreAggregateFieldRecipe {
            name: "marker".into(),
            public: false,
            manages: true,
            ty: CoreAggregateTypeRecipe::Concrete(Type::Named("Ticket".into())),
            span,
        }],
        span,
    });
    input
}

#[test]
fn unused_metadata_text_is_prepaid_in_types_and_ct_before_any_clone() {
    let target = TargetProfile::host_llvm();
    let limits = Limits {
        max_analysis_steps: 20_000,
        ..Limits::default()
    };
    let good = phantom_metadata("UnusedPhantom");
    zil_types::check(&good, &target, &limits).unwrap();
    zil_ct::evaluate_source_constants(&good, &target, &limits).unwrap();
    let huge = phantom_metadata(&"X".repeat(65_536));
    for (phase, result) in [
        (
            Phase::Types,
            zil_types::check(&huge, &target, &limits).map(|_| ()),
        ),
        (
            Phase::Ct,
            zil_ct::evaluate_source_constants(&huge, &target, &limits).map(|_| ()),
        ),
    ] {
        let errors = result.expect_err("metadata text must not be copied for free");
        assert!(
            errors
                .iter()
                .all(|d| d.phase == phase && d.category == ResultCategory::ResourceLimit),
            "{errors:#?}"
        );
    }
}

#[test]
fn an_uninstantiated_family_still_has_complete_recipe_and_ownership_checks() {
    for mode in [
        "free-Concrete-F",
        "recipe-parameter-slot",
        "unowned-address",
        "duplicate-field",
        "absent-Apply-family",
    ] {
        let mut input = phantom_metadata("UnusedInvalid");
        let template = &mut input.aggregate_templates[0];
        let field = &mut template.fields[0];
        match mode {
            "free-Concrete-F" => {
                field.ty = CoreAggregateTypeRecipe::Concrete(Type::Named("F".into()))
            }
            "recipe-parameter-slot" => field.ty = CoreAggregateTypeRecipe::Parameter { index: 1 },
            "unowned-address" => {
                field.manages = false;
                field.ty = CoreAggregateTypeRecipe::Concrete(Type::Address {
                    mutable: false,
                    inner: Box::new(Type::I32),
                });
            }
            "duplicate-field" => {
                let copy = field.clone();
                template.fields.push(copy);
            }
            "absent-Apply-family" => {
                field.manages = false;
                field.ty = CoreAggregateTypeRecipe::Apply {
                    module: input.module_id,
                    template: NodeId(9292),
                    arguments: vec![CoreAggregateTypeRecipe::Parameter { index: 0 }],
                };
            }
            _ => unreachable!(),
        }
        all_type_entries_reject(mode, &input);
    }
}

#[test]
fn an_uninstantiated_copy_only_value_recipe_is_valid() {
    let mut input = phantom_metadata("UnusedCopyValue");
    input.aggregate_templates[0].fields[0].manages = false;
    input.aggregate_templates[0].fields[0].ty =
        CoreAggregateTypeRecipe::Concrete(Type::I32);
    zil_types::check(&input, &TargetProfile::host_llvm(), &Limits::default()).unwrap();
}

#[test]
fn types_new_recipe_work_has_an_exact_fresh_limit_boundary() {
    let input = core(SIMPLE);
    types_work_boundary(&input, "types-budget");
}

fn types_work_boundary(input: &CoreModule, label: &str) {
    let target = TargetProfile::host_llvm();
    let default = Limits::default();
    let expected = zil_types::check(input, &target, &default).unwrap();
    let run = |steps| match zil_types::check(
        input,
        &target,
        &Limits {
            max_analysis_steps: steps,
            ..default.clone()
        },
    ) {
        Ok(actual) => {
            assert_eq!(actual, expected);
            true
        }
        Err(errors) => {
            assert!(
                errors
                    .iter()
                    .all(|d| d.category == ResultCategory::ResourceLimit),
                "{errors:#?}"
            );
            false
        }
    };
    let (mut low, mut high) = (0, default.max_analysis_steps);
    assert!(run(high));
    while high - low > 1 {
        let mid = low + (high - low) / 2;
        if run(mid) { high = mid } else { low = mid }
    }
    assert!(run(high));
    assert!(!run(high - 1));
    assert!(run(high));
    let out = dir(label);
    fs::write(out.join("boundary.json"),format!("{{\"types_N\":{high},\"N_pass\":true,\"N_minus_one_resource_limit\":true,\"fresh_N_pass\":true}}\n")).unwrap();
}

const PHANTOM: &str = r#"
resource Ticket{code:i32}
package ItemA{manages ticket:Ticket;value:i32}
package ItemB{manages ticket:Ticket;value:i32}
package Phantom<T:StoreElement>{manages marker:Ticket}
fn first<T:StoreElement>(give items:Store<T>)->Phantom<T>{return Phantom::<T>{marker:Ticket{code:7}}}
fn second<T:StoreElement>(give items:Store<T>)->Phantom<T>{return Phantom::<T>{marker:Ticket{code:7}}}
fn accept(value:Phantom<ItemA>)->i32{return value.marker.code}
"#;

#[test]
fn phantom_actuals_participate_in_identity_but_resolved_function_owners_do_not() {
    let good = format!(
        "{PHANTOM}package Item<F>{{manages ticket:Ticket;callback:F;value:i32}}package Packet<F>{{manages marker:Ticket;callback:F}}fn make()->impl Fn()->i32{{let n=8;return move()=>n}}fn pack<F:Fn()->i32>(callback:F)->Packet<F>{{return Packet::<F>{{marker:Ticket{{code:7}},callback:callback}}}}fn run<F:Fn()->i32>(packet:Packet<F>)->i32{{let a:Store<Item<F>>=store_new();let b:Store<Item<F>>=store_new();let x:Phantom<Item<F>>=first(give a);let y:Phantom<Item<F>>=second(give b);let z=Phantom::<Item<F>>{{marker:Ticket{{code:7}}}};return x.marker.code+y.marker.code+z.marker.code-21}}fn main()->i32{{return run(pack(make()))}}"
    );
    native("phantom-direct-retained", &good, "");
    let bad = format!(
        "{PHANTOM}fn main()->i32{{let b:Store<ItemB>=store_new();return accept(second(give b))-7}}"
    );
    let out = dir("phantom-mismatch");
    fs::write(out.join("source.zil"), &bad).unwrap();
    let report = compile_source(
        &unit(&bad),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    fs::write(out.join("report.json"), report.to_json()).unwrap();
    assert!(
        matches!(
            report.status,
            ResultCategory::SafeReject | ResultCategory::Unsupported
        ),
        "{}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
}

#[test]
fn canonical_product_names_do_not_erase_cross_module_field_privacy() {
    for field in ["child", "items"] {
        let mut project = imported_project();
        if field == "child" {
            project.modules[1].text = project.modules[1].text.replace("pub child", "child");
        }
        project.modules[0].text = format!(
            "import library;resource Ticket{{n:i32}}package Item{{manages ticket:Ticket;value:i32}}fn main()->i32{{let items:Store<Item>=store_new();let root=library.wrap(give items);let child=root.child;{}return 0}}",
            if field == "items" {
                "let secret=child.items;let n=store_len(secret);"
            } else {
                ""
            }
        );
        let out = dir(&format!("private-{field}"));
        let report = compile_project(
            &project,
            &CompileOptions {
                stop_after: Stage::Verify,
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "E-FIELD-PRIVATE"),
            "{}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}

#[test]
fn natural_product_input_infers_t_across_two_retained_function_owners() {
    let definitions = "\nfn relay<T:StoreElement>(root:Root<T>)->Root<T>{return root}\nfn relay_again<T:StoreElement>(root:Root<T>)->Root<T>{return relay(root)}\n";
    let explicit = ORIGINAL.replace(
        "let root=wrap(give items)",
        "let root=relay_again::<Item<F>>(relay::<Item<F>>(wrap(give items)))",
    ) + definitions;
    native("product-input-explicit-control", &explicit, "AZ");
    let natural = ORIGINAL.replace(
        "let root=wrap(give items)",
        "let root=relay_again(relay(wrap(give items)))",
    ) + definitions;
    native("product-input-natural", &natural, "AZ");
}

const PHANTOM_RELAY: &str = r#"
fn phantom_relay<T:StoreElement>(value:Phantom<T>)->Phantom<T>{return value}
fn phantom_again<T:StoreElement>(value:Phantom<T>)->Phantom<T>{return phantom_relay(value)}
"#;
fn phantom_input_source(explicit: bool) -> String {
    let first = if explicit {
        "phantom_again::<Item<F>>(phantom_relay::<Item<F>>(first(give a)))"
    } else {
        "phantom_again(phantom_relay(first(give a)))"
    };
    let second = if explicit {
        "phantom_relay::<Item<F>>(second(give b))"
    } else {
        "phantom_relay(second(give b))"
    };
    format!(
        "{PHANTOM}{PHANTOM_RELAY}package Item<F>{{manages ticket:Ticket;callback:F;value:i32}}package Packet<F>{{manages marker:Ticket;callback:F}}fn make()->impl Fn()->i32{{let n=8;return move()=>n}}fn pack<F:Fn()->i32>(callback:F)->Packet<F>{{return Packet::<F>{{marker:Ticket{{code:7}},callback:callback}}}}fn run<F:Fn()->i32>(packet:Packet<F>)->i32{{let a:Store<Item<F>>=store_new();let b:Store<Item<F>>=store_new();let x={first};let y={second};let z=Phantom::<Item<F>>{{marker:Ticket{{code:7}}}};return x.marker.code+y.marker.code+z.marker.code-21}}fn main()->i32{{return run(pack(make()))}}"
    )
}

#[test]
fn natural_phantom_input_infers_actual_arguments_that_do_not_appear_in_fields() {
    native(
        "phantom-input-explicit-control",
        &phantom_input_source(true),
        "",
    );
    native("phantom-input-natural", &phantom_input_source(false), "");
}

const INPUT_RELAY_MODEL: &str = r#"
package InputBag<T:StoreElement>{manages items:Store<T>}
package InputRoot<T:StoreElement>{child:InputBag<T>}
package OtherRoot<T:StoreElement>{child:InputBag<T>}
package OtherPhantom<T:StoreElement>{manages marker:Ticket}
fn input_wrap<T:StoreElement>(give items:Store<T>)->InputRoot<T>{return InputRoot::<T>{child:InputBag::<T>{items:give items}}}
fn input_relay<T:StoreElement>(root:InputRoot<T>)->InputRoot<T>{return root}
fn input_pick<T:StoreElement>(left:InputRoot<T>,right:InputRoot<T>)->InputRoot<T>{return left}
fn phantom_pick<T:StoreElement>(left:Phantom<T>,right:Phantom<T>)->Phantom<T>{return left}
fn unconstrained<T:StoreElement>()->Phantom<T>{return Phantom::<T>{marker:Ticket{code:7}}}
"#;

#[test]
fn natural_input_inference_requires_the_same_family_and_one_complete_actual_t() {
    for (label, body) in [
        (
            "root-different-family-same-layout",
            "let items:Store<ItemA>=store_new();let other=OtherRoot::<ItemA>{child:InputBag::<ItemA>{items:give items}};let value=input_relay(other);",
        ),
        (
            "root-conflicting-actuals",
            "let a:Store<ItemA>=store_new();let b:Store<ItemB>=store_new();let value=input_pick(input_wrap(give a),input_wrap(give b));",
        ),
        (
            "root-explicit-wrong-actual",
            "let items:Store<ItemA>=store_new();let value=input_relay::<ItemB>(input_wrap(give items));",
        ),
        (
            "phantom-conflicting-actuals",
            "let a=Phantom::<ItemA>{marker:Ticket{code:7}};let b=Phantom::<ItemB>{marker:Ticket{code:7}};let value=phantom_pick(a,b);",
        ),
        (
            "phantom-different-family-same-layout",
            "let other=OtherPhantom::<ItemA>{marker:Ticket{code:7}};let value=phantom_relay(other);",
        ),
        ("phantom-no-input-constraint", "let value=unconstrained();"),
        (
            "give-is-still-not-product-move",
            "let items:Store<ItemA>=store_new();let old=input_wrap(give items);let value=input_relay(give old);",
        ),
    ] {
        let text = format!("{PHANTOM}{PHANTOM_RELAY}{INPUT_RELAY_MODEL}fn main(){{{body}}}");
        let out = dir(label);
        fs::write(out.join("source.zil"), &text).unwrap();
        let report = compile_source(
            &unit(&text),
            &CompileOptions {
                stop_after: Stage::Verify,
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{label}: {}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| matches!(d.phase, Phase::Expand | Phase::Types)),
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
        assert!(!report.artifacts.contains_key("executable"));
    }
}

fn nested_phantom_input_source(explicit: bool) -> String {
    let call = if explicit {
        "nested_relay::<Item<F>>(carrier)"
    } else {
        "nested_relay(carrier)"
    };
    format!(
        "{PHANTOM}package Carrier<T:StoreElement>{{manages items:Store<T>}}fn nested_relay<T:StoreElement>(value:Carrier<Phantom<T>>)->Carrier<Phantom<T>>{{return value}}package Item<F>{{manages ticket:Ticket;callback:F;value:i32}}package Packet<F>{{manages marker:Ticket;callback:F}}fn make()->impl Fn()->i32{{let n=8;return move()=>n}}fn pack<F:Fn()->i32>(callback:F)->Packet<F>{{return Packet::<F>{{marker:Ticket{{code:7}},callback:callback}}}}fn run<F:Fn()->i32>(packet:Packet<F>)->i32{{let items:Store<Phantom<Item<F>>> = store_new();let carrier=Carrier::<Phantom<Item<F>>>{{items:give items}};let moved={call};return i32(store_len(moved.items))}}fn main()->i32{{return run(pack(make()))}}"
    )
}

#[test]
fn natural_input_finds_t_in_nested_family_arguments_even_when_inner_t_is_phantom() {
    native(
        "nested-phantom-explicit-control",
        &nested_phantom_input_source(true),
        "",
    );
    native(
        "nested-phantom-natural",
        &nested_phantom_input_source(false),
        "",
    );
}

#[test]
fn nested_phantom_identity_survives_rebinding_between_two_ordinary_t_owners() {
    let text = nested_phantom_input_source(false).replace(
        "let moved=nested_relay(carrier)",
        "let moved:Carrier<Phantom<Item<F>>>=nested_again(nested_relay(carrier))",
    ) + "\nfn nested_again<T:StoreElement>(value:Carrier<Phantom<T>>)->Carrier<Phantom<T>>{return nested_relay(value)}\n";
    native("nested-phantom-two-T-owners", &text, "");
}

fn nested_phantom_owner_attack(owner_name: Option<&str>) -> CoreModule {
    let mut input = core(&nested_phantom_input_source(false));
    zil_types::check(&input, &TargetProfile::host_llvm(), &Limits::default())
        .expect("unmodified nested phantom source remains valid");
    let family = input
        .aggregate_templates
        .iter()
        .find(|t| t.name == "Carrier")
        .expect("outer Carrier family")
        .id;
    let run = input
        .functions
        .iter()
        .find(|f| f.name == "run")
        .expect("original F owner")
        .id;
    let index = input
        .aggregate_applications
        .iter()
        .position(|a| a.template == family && a.owner.is_some_and(|o| o.function == run))
        .expect("Carrier whose argument hides F in Phantom's nominal arguments");
    let replacement = owner_name.map(|name| {
        let function = input
            .functions
            .iter()
            .find(|f| f.name == name)
            .expect("alternate same-spelled F function");
        assert!(
            function
                .callable_parameters
                .iter()
                .any(|b| b.generic_name == "F")
        );
        assert_ne!(function.id, run);
        CoreAggregateOwner {
            module: input.module_id,
            function: function.id,
        }
    });
    input.aggregate_applications[index].owner = replacement;
    input
}

#[test]
fn hidden_phantom_arguments_cannot_borrow_another_functions_same_f_owner() {
    all_type_entries_reject(
        "hidden-phantom-same-F-wrong-owner",
        &nested_phantom_owner_attack(Some("pack")),
    );
}

#[test]
fn hidden_phantom_arguments_cannot_erase_their_outer_application_owner() {
    all_type_entries_reject(
        "hidden-phantom-absent-owner",
        &nested_phantom_owner_attack(None),
    );
}

#[test]
fn an_unused_concrete_recipe_cannot_capture_f_hidden_in_another_familys_arguments() {
    let mut input = core(&nested_phantom_input_source(false));
    let run = input.functions.iter().find(|f| f.name == "run").unwrap().id;
    let phantom = input
        .aggregate_templates
        .iter()
        .find(|t| t.name == "Phantom")
        .unwrap()
        .id;
    let header = input
        .aggregate_applications
        .iter()
        .find(|a| a.template == phantom && a.owner.is_some_and(|o| o.function == run))
        .and_then(|a| input.aggregates.iter().find(|d| d.id == a.aggregate))
        .expect("Phantom actual with F absent from its fields");
    let span = header.span;
    let hidden = header.name.clone();
    input.aggregate_templates.push(CoreAggregateTemplate {
        id: NodeId(9200),
        name: "UnusedCannotCaptureCallerF".into(),
        parameters: vec![CoreTypeParameter {
            index: 0,
            name: "T".into(),
            bound: CoreTypeBound::StoreElement,
        }],
        fields: vec![CoreAggregateFieldRecipe {
            name: "child".into(),
            public: false,
            manages: false,
            ty: CoreAggregateTypeRecipe::Concrete(Type::Named(hidden)),
            span,
        }],
        span,
    });
    all_type_entries_reject("unused-recipe-hidden-free-F", &input);
}

fn nested_raw_natural_core() -> CoreModule {
    nested_raw_natural_core_from(&nested_phantom_input_source(false))
}

fn nested_raw_natural_core_from(text: &str) -> CoreModule {
    let mut input = core(text);
    let run = input
        .functions
        .iter_mut()
        .find(|f| f.name == "run")
        .unwrap();
    let initializer = run
        .body
        .statements
        .iter_mut()
        .find_map(|s| match &mut s.kind {
            CoreStatementKind::Binding {
                name,
                initializer: Some(e),
                ..
            } if name == "moved" => Some(e),
            _ => None,
        })
        .expect("nested call initializer");
    let CoreExprKind::Call { callee, .. } = &mut initializer.kind else {
        panic!("expected a static call, got {initializer:#?}");
    };
    let CoreExprKind::TypeApply {
        callee: target,
        type_arguments,
    } = &callee.kind
    else {
        panic!("Expand should supply the direct control's type argument: {callee:#?}");
    };
    assert!(matches!(&target.kind, CoreExprKind::Name(n) if n == "nested_relay"));
    assert_eq!(type_arguments.len(), 1);
    *callee = target.clone();
    input
}

#[test]
fn raw_static_call_can_infer_t_through_nested_phantom_origin_arguments() {
    let input = nested_raw_natural_core();
    let out = dir("raw-nested-phantom-natural");
    fs::write(out.join("core.txt"), format!("{input:#?}")).unwrap();
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let single = zil_types::check(&input, &target, &limits);
    let checked = zil_types::check_program(&program(&input), &target, &limits);
    fs::write(out.join("single.txt"), format!("{single:#?}")).unwrap();
    fs::write(out.join("program.txt"), format!("{checked:#?}")).unwrap();
    let module =
        single.expect("raw static ordinary call must infer from full nested nominal arguments");
    checked.expect("program entry must preserve the same natural-call inference");
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    fs::write(out.join("candidate.wire"), &bytes).unwrap();
    zil_verify::verify_module(module, plan, &target, &limits).unwrap();
    zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap();
}

#[test]
fn nested_nominal_inference_work_has_an_exact_fresh_limit_boundary() {
    types_work_boundary(&nested_raw_natural_core(), "types-nested-inference-budget");
}

#[test]
fn recursive_query_accepts_two_finite_instances_of_the_same_phantom_family() {
    let text = nested_phantom_input_source(false)
        .replace("Carrier<Phantom<T>>", "Carrier<Phantom<Phantom<T>>>")
        .replace(
            "Store<Phantom<Item<F>>>",
            "Store<Phantom<Phantom<Item<F>>>>",
        )
        .replace(
            "Carrier::<Phantom<Item<F>>>",
            "Carrier::<Phantom<Phantom<Item<F>>>>",
        );
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let original = core(&text);
    zil_types::check(&original, &target, &limits).expect("finite source-derived control");
    let input = nested_raw_natural_core_from(&text);
    let out = dir("raw-repeated-phantom-family");
    fs::write(out.join("source.zil"), &text).unwrap();
    fs::write(out.join("core.txt"), format!("{input:#?}")).unwrap();
    let single = zil_types::check(&input, &target, &limits);
    let checked = zil_types::check_program(&program(&input), &target, &limits);
    fs::write(out.join("single.txt"), format!("{single:#?}")).unwrap();
    fs::write(out.join("program.txt"), format!("{checked:#?}")).unwrap();
    single.expect("the same family with distinct finite full arguments is not an origin cycle");
    checked.expect("program preparation must preserve both finite family instances");
}
