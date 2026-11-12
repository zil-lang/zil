use zil_base::{FileId, Limits, ModuleId, SourceUnit, TargetProfile};
use zil_ir::{InstructionKind, Type};

fn module(text: &str) -> zil_ir::TypedModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "qualified-owner-transfer.zil", text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let core = zil_expand::expand_with_map_limits_target_context(
        &source,
        &syntax,
        &limits,
        &target,
        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
            root: source.module_id,
            aliases: &Default::default(),
            declarations: zil_expand::source_context::DeclarationPlacement::Original,
        },
    )
    .unwrap()
    .core;
    zil_types::check(&core, &target, &limits).unwrap()
}

#[test]
fn qualified_child_owner_moves_and_raw_copy_or_missing_transfer_is_rejected() {
    for source in [
        include_str!(
            "../../../tests/fixtures/returned-owning-holders/nested-qualified-callback.zil"
        ),
        include_str!(
            "../../../tests/fixtures/returned-owning-holders/inline-nested-qualified-callback.zil"
        ),
    ] {
        let module = module(source);
        let owns=module.closures.iter().map(|d|d.captures.iter().any(|ty|matches!(ty,Type::BorrowedStorage{storage,..} if matches!(storage.as_ref(),Type::Package{..})))).collect::<Vec<_>>();
        let mut sites = Vec::new();
        for (function, f) in module.functions.iter().enumerate() {
            for (block, b) in f.blocks.iter().enumerate() {
                for (instruction, i) in b.instructions.iter().enumerate() {
                    let local = match i.kind {
                        InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                            local
                        }
                        _ => continue,
                    };
                    if matches!(f.locals[local.0 as usize].ty,Type::Closure(id) if owns[id.0 as usize])
                    {
                        assert!(
                            matches!(i.kind, InstructionKind::TakeLocal { .. }),
                            "{}: {:?}",
                            f.name,
                            i.kind
                        );
                        sites.push((function, block, instruction, local));
                    }
                }
            }
        }
        assert!(!sites.is_empty());
        zil_analyze::analyze(&module, &Limits::default()).unwrap();
        for (function, block, instruction, local) in sites {
            let mut copied = module.clone();
            copied.functions[function].blocks[block].instructions[instruction].kind =
                InstructionKind::Load(local);
            let errors = zil_analyze::analyze(&copied, &Limits::default()).unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|e| e.message.contains("Load cannot copy an owned carrier")),
                "{errors:?}"
            );
            let mut missing = module.clone();
            missing.functions[function].blocks[block]
                .instructions
                .remove(instruction);
            assert!(zil_analyze::analyze(&missing, &Limits::default()).is_err());
        }
    }
}
