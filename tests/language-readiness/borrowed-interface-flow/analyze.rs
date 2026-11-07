//! Analyze's source-derived lending controls. Included by the existing driver
//! borrow test target after the ordinary trait producer is integrated.
use std::{fs, path::PathBuf};
use zil_base::{
    CompileResult, FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_ir::{
    CoreImport, CoreProgram, InstructionKind, StorageBase, StorageBinder, Type, TypedModule,
};

fn lending_core(
    cursor_change: impl FnOnce(String) -> String,
    limits: &Limits,
) -> CompileResult<CoreProgram> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/language-readiness/borrowed-interface-flow/project");
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
    let expanded = zil_expand::expand_program_with_limits(
        &sources,
        &syntax.iter().collect::<Vec<_>>(),
        &imports,
        limits,
    )?;
    let modules = expanded.into_iter().map(|(module, _)| module).collect();
    Ok(CoreProgram {
        root: ModuleId(0),
        source_fingerprint: sources[0].fingerprint(),
        modules,
    })
}

fn lending_module(cursor_change: impl FnOnce(String) -> String) -> TypedModule {
    let limits = Limits::default();
    let program = lending_core(cursor_change, &limits).unwrap();
    zil_types::check_program(&program, &TargetProfile::host_llvm(), &limits)
        .unwrap()
        .module
}

#[test]
fn source_core_and_types_have_exact_fresh_work_boundaries() {
    let defaults = Limits::default();
    let target = TargetProfile::host_llvm();
    let core = lending_core(|text| text, &defaults).unwrap();
    let typed = zil_types::check_program(&core, &target, &defaults).unwrap();

    let (mut low, mut high) = (1, defaults.max_analysis_steps);
    while low < high {
        let middle = low + (high - low) / 2;
        let limits = Limits {
            max_analysis_steps: middle,
            ..defaults.clone()
        };
        match lending_core(|text| text, &limits) {
            Ok(_) => high = middle,
            Err(diagnostics)
                if diagnostics
                    .iter()
                    .any(|d| d.category == ResultCategory::ResourceLimit) =>
            {
                low = middle + 1
            }
            Err(diagnostics) => panic!("Expand boundary: {diagnostics:?}"),
        }
    }
    assert!(low > 1);
    for (n, success) in [(low, true), (low - 1, false), (low, true)] {
        let limits = Limits {
            max_analysis_steps: n,
            ..defaults.clone()
        };
        if success {
            assert_eq!(lending_core(|text| text, &limits).unwrap(), core);
        } else {
            let diagnostics = lending_core(|text| text, &limits).unwrap_err();
            assert!(
                diagnostics.iter().any(
                    |d| d.phase == Phase::Expand && d.category == ResultCategory::ResourceLimit
                ),
                "{diagnostics:?}"
            );
        }
    }
    eprintln!("G2.4.1 source Core Expand work N={low}; N-1 ResourceLimit; fresh equality");

    let (mut low, mut high) = (1, defaults.max_analysis_steps);
    while low < high {
        let middle = low + (high - low) / 2;
        let limits = Limits {
            max_analysis_steps: middle,
            ..defaults.clone()
        };
        match zil_types::check_program(&core, &target, &limits) {
            Ok(_) => high = middle,
            Err(diagnostics)
                if diagnostics
                    .iter()
                    .any(|d| d.category == ResultCategory::ResourceLimit) =>
            {
                low = middle + 1
            }
            Err(diagnostics) => panic!("Types boundary: {diagnostics:?}"),
        }
    }
    assert!(low > 1);
    for (n, success) in [(low, true), (low - 1, false), (low, true)] {
        let limits = Limits {
            max_analysis_steps: n,
            ..defaults.clone()
        };
        if success {
            assert_eq!(
                zil_types::check_program(&core, &target, &limits).unwrap(),
                typed
            );
        } else {
            let diagnostics = zil_types::check_program(&core, &target, &limits).unwrap_err();
            assert!(diagnostics.iter().any(|d| d.phase == Phase::Types
                && d.category == ResultCategory::ResourceLimit), "{diagnostics:?}");
        }
    }
    eprintln!("G2.4.1 source Core direct Types work N={low}; N-1 ResourceLimit; fresh equality");
}

#[test]
fn analyze_source_actual_option_return_roots() {
    let module = lending_module(|text| text);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let cursor = include_str!("project/cursor.zil");
    let provider_start = cursor.find("fn next<life p>").unwrap();
    assert!(module.functions.iter().any(|f| {
        let actual_source = f.span.file == FileId(2)
            && f.span.start >= provider_start
            && f.span.end <= cursor.len();
        let actual_receiver = f.parameters.len() == 1
            && matches!(
                f.locals[f.parameters[0].0 as usize].ty,
                Type::Address { mutable: true, .. }
            );
        let actual_result = matches!(
            &f.return_type,
            Type::BorrowedStorage { storage, .. }
                if matches!(storage.as_ref(), Type::Enum { variants, .. }
                    if matches!(variants.as_slice(), [(_, empty), (_, some)]
                        if empty.is_empty() && matches!(some.as_slice(), [Type::Address { mutable: false, .. }])))
        );
        let relation = f.storage_life.as_ref().is_some_and(|life| {
            life.fixed_binder_count == 0
                && life.invocation_binder_count == 1
                && matches!(life.bindings.as_slice(), [row]
                    if row.position.base == StorageBase::Parameter(0)
                        && row.position.path.is_empty()
                        && row.binder == StorageBinder::Invocation(0))
                && matches!(life.classes.as_slice(), [row]
                    if row.position.base == StorageBase::Result
                        && row.position.path.is_empty()
                        && row.position.class == 0
                        && row.binder == StorageBinder::Invocation(0))
        });
        actual_source
            && actual_receiver
            && actual_result
            && relation
            && f.reference_return_bound.is_none()
    }));
    assert!(plan.functions.iter().any(|f| {
        f.holds
            .iter()
            .any(|hold| hold.roots.contains(&zil_ir::HoldRoot::Parameter(0)))
    }));
}

#[test]
fn analyze_rejects_local_payload_hidden_in_source_option_shape() {
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
