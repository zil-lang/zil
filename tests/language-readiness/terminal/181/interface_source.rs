#[test]
fn terminal_interface_actual_exported_closure_layout_is_bounded() {
    use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
    use zil_ir::{CoreProgram, Type};
    fn program(width: usize, nested: bool, wide: bool) -> CoreProgram {
        let mut text = String::from("pub fn make(n:i32)->impl Fn()->i32{");
        for i in 0..width {
            text.push_str(&format!(
                "let a{i}:{}={}n{};",
                if wide { "i64" } else { "i32" },
                if wide { "i64(" } else { "" },
                if wide { ")" } else { "" }
            ));
        }
        let sum = (0..width)
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join("+");
        let body = if wide { format!("i32({sum})") } else { sum };
        if nested {
            text.push_str(&format!(
                "let inner=move()=>{body};return move()=>inner()}}"
            ));
        } else {
            text.push_str(&format!("return move()=>{body}}}"));
        }
        text.push_str("fn main()->i32{let f=make(1);return f()}");
        let source = SourceUnit::new(FileId(0), ModuleId(0), "wide_export.zil", text);
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let (module, _) = zil_expand::expand_program_module(&source, &syntax, vec![]).unwrap();
        CoreProgram {
            root: ModuleId(0),
            source_fingerprint: module.core.source_fingerprint,
            modules: vec![module],
        }
    }
    fn run_controls() {
        for nested in [false, true] {
            for width in [4, 24] {
                let p = program(width, nested, false);
                let limits = Limits::default();
                let target = TargetProfile::default();
                let typed = zil_types::check_program(&p, &target, &limits).unwrap();
                let export = typed.interfaces[0]
                    .exports
                    .iter()
                    .find(|x| x.name == "make")
                    .unwrap();
                assert!(
                    matches!(export.result, Type::Closure(_)),
                    "{:?}",
                    export.result
                );
                let changed =
                    zil_types::check_program(&program(width, nested, true), &target, &limits)
                        .unwrap();
                assert_ne!(
                    typed.interfaces[0].signature_fingerprint,
                    changed.interfaces[0].signature_fingerprint
                );
                let accepts = |steps| {
                    zil_types::check_program(
                        &p,
                        &target,
                        &Limits {
                            max_analysis_steps: steps,
                            ..limits.clone()
                        },
                    )
                };
                let (mut low, mut high) = (1, limits.max_analysis_steps);
                while low < high {
                    let middle = low + (high - low) / 2;
                    match accepts(middle) {
                        Ok(_) => high = middle,
                        Err(ds) => {
                            assert_eq!(
                                ds[0].category,
                                zil_base::ResultCategory::ResourceLimit,
                                "{ds:?}"
                            );
                            low = middle + 1
                        }
                    }
                }
                let exact = accepts(low).unwrap();
                assert_eq!(
                    exact.interfaces[0].signature_fingerprint,
                    typed.interfaces[0].signature_fingerprint
                );
                assert_eq!(
                    accepts(low - 1).unwrap_err()[0].category,
                    zil_base::ResultCategory::ResourceLimit
                );
                accepts(low).unwrap();
                println!(
                    "width={width} nested={nested} closures={} TypesN={low}",
                    typed.module.closures.len()
                );
            }
        }
    }

    run_controls();
}
