//! Direct scalar Store Source, native values and genuine standard library.
use super::*;
#[test]
fn g934_all_scalar_values_mutate_and_roundtrip_full_error_payloads() {
    for (ty, first, second) in [
        ("bool", "true", "false"),
        ("u8", "41u8", "42u8"),
        ("u16", "40000u16", "40001u16"),
        ("i32", "41", "42"),
        ("i64", "-41i64", "-42i64"),
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
