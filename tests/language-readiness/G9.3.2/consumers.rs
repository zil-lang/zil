//! Real ordinary Record consumers. Generated large fixtures are actual Source,
//! persisted by the same native harness as checked-in sources, never raw IR.
use super::*;

#[path = "allocation.rs"]
mod allocation;
#[path = "trust_boundary.rs"]
mod trust_boundary;

#[test]
fn g932_numeric_records_preserve_complete_values_and_copy_protocols() {
    native("g932-numeric", include_str!("numeric.zil"), b"POD\n", 0);
}

#[test]
fn g932_std_store_uses_actual_module_interfaces_and_native_values() {
    let source = include_str!("std-store.zil");
    let backend = if cfg!(feature = "llvm-api") {
        NativeBackend::LlvmApi
    } else {
        NativeBackend::Clang
    };
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory("g932-std-store");
        let input = dir.join("main.zil");
        fs::write(&input, source).unwrap();
        let exe = dir.join("program");
        // compile_path must resolve the real lib/std/store.zil bytes, check
        // its symbolic generics, and publish their concrete interfaces.
        let report = zil_driver::compile_path(
            &input,
            &CompileOptions {
                backend,
                stop_after: Stage::Build,
                optimization,
                output: Some(exe.clone()),
                emit_dir: Some(dir.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(dir.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert!(report.source_files.len() >= 2, "{}", report.to_json());
        assert!(report.verification_runs > 0);
        assert_eq!(report.native_codegen.as_ref().unwrap().backend, backend);
        let output = Command::new(exe).output().unwrap();
        fs::write(dir.join("stdout"), &output.stdout).unwrap();
        fs::write(dir.join("stderr"), &output.stderr).unwrap();
        fs::write(dir.join("exit"), format!("{:?}", output.status.code())).unwrap();
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert_eq!(output.stdout, b"STD\n", "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

#[test]
fn g932_actual_shared_view_last_use_and_distinct_roots_are_precise() {
    native("g932-view", include_str!("shared.zil"), b"VIEW\n", 0);
}

#[test]
fn g932_actual_live_shared_view_blocks_same_root_mutations_with_spare_capacity() {
    for (label, mutation) in [
        ("reserve", "store_reserve(items,4)"),
        ("push", "store_push(items,Cell{value:42})"),
        ("pop", "store_pop(items)"),
        ("give", "let moved:Store<Cell>=give items"),
        ("drop", "destroy(give items)"),
    ] {
        let source = format!(
            "struct Cell{{value:i32}}fn destroy(give items:Store<Cell>){{}}fn main()->i32{{let items:Store<Cell>=store_new();store_reserve(items,3);store_push(items,Cell{{value:41}});let view=store_at(items,0);{mutation};return view.value}}"
        );
        let report = check(&format!("g932-live-{label}"), &source, Stage::Verify);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{label}: {}",
            report.to_json()
        );
        assert!(
            matches!(report.phase, Phase::Analyze | Phase::Verify),
            "{label}: {}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}

#[test]
fn g932_pod_store_mutable_view_is_rejected_without_banning_record_mutability() {
    let source = "struct Cell{value:i32}fn main(){let items:Store<Cell>=store_new();store_reserve(items,1);store_push(items,Cell{value:41});let view=store_at_mut(items,0);view.value=42}";
    let report = check("g932-store-mut", source, Stage::Verify);
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
    pass(
        "g932-ordinary-mut",
        "struct Cell{value:i32}fn update(value:&mut Cell){value.value=42}fn main()->i32{var value=Cell{value:41};update(&mut value);return value.value-42}",
    );
}

// Build bounded binary arrays using the existing large-array-a18 source idiom.
// No repeat-literal syntax or enormous scalar source/SSA aggregate is assumed.
fn arrays(source: &mut String, levels: usize, scalar: &str) {
    source.push_str(&format!("type A0={scalar};\n"));
    for level in 1..=levels {
        source.push_str(&format!("type A{level}=[A{};2];\n", level - 1));
    }
    source.push_str(&format!(
        "fn all0(value:&A0,want:{scalar})->bool{{return *value==want}}\n"
    ));
    for level in 1..=levels {
        source.push_str(&format!("fn all{level}(value:&A{level},want:{scalar})->bool{{return all{}(&(*value)[0],want) && all{}(&(*value)[1],want)}}\n", level - 1, level - 1));
    }
    source.push_str("fn copy<T:Copy>(value:T)->T{return value}\n");
}

fn large_record(source: &mut String, bytes: usize, scalar: &str, scalar_bytes: usize) {
    let lanes = bytes / scalar_bytes;
    let fields: Vec<_> = (0..usize::BITS as usize)
        .filter(|level| lanes & (1usize << level) != 0)
        .collect();
    let max = *fields.last().unwrap();
    let name = format!("E{bytes}");
    source.push_str(&format!("struct {name}{{"));
    for level in &fields {
        source.push_str(&format!("part{level}:A{level};"));
    }
    source.push_str("}\n");
    source.push_str(&format!(
        "fn make{bytes}(seed:{scalar})->{name}{{let a0=seed;"
    ));
    for level in 1..=max {
        source.push_str(&format!("let a{level}=[a{},a{}];", level - 1, level - 1));
    }
    source.push_str(&format!("return {name}{{"));
    for level in &fields {
        source.push_str(&format!("part{level}:a{level},"));
    }
    source.push_str("}}\n");
    let predicates = fields
        .iter()
        .map(|level| format!("all{level}(&value.part{level},want)"))
        .collect::<Vec<_>>()
        .join(" && ");
    source.push_str(&format!(
        "fn intact{bytes}(value:&{name},want:{scalar})->bool{{return {predicates}}}\n"
    ));
    // Concrete helpers exercise the differing complete push/pop protocol ABI.
    source.push_str(&format!("fn relay{bytes}(value:{name}!void)->{name}!void{{return value}}\nfn pop{bytes}(items:Store<{name}>)->void!{name}{{return store_pop(items)}}\n"));
    let first = fields[0];
    let index = "[0]".repeat(first);
    source.push_str(&format!(r#"
fn run{bytes}()->i32{{
    let items:Store<{name}>=store_new()
    if !store_reserve(items,1){{return 1}}
    var original=make{bytes}(7{scalar})
    let success=relay{bytes}(store_push(items,original))
    let success_copy=copy(success)
    original.part{first}{index}=11{scalar}
    match success{{.ok(())=>{{}},.err(_)=>{{return 2}}}}
    match success_copy{{.ok(())=>{{}},.err(_)=>{{return 3}}}}
    var rejected=make{bytes}(9{scalar})
    let failed=relay{bytes}(store_push(items,rejected))
    let failed_copy=copy(failed)
    rejected.part{first}{index}=13{scalar}
    match failed{{.ok(())=>{{return 4}},.err(value)=>{{if !intact{bytes}(&value,9{scalar}){{return 5}}}}}}
    match failed_copy{{.ok(())=>{{return 6}},.err(value)=>{{if !intact{bytes}(&value,9{scalar}){{return 7}}}}}}
    if store_len(items)!=1 || store_capacity(items)!=1{{return 8}}
    let popped=pop{bytes}(items)
    let popped_copy=copy(popped)
    match popped{{.ok(value)=>{{if !intact{bytes}(&value,7{scalar}){{return 9}}}},.err(())=>{{return 10}}}}
    match popped_copy{{.ok(value)=>{{if !intact{bytes}(&value,7{scalar}){{return 11}}}},.err(())=>{{return 12}}}}
    match pop{bytes}(items){{.ok(_)=>{{return 13}},.err(())=>{{}}}}
    if store_len(items)!=0 || store_capacity(items)!=1{{return 14}}
    return 0
}}
"#));
}

#[test]
fn g932_4080_4088_4096_records_classify_complete_push_and_pop_protocols() {
    let mut source = String::from("extern \"C\" fn putchar(value:i32)->i32;\n");
    arrays(&mut source, 9, "u64");
    // align=8: (element, push, pop) byte sizes are respectively
    // Unit aggregate fields remain empty: (4080,4088,4088),
    // (4088,4096,4096), (4096,4104,4104).
    for bytes in [4080, 4088, 4096] {
        large_record(&mut source, bytes, "u64", 8);
    }
    source.push_str("fn main()->i32{if run4080()!=0{return 1};unsafe{putchar(65)};if run4088()!=0{return 2};unsafe{putchar(66)};if run4096()!=0{return 3};unsafe{putchar(67);putchar(10)};return 0}");
    native("g932-protocol-thresholds", &source, b"ABC\n", 0);
}

#[test]
fn g932_262144_byte_record_actual_store_native_o0_reliability() {
    let mut source = String::from("extern \"C\" fn putchar(value:i32)->i32;\n");
    arrays(&mut source, 18, "u8");
    large_record(&mut source, 262144, "u8", 1);
    source.push_str("fn main()->i32{if run262144()!=0{return 1};unsafe{putchar(76);putchar(65);putchar(82);putchar(71);putchar(69);putchar(10)};return 0}");
    native("g932-large-o0-o2", &source, b"LARGE\n", 0);
}
