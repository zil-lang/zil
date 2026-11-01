use super::*;
use crate::machine_cgu::{partition_view_cached, partition_view_recorded};
use zil_base::{FileId, ModuleId, Span, TargetProfile};
use zil_ir::{BlockId, FunctionId, LirModule, PassingMode, Type, TypedFunction};
use zil_llvm_api::model::*;

fn function(name: &str, callee: Option<&str>) -> Function {
    let mut instructions = Vec::new();
    let value = if let Some(callee) = callee {
        instructions.push(Inst::new(
            Some("v".into()),
            Op::Call {
                result: Ty::Int(32),
                result_attrs: vec![],
                callee: Val::Global(callee.into()),
                args: vec![],
                vararg: None,
            },
        ));
        Val::Local("v".into())
    } else {
        Val::Int(7)
    };
    instructions.push(Inst::new(
        None,
        Op::Return(Some(Arg::new(Ty::Int(32), value))),
    ));
    Function {
        name: name.into(),
        linkage: Linkage::External,
        signature: Signature::new(Ty::Int(32), vec![]),
        attrs: vec![],
        blocks: Some(vec![Block {
            name: "entry".into(),
            instructions,
        }]),
    }
}
fn fixture() -> (Module, LirModule) {
    let source = LirModule {
        schema_version: zil_base::SCHEMA_VERSION,
        module_id: ModuleId(0),
        verified_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        functions: (0..3)
            .map(|id| TypedFunction {
                callable_return_contract: None,
                callable_contract: None,
                id: FunctionId(id),
                name: format!("f{id}"),
                foreign: None,
                environment: None,
                parameters: vec![],
                return_type: Type::I32,
                result_mode: PassingMode::Infer,
                locals: vec![],
                values: vec![],
                blocks: vec![],
                entry: BlockId(0),
                span: Span {
                    file: FileId(id % 2),
                    ..Span::default()
                },
            })
            .collect(),
        resources: vec![],
        closures: vec![],
        layouts: vec![],
    };
    let mut local_user = function("zil.fn.2", None);
    local_user.signature.result = Ty::Ptr;
    local_user.blocks.as_mut().unwrap()[0].instructions[0] = Inst::new(
        None,
        Op::Return(Some(Arg::new(Ty::Ptr, Val::Global("state".into())))),
    );
    let module = Module {
        triple: source.target.triple.clone(),
        fingerprint: "current".into(),
        items: vec![
            Item::NamedType("shape".into(), vec![Ty::Int(32)]),
            Item::Function(function("foreign", None).declaration()),
            Item::Function(function("unused", None).declaration()),
            Item::Function(function("zil.fn.0", Some("zil.fn.1"))),
            Item::Function(function("zil.fn.1", Some("foreign"))),
            Item::Function(local_user),
            Item::Global(Global {
                name: "state".into(),
                linkage: Linkage::Private,
                ty: Ty::Int(32),
                initializer: Some(Val::Int(3)),
                constant: false,
                unnamed_addr: false,
                alignment: 4,
            }),
        ],
    };
    (module, source)
}
fn planned(module: &Module, source: &LirModule) -> (super::super::Plan, Layout) {
    let mut layout = None;
    let plan = partition_view_recorded(
        module.clone(),
        &source.as_view(),
        EntryPoint::None,
        2,
        true,
        true,
        &mut layout,
    );
    assert_eq!(plan.fallback, None);
    (plan, layout.unwrap())
}
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-cgu-layout-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn exact_layout_replay_preserves_imports_addresses_and_item_order() {
    let (module, source) = fixture();
    zil_llvm_api::validate_module(&module).unwrap();
    let (plan, layout) = planned(&module, &source);
    let bytes = layout.encode().unwrap();
    let decoded = Layout::decode(&bytes).unwrap();
    assert!(decoded.valid(&module, &source.as_view(), 2));
    assert_eq!(layout, decoded);
    assert_eq!(decoded.replay(module.clone()), plan.units);
    assert_eq!(
        decoded.units[0],
        Unit {
            owner: 0,
            prelude: vec![0, 1],
            declarations: vec![],
            imports: vec![4],
            definitions: vec![3, 5, 6]
        }
    );
    assert_eq!(
        decoded.units[1],
        Unit {
            owner: 1,
            prelude: vec![0, 1],
            declarations: vec![],
            imports: vec![],
            definitions: vec![4]
        }
    );
    for unit in &plan.units {
        zil_llvm_api::validate_module(unit).unwrap();
    }
    let text = plan.units[0].to_text();
    assert!(text.contains("@zil.import.0.zil.fn.1"));
    assert!(text.contains("@state"));
    assert!(!text.contains("@unused"));
    let mut current = module;
    current.fingerprint = "new-current-diagnostic".into();
    assert!(
        decoded
            .replay(current)
            .iter()
            .all(|unit| unit.fingerprint == "new-current-diagnostic")
    );
}

#[test]
fn layout_key_binds_current_model_planning_projection_and_policy() {
    let (module, mut source) = fixture();
    let compiler = Fingerprint::of(b"this-compiler");
    let key = |m: &Module, l: &LirModule, entry, count, split, imports, compiler| {
        action_key(m, &l.as_view(), entry, count, split, imports, compiler).unwrap()
    };
    let original = key(&module, &source, EntryPoint::None, 2, true, true, compiler);
    let variants = [
        key(&module, &source, EntryPoint::Main, 2, true, true, compiler),
        key(
            &module,
            &source,
            EntryPoint::ReturnValue(FunctionId(0)),
            2,
            true,
            true,
            compiler,
        ),
        key(&module, &source, EntryPoint::None, 3, true, true, compiler),
        key(&module, &source, EntryPoint::None, 2, false, true, compiler),
        key(&module, &source, EntryPoint::None, 2, true, false, compiler),
        key(
            &module,
            &source,
            EntryPoint::None,
            2,
            true,
            true,
            Fingerprint::ZERO,
        ),
    ];
    assert!(variants.iter().all(|&changed| changed != original));
    let mut changed = module.clone();
    changed.items.swap(3, 4);
    assert_ne!(
        key(&changed, &source, EntryPoint::None, 2, true, true, compiler),
        original
    );
    source.functions[0].span.file = FileId(7);
    assert_ne!(
        key(&module, &source, EntryPoint::None, 2, true, true, compiler),
        original
    );
    source.functions[0].span.file = FileId(0);
    source.functions[0].blocks.push(zil_ir::BasicBlock {
        id: BlockId(0),
        instructions: vec![zil_ir::Instruction {
            result: None,
            span: Span::default(),
            kind: InstructionKind::Call {
                function: FunctionId(1),
                arguments: vec![],
            },
        }],
        terminator: zil_ir::Terminator::Return(None),
    });
    let call_key = key(&module, &source, EntryPoint::None, 2, true, true, compiler);
    source.functions[0].blocks[0].instructions[0].kind = InstructionKind::Call {
        function: FunctionId(2),
        arguments: vec![],
    };
    assert_ne!(
        key(&module, &source, EntryPoint::None, 2, true, true, compiler),
        call_key
    );
}

#[test]
fn layout_records_are_bounded_and_require_complete_current_ownership() {
    let (module, source) = fixture();
    let (_, layout) = planned(&module, &source);
    let bytes = layout.encode().unwrap();
    assert!(Layout::decode(&bytes[..bytes.len() - 1]).is_none());
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(Layout::decode(&trailing).is_none());
    let mut oversized = bytes;
    oversized[..4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(Layout::decode(&oversized).is_none());
    let changes: [fn(&mut Layout); 5] = [
        |l| {
            l.units[0].definitions.pop();
        },
        |l| l.units[0].definitions.push(4),
        |l| l.units[0].prelude = vec![1],
        |l| l.units[0].imports = vec![6],
        |l| l.units[1].declarations = vec![6],
    ];
    for change in changes {
        let mut corrupt = layout.clone();
        change(&mut corrupt);
        assert!(!corrupt.valid(&module, &source.as_view(), 2));
    }
    let mut invalid = module;
    let Item::Function(unused) = &mut invalid.items[2] else {
        unreachable!()
    };
    unused.linkage = Linkage::Internal;
    let (plan, pending) = partition_view_cached(
        invalid,
        &source.as_view(),
        EntryPoint::None,
        2,
        true,
        true,
        None,
    );
    assert_eq!(plan.fallback, Some("invalid machine declaration prelude"));
    assert!(pending.is_none());
}

#[test]
fn layout_cache_publishes_only_on_commit_and_corruption_falls_back() {
    let directory = Directory::new();
    let (module, source) = fixture();
    let cache = || Cache {
        root: directory.0.clone(),
        key: Fingerprint::of(b"exact-test-key"),
    };
    let (plan, layout) = planned(&module, &source);
    let pending = cache().pending(&layout).unwrap().pin().unwrap();
    let path = pending.path().to_owned();
    assert!(path.is_absolute());
    drop(pending);
    assert!(!path.exists());
    cache().pending(&layout).unwrap().pin().unwrap().commit();
    let loaded = cache().load(&module, &source.as_view(), 2).unwrap();
    assert_eq!(loaded.replay(module.clone()), plan.units);
    let mut corrupt = fs::read(&path).unwrap();
    *corrupt.last_mut().unwrap() ^= 1;
    fs::write(&path, corrupt).unwrap();
    assert!(cache().load(&module, &source.as_view(), 2).is_none());
    let mut incomplete = layout;
    incomplete.units[0].definitions.pop();
    cache()
        .pending(&incomplete)
        .unwrap()
        .pin()
        .unwrap()
        .commit();
    assert!(cache().load(&module, &source.as_view(), 2).is_none());
}
