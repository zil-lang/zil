//! Analyze's source-derived lending controls. Included by the existing driver
//! borrow test target after the ordinary trait producer is integrated.
use std::{fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::{CoreImport, CoreProgram, InstructionKind, TypedModule};

fn lending_module(cursor_change: impl FnOnce(String) -> String) -> TypedModule {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/language-readiness/G2.4.1/project");
    let names = ["main", "api", "cursor", "consumer", "other"];
    let mut sources = names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let path = root.join(format!("{name}.zil"));
            let text = fs::read_to_string(&path).unwrap();
            SourceUnit::new(
                FileId(index as u32),
                ModuleId(index as u32),
                path.to_string_lossy().into_owned(),
                text,
            )
        })
        .collect::<Vec<_>>();
    sources[2].text = cursor_change(sources[2].text.clone());
    let syntax = sources
        .iter()
        .map(|source| zil_syntax::parse(source).into_result().unwrap())
        .collect::<Vec<_>>();
    let import = |name: &str, id| CoreImport {
        alias: name.into(),
        module: ModuleId(id),
        span: zil_base::Span::default(),
    };
    let imports = [
        vec![
            import("api", 1),
            import("cursor", 2),
            import("consumer", 3),
            import("other", 4),
        ],
        vec![],
        vec![import("api", 1)],
        vec![import("api", 1)],
        vec![import("api", 1)],
    ];
    let limits = Limits::default();
    let expanded = zil_expand::expand_program_with_limits(
        &sources,
        &syntax.iter().collect::<Vec<_>>(),
        &imports,
        &limits,
    )
    .unwrap();
    let modules = expanded.into_iter().map(|(module, _)| module).collect();
    let program = CoreProgram {
        root: ModuleId(0),
        source_fingerprint: sources[0].fingerprint(),
        modules,
    };
    zil_types::check_program(&program, &TargetProfile::host_llvm(), &limits)
        .unwrap()
        .module
}

#[test]
fn g2_4_1_analyze_source_actual_option_return_roots() {
    let module = lending_module(|text| text);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    assert!(module.functions.iter().any(|f| {
        f.storage_life.is_some() && f.reference_return_bound.is_none() && f.name.contains("next")
    }));
    assert!(plan.functions.iter().any(|f| {
        f.holds
            .iter()
            .any(|hold| hold.roots.contains(&zil_ir::HoldRoot::Parameter(0)))
    }));
}

#[test]
fn g2_4_1_analyze_rejects_local_payload_hidden_in_source_option_shape() {
    let mut module = lending_module(|text| {
        let original = "return Option::<&'p T>.Some(&self.values[i]);";
        assert!(text.contains(original));
        text.replace(
            original,
            "let alternate:T=self.values[i];let alternative=&alternate;return Option::<&'p T>.Some(&self.values[i]);",
        )
    });
    zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let mut altered = 0;
    for function in &mut module.functions {
        let alternative = function
            .locals
            .iter()
            .find(|local| local.name == "alternate")
            .map(|local| local.id);
        let Some(alternative) = alternative else {
            continue;
        };
        let borrowed = function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .find_map(|instruction| match instruction.kind {
                InstructionKind::BorrowLocal { local } if local == alternative => {
                    instruction.result
                }
                _ => None,
            })
            .expect("source omitted the local address");
        for block in &mut function.blocks {
            for instruction in &mut block.instructions {
                if let InstructionKind::MakeVariant { variant: 1, fields } = &mut instruction.kind
                    && fields.len() == 1
                    && fields[0] != borrowed
                {
                    fields[0] = borrowed;
                    altered += 1;
                }
            }
        }
    }
    assert!(altered > 0, "source omitted an actual Some producer");
    let error = zil_analyze::analyze(&module, &Limits::default()).unwrap_err();
    assert!(
        error.iter().any(|d| d.phase == zil_base::Phase::Analyze
            && d.message.contains("activation-local target")),
        "{error:?}"
    );
}
