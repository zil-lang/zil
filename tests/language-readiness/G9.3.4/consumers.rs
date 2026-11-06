//! Direct scalar Store Source, native values and genuine standard library.
use super::*;
#[test]
fn g934_all_scalar_values_mutate_and_roundtrip_full_error_payloads() {
    for (ty, first, second) in [
        ("bool", "true", "false"),
        ("u8", "u8(41)", "u8(42)"),
        ("u16", "u16(40000)", "u16(40001)"),
        ("i32", "41", "42"),
        ("i64", "i64(-41)", "i64(-42)"),
        ("u64", "18446744073709551613u64", "18446744073709551614u64"),
        ("usize", "2147483648usize", "2147483649usize"),
        ("f32", "1.25f32", "2.5f32"),
        ("f64", "2.25f64", "3.75f64"),
    ] {
        let text = format!(
            "extern \"C\" fn putchar(x:i32)->i32;fn main()->i32{{let values:Store<{ty}>=store_new();if !store_reserve(values,1){{return 1}}store_push(values,{first});let view=store_at_mut(values,0);*view={second};if *view!={second}{{return 2}}match store_push(values,{first}){{.ok(())=>{{return 3}},.err(value)=>{{if value!={first}{{return 4}}}}}}match store_pop(values){{.ok(value)=>{{if value!={second}{{return 5}}}},.err(_)=>{{return 6}}}}if store_len(values)!=0{{return 7}}unsafe{{putchar(83);putchar(10)}}return 0}}"
        );
        super::g933_consumers::native_all_backends(&format!("scalar-{ty}"), &text, b"S\n", false);
    }
}
#[test]
fn g934_real_std_store_and_scalar_alias() {
    let text = "import std.store;extern \"C\" fn putchar(x:i32)->i32;type Number=i32;fn bump(p:&mut Number){*p+=1}fn main()->i32{let values:Store<Number>=store.new();if !store.reserve(values,1){return 1}store.push(values,41);let view=store_at_mut(values,0);bump(view);if *view!=42{return 2}match store.pop(values){.ok(value)=>{if value!=42{return 3}},.err(_)=>{return 4}}unsafe{putchar(83);putchar(84);putchar(68);putchar(83);putchar(10)}return 0}";
    super::g933_consumers::native_all_backends("scalar-std", text, b"STDS\n", true);
}
#[test]
fn g934_scalar_windows_retain_actual_parent() {
    let legal = "fn main()->i32{let values:Store<i32>=store_new();store_reserve(values,2);store_push(values,41);let view=store_at_mut(values,0);*view=42;return *view}";
    assert_eq!(
        check("scalar-baseline", legal, Stage::Verify).status,
        ResultCategory::Success
    );
    for op in [
        "store_reserve(values,4);",
        "store_push(values,13);",
        "store_pop(values);",
    ] {
        let text = legal.replace("return *view", &format!("{op}return *view"));
        let r = check("scalar-live", &text, Stage::Verify);
        assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
        assert!(matches!(r.phase, Phase::Analyze | Phase::Verify));
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.code == "E-BORROW-CONFLICT" || d.code == "E-ANALYZE-IR"),
            "{}",
            r.to_json()
        );
    }
}

#[test]
fn g934_unsigned_value_above_signed_range_reaches_actual_stdout() {
    let text = "fn result()->u64{let values:Store<u64>=store_new();if !store_reserve(values,1){return 0u64}store_push(values,18446744073709551613u64);let view=store_at_mut(values,0);*view+=1u64;match store_pop(values){.ok(value)=>{return value},.err(_)=>{return 0u64}}}";
    super::g933_consumers::native_entry_all_backends(
        "scalar-full-u64",
        text,
        b"18446744073709551614\n",
        false,
        Some("result"),
    );
}
#[test]
fn g934_scalar_source_derived_raw_wire_parent_conflict() {
    super::g933_consumers::source_derived_receiver_conflict(
        "fn main()->u64{let first:Store<u64>=store_new();let second:Store<u64>=store_new();store_reserve(first,1);store_reserve(second,1);store_push(first,20u64);store_push(second,22u64);let a=store_at_mut(first,0);let b=store_at_mut(second,0);*b=23u64;return *a+*b}",
    );
}
