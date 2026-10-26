use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};

fn expanded(text: &str, limits: &Limits) -> zil_base::CompileResult<zil_expand::ExpandedModule> {
    let input = SourceUnit::new(FileId(0), ModuleId(0), "impl090-formation.zil", text);
    let syntax = zil_syntax::parse(&input).into_result().unwrap();
    zil_expand::expand_with_map_limits(&input, &syntax, limits)
}

#[test]
fn unused_slice_aliases_and_templates_cannot_hide_fixed_noncopy_elements() {
    for text in [
        "type Bad=[&i32];fn result(){}",
        "type Bad=[[i32]];fn result(){}",
        "type Bad=[(&i32,i32)];fn result(){}",
        "type Bad=[[&i32;2]];fn result(){}",
        "type Ref=&i32;type Bad=[Ref];fn result(){}",
        "type Row=[i32];type Bad=[Row];fn result(){}",
        "fn unused<T>(s:&[&T]){}fn result(){}",
        "fn unused<T>(s:&[[T]]){}fn result(){}",
    ] {
        let errors = expanded(text, &Limits::default()).unwrap_err();
        assert_eq!(
            errors[0].category,
            ResultCategory::Unsupported,
            "{text}: {errors:#?}"
        );
        assert_eq!(errors[0].phase, Phase::Expand, "{text}: {errors:#?}");
        assert_eq!(errors[0].code, "E-UNSUPPORTED", "{text}: {errors:#?}");
    }
}

#[test]
fn fixed_copy_shapes_and_unresolved_nominals_keep_the_ordinary_resolution_path() {
    for text in [
        "type Bytes=[u8];fn count(s:&Bytes)->i64{return s.len();}",
        "type Rows=[[i32;2]];fn result(){}",
        "type Pairs=[(i32,bool)];fn result(){}",
        "struct P{x:i32;}type Values=[P];fn result(){}",
        "struct Marker<T>{}type Values=[Marker<&i32>];fn result(){}",
        "fn unused<T>(s:&[T]){}fn result(){}",
    ] {
        expanded(text, &Limits::default()).unwrap_or_else(|d| panic!("{text}: {d:#?}"));
    }
}

#[test]
fn formation_walk_consumes_existing_depth_node_and_work_budgets() {
    let text = "type Bad=[((((((&i32))))))];fn result(){}";
    for limits in [
        Limits {
            max_call_depth: 4,
            ..Limits::default()
        },
        Limits {
            max_nodes: 0,
            ..Limits::default()
        },
        Limits {
            max_analysis_steps: 0,
            ..Limits::default()
        },
    ] {
        let errors = expanded(text, &limits).unwrap_err();
        assert_eq!(
            errors[0].category,
            ResultCategory::ResourceLimit,
            "{errors:#?}"
        );
        assert_eq!(errors[0].phase, Phase::Expand, "{errors:#?}");
    }
}
