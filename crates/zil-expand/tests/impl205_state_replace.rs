use zil_base::{Diagnostic, FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_ir::{CallableKind, CoreModule, Type};

const ORIGINAL: &str = include_str!("impl205_fixtures/00_original_state_and_replacement.zil");
const LAST_USE: &str = include_str!("impl205_fixtures/03_mixed_unused_copy_alias_last_use.zil");

fn run(text: &str) -> Result<CoreModule, Vec<Diagnostic>> {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "state205.zil", text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let result = zil_expand::expand_with_map(&source, &syntax);
    if let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
        let out = std::path::PathBuf::from(root).join(source.fingerprint().to_string());
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("source.zil"), text).unwrap();
        std::fs::write(out.join("expand.txt"), format!("{result:#?}")).unwrap();
    }
    result.map(|result| {
        result
            .source_map
            .validate(std::slice::from_ref(&source), &Limits::default())
            .unwrap();
        assert!(result.core.unsupported.is_empty());
        result.core
    })
}

fn header(text: &str, owner: u32, replacement: u32, fields: &[u32], kind: CallableKind) {
    let core = run(text).unwrap();
    let function = core.functions.iter().find(|f| f.name == "replace").unwrap();
    assert!(function.type_parameters.is_empty());
    assert!(function.callable_return.is_none());
    let [state] = function.state_parameters.as_slice() else {
        panic!("{function:#?}");
    };
    let [binding] = state.bindings.as_slice() else {
        panic!("{state:#?}");
    };
    let [direct] = function.callable_parameters.as_slice() else {
        panic!("{function:#?}");
    };
    assert_eq!(state.index, owner);
    assert_eq!(direct.index, replacement);
    assert_eq!(binding.generic_index, 0);
    assert_eq!(direct.generic_index, 0);
    assert_eq!(binding.generic_name, "F");
    assert_eq!(direct.generic_name, binding.generic_name);
    assert_eq!(binding.fields, fields);
    assert_eq!(binding.kind, kind);
    assert_eq!(direct.kind, kind);
    assert_eq!(binding.parameters, direct.parameters);
    assert_eq!(binding.result, Type::I32);
    assert_eq!(direct.result, binding.result);
    assert_eq!(function.parameters[owner as usize].ty, function.return_type);
    assert_ne!(owner, replacement);
}

#[test]
fn original_138_bytes_publish_one_owner_and_one_real_replacement() {
    assert_eq!(ORIGINAL.len(), 138);
    header(ORIGINAL, 0, 1, &[1, 2], CallableKind::Fn);
}

#[test]
fn declaration_positions_and_field_count_are_not_the_single_type_slot() {
    for (bound, kind) in [
        ("Fn", CallableKind::Fn),
        ("FnMut", CallableKind::FnMut),
        ("FnOnce", CallableKind::FnOnce),
    ] {
        for (fields, selectors) in [("a:F", vec![1]), ("a:F;b:F", vec![1, 2])] {
            for (parameters, owner, replacement) in [
                ("state:State<i64,F>,tag:i32,replacement:F", 0, 2),
                ("replacement:F,tag:i32,state:State<i64,F>", 2, 0),
            ] {
                let text = format!(
                    "package State<T,F>{{manages bytes:ByteBuffer;{fields};tag:T}} \
                     fn replace<F:{bound}()->i32>({parameters})->State<i64,F>{{return state}} fn main(){{}}"
                );
                header(&text, owner, replacement, &selectors, kind);
            }
        }
    }
}

#[test]
fn unused_copy_parameter_keeps_the_early_complete_header_obligation() {
    // No concrete ordinary T means this source definition is checked and then
    // omitted. It cannot rely on the eventual Core selector validator.
    let core = run(LAST_USE).unwrap();
    assert!(!core.functions.iter().any(|f| f.name == "replace"));
    for text in [
        LAST_USE.replace("a:F;b:F;tag:T", "a:F;b:F;c:F;tag:T"),
        LAST_USE.replace("a:F;b:F;tag:T", "a:F;b:(F,);tag:T"),
        LAST_USE.replace("replacement:F)", "replacement:F,extra:ByteBuffer)"),
        LAST_USE.replace("->State<T,F>{", "->State<i64,F>{"),
        LAST_USE.replace("F:Fn()->i32", "F:Fn()->&[u8]"),
    ] {
        let errors = run(&text).unwrap_err();
        assert!(errors.iter().all(|d| d.phase == Phase::Expand));
        assert!(
            errors.iter().any(|d| matches!(
                d.category,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            )),
            "{errors:?}"
        );
    }
}

#[test]
fn adjacent_shapes_do_not_use_the_mixed_header_exception() {
    for text in [
        ORIGINAL.replace("replacement:F", "replacement:State<F>"),
        ORIGINAL.replace("replacement:F", "replacement:&F"),
        ORIGINAL.replace("state:State<F>", "state:&mut State<F>"),
        ORIGINAL.replace("replacement:F)", "replacement:F,another:F)"),
    ] {
        let errors = run(&text).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.phase == Phase::Expand && d.category == ResultCategory::Unsupported),
            "{errors:?}"
        );
    }
}
