//! Source-level Analyze controls. Verify's raw/Wire tests are independently authored.
use super::*;

const BAD: &str = include_str!("helper-retirement-old-desc.zil");
const INLINE: &str = include_str!("helper-retirement-local-counterpart.zil");
const PREFIX: &str = "resource R{code:i32}package P{manages owner:R;n:i32}";

fn check(text: &str, rejected: bool) {
    let report = compile_source(
        &source(text),
        &CompileOptions {
            stop_after: Stage::Analyze,
            ..CompileOptions::default()
        },
    );
    if rejected {
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Analyze, "{}", report.to_json());
        assert!(
            report.diagnostics.iter().any(|d| d.code == "E-ANALYZE-IR"
                && d.message.contains("may retire a resource instance")),
            "{}",
            report.to_json()
        );
    } else {
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn original_helper_and_inline_actual_commit_both_reject_old_desc() {
    check(BAD, true);
    check(INLINE, true);
}

#[test]
fn readonly_copy_write_and_pre_retirement_last_use_are_legal() {
    for body in ["let n=target.n;", "target.n=42;"] {
        check(&BAD.replace("target.owner=R{code:66};", body), false);
    }
    check(
        &BAD.replace(
            "reset(&mut packet);\n    return old.code;",
            "let value=old.code;\n    reset(&mut packet);\n    return value;",
        ),
        false,
    );
}

#[test]
fn conditional_and_nested_direct_commit_still_may_retire() {
    let conditional = BAD
        .replace(
            "fn reset(target:&mut P)",
            "fn reset(target:&mut P,change:bool)",
        )
        .replace(
            "target.owner=R{code:66};",
            "if change{target.owner=R{code:66};}",
        )
        .replace("reset(&mut packet)", "reset(&mut packet,false)");
    // No interprocedural boolean specialization is claimed. There is an
    // executable Commit in this callee even if this invocation passes false.
    check(&conditional, true);
    let nested = BAD.replace("fn reset(target:&mut P)", "fn leaf(target:&mut P)");
    check(
        &format!(
            "{nested}fn reset(target:&mut P){{relay(target);}}fn relay(target:&mut P){{leaf(target);}}"
        ),
        true,
    );
}

#[test]
fn actual_argument_root_and_owned_field_path_are_not_type_aliases() {
    let different = BAD.replace(
        "    let old=packet.owner;",
        "    var other=P{owner:R{code:67},n:7};\n    let old=other.owner;",
    );
    check(&different, false);
    let two = BAD
        .replace(
            "package P{manages owner:R;n:i32}",
            "package P{manages owner:R;manages second:R;n:i32}",
        )
        .replace(
            "P{owner:R{code:65},n:0}",
            "P{owner:R{code:65},second:R{code:67},n:0}",
        )
        .replace("let old=packet.owner;", "let old=packet.second;");
    check(&two, false);
    check(
        &two.replace("target.owner=R{code:66};", "target.second=R{code:66};"),
        true,
    );
    let parameters = BAD
        .replace(
            "fn reset(target:&mut P)",
            "fn reset(unused:&mut P,target:&mut P)",
        )
        .replace(
            "    let old=packet.owner;",
            "    var other=P{owner:R{code:67},n:7};\n    let old=packet.owner;",
        )
        .replace("reset(&mut packet)", "reset(&mut packet,&mut other)");
    check(&parameters, false);
    check(
        &parameters.replace(
            "reset(&mut packet,&mut other)",
            "reset(&mut other,&mut packet)",
        ),
        true,
    );
}

#[test]
fn saved_desc_and_borrowed_closure_carriers_remain_live() {
    check(
        &BAD.replace(
            "    reset(&mut packet);",
            "    let alias=old;reset(&mut packet);",
        )
        .replace("return old.code;", "return alias.code;"),
        true,
    );
    for borrowed in [false, true] {
        let saved = if borrowed {
            "let saved=()->i32=>old.code;let borrowed=&saved;"
        } else {
            "let saved=()->i32=>old.code;"
        };
        let text = BAD
            .replace(
                "    reset(&mut packet);",
                &format!("    {saved}reset(&mut packet);"),
            )
            .replace(
                "return old.code;",
                if borrowed {
                    "return borrowed();"
                } else {
                    "return saved();"
                },
            );
        check(&text, true);
    }
}

#[test]
fn indirect_pointer_stays_closed_and_capture_checks_actual_retirement() {
    let pointer = BAD.replace("reset(&mut packet);", "let f=reset;f(&mut packet);");
    let report = compile_source(&source(&pointer), &CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("indirect address contracts"))
    );
    let callback = format!(
        "{PREFIX}fn reset(target:&mut P){{target.owner=R{{code:66}};}}fn run<F:FnMut(i32)->i32>(f:&mut F)->i32{{return f(0);}}fn main()->i32{{var packet=P{{owner:R{{code:65}},n:0}};let old=packet.owner;let parent=&mut packet;var f=move(x:i32)->i32=>{{reset(parent);return x;}};run(&mut f);return old.code;}}"
    );
    // The capture profile is now implemented. This unsafe program must reach
    // the real retirement proof instead of the old Types capability fence.
    check(&callback, true);
}

#[test]
fn native_nonretiring_writes_and_new_instance_description_remain_usable() {
    let readonly = BAD.replace("target.owner=R{code:66};", "let n=target.n;");
    let copy = BAD
        .replace("target.owner=R{code:66};", "target.n=42;")
        .replace("return old.code;", "return old.code+packet.n;");
    let new_desc = BAD.replace(
        "    let old=packet.owner;\n    reset(&mut packet);",
        "    reset(&mut packet);\n    let old=packet.owner;",
    );
    for (text, expected) in [
        (readonly.as_str(), b"A65\n".as_slice()),
        (copy.as_str(), b"A107\n".as_slice()),
        (new_desc.as_str(), b"AB66\n".as_slice()),
    ] {
        review329_native_entry_output(&[("main", text)], "main", expected);
    }
}

#[test]
fn different_nonlocal_parameters_do_not_prove_instance_apart() {
    let prefix = format!("{PREFIX}fn reset(target:&mut P){{target.owner=R{{code:66}};}}");
    let tail = "fn main()->i32{var packet=P{owner:R{code:65},n:0};let old=packet.owner;return relay(&mut packet,old);}";
    check(
        &format!(
            "{prefix}fn relay(target:&mut P,old:R)->i32{{reset(target);return old.code;}}{tail}"
        ),
        true,
    );
    let apart = "fn main()->i32{var packet=P{owner:R{code:65},n:0};var other=P{owner:R{code:67},n:7};let old=other.owner;return relay(&mut packet,old);}";
    check(
        &format!(
            "{prefix}fn relay(target:&mut P,old:R)->i32{{reset(target);return old.code;}}{apart}"
        ),
        false,
    );
    check(
        &format!(
            "{prefix}fn relay(target:&mut P,old:R)->i32{{let value=old.code;reset(target);return value;}}{tail}"
        ),
        false,
    );
}

#[test]
fn no_commit_is_not_invalidation_but_possible_success_has_no_error_refinement() {
    let old=BAD.replace("fn reset(target:&mut P){target.owner=R{code:66};}","enum F{Bad}fn fail()->F!i32{return F.Bad();}fn reset(target:&mut P)->F!i32{let n=fail()?;return n;}");
    check(&old, false);
    let may = old.replace(
        "let n=fail()?;return n;",
        "target.owner=R{code:fail()?};return 0;",
    );
    // Its failing execution does not Commit, but the independently checked
    // helper also has a possible success/Commit. No exit-sensitive effect here.
    check(&may, true);
    let whole = BAD.replace(
        "target.owner=R{code:66};",
        "*target=P{owner:R{code:66},n:42};",
    );
    // owned replacement.7 admits the endpoint; actual retirement still rejects this use.
    check(&whole, true);
}
