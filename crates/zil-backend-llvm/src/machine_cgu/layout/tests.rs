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
                storage_life: None,
                call_kind: zil_ir::CallKind::Direct,
                reference_return_bound: None,
                callable_return_contract: None,
                callable_contract: None,
                protocol_contract: None,
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
    let key = |m: &Module, l: &LirModule, entry, count, split, imports: bool, compiler| {
        let imports = if imports {
            CallImportPolicy::Complete
        } else {
            CallImportPolicy::Disabled
        };
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

fn projection_key(module: &Module, source: &LirModule, count: usize) -> Fingerprint {
    action_key(
        module,
        &source.as_view(),
        EntryPoint::None,
        count,
        true,
        CallImportPolicy::Complete,
        Fingerprint::of(b"projection-test-compiler"),
    )
    .unwrap()
}

#[test]
fn layout_projection_reuses_multi_body_edits_but_replays_every_current_operand() {
    let directory = Directory::new();
    let (mut module, source) = fixture();
    for item in &mut module.items[3..5] {
        let Item::Function(function) = item else {
            unreachable!()
        };
        function.blocks.as_mut().unwrap()[0].instructions.insert(
            0,
            Inst::new(
                Some("constant".into()),
                Op::Freeze(Arg::new(Ty::Int(32), Val::Int(7))),
            ),
        );
    }
    zil_llvm_api::validate_module(&module).unwrap();
    let (_, layout) = planned(&module, &source);
    let key = projection_key(&module, &source, 2);
    let cache = Cache {
        root: directory.0.clone(),
        key,
    };
    cache.pending(&layout).unwrap().pin().unwrap().commit();
    let mut changed = module.clone();
    for (offset, item) in changed.items[3..5].iter_mut().enumerate() {
        let Item::Function(function) = item else {
            unreachable!()
        };
        function.blocks.as_mut().unwrap()[0].instructions[0].op =
            Op::Freeze(Arg::new(Ty::Int(32), Val::Int(100 + offset as i128)));
    }
    let Item::Global(global) = &mut changed.items[6] else {
        unreachable!()
    };
    global.initializer = Some(Val::Int(99));
    zil_llvm_api::validate_module(&changed).unwrap();
    assert_ne!(changed, module);
    assert_eq!(projection_key(&changed, &source, 2), key);
    let loaded = cache.load(&changed, &source.as_view(), 2).unwrap();
    let current = loaded.replay(changed.clone());
    let (uncached, _) = planned(&changed, &source);
    assert_eq!(current, uncached.units);
    assert_ne!(current, layout.replay(module));
    for unit in &current {
        zil_llvm_api::validate_module(unit).unwrap();
    }
    // Body types are not placement inputs. Reusing the recipe must still
    // present a malformed current instruction to the native-unit preflight.
    let Item::Function(function) = &mut changed.items[3] else {
        unreachable!()
    };
    function.blocks.as_mut().unwrap()[0].instructions[0].op =
        Op::Freeze(Arg::new(Ty::Int(0), Val::Int(7)));
    assert_eq!(projection_key(&changed, &source, 2), key);
    let invalid = cache
        .load(&changed, &source.as_view(), 2)
        .unwrap()
        .replay(changed.clone());
    let (uncached, _) = planned(&changed, &source);
    assert_eq!(invalid, uncached.units);
    assert!(
        invalid
            .iter()
            .any(|unit| zil_llvm_api::validate_module(unit).is_err())
    );
}

#[test]
fn layout_projection_binds_address_direct_call_linkage_and_definition_changes() {
    let (mut module, source) = fixture();
    let Item::Function(caller) = &mut module.items[3] else {
        unreachable!()
    };
    caller.blocks.as_mut().unwrap()[0].instructions.insert(
        0,
        Inst::new(
            Some("fp".into()),
            Op::Freeze(Arg::new(Ty::Ptr, Val::Global("zil.fn.1".into()))),
        ),
    );
    zil_llvm_api::validate_module(&module).unwrap();
    let key = projection_key(&module, &source, 2);
    let variants: [fn(&mut Module); 5] = [
        |m| {
            let Item::Function(f) = &mut m.items[3] else {
                unreachable!()
            };
            let Op::Call { callee, .. } = &mut f.blocks.as_mut().unwrap()[0].instructions[1].op
            else {
                unreachable!()
            };
            *callee = Val::Local("fp".into());
        },
        |m| {
            let Item::Function(f) = &mut m.items[3] else {
                unreachable!()
            };
            f.blocks.as_mut().unwrap()[0].instructions[0].op =
                Op::Freeze(Arg::new(Ty::Ptr, Val::Global("foreign".into())));
        },
        |m| {
            let Item::Function(f) = &mut m.items[4] else {
                unreachable!()
            };
            f.linkage = Linkage::AvailableExternally;
        },
        |m| {
            let Item::Function(f) = &mut m.items[4] else {
                unreachable!()
            };
            *f = f.declaration();
        },
        |m| {
            let Item::Global(g) = &mut m.items[6] else {
                unreachable!()
            };
            g.linkage = Linkage::External;
        },
    ];
    for change in variants {
        let mut changed = module.clone();
        change(&mut changed);
        assert_ne!(projection_key(&changed, &source, 2), key);
    }
    let mut addressed = module.clone();
    addressed.items.push(Item::Global(Global {
        name: "address".into(),
        linkage: Linkage::External,
        ty: Ty::Ptr,
        initializer: Some(Val::Global("state".into())),
        constant: false,
        unnamed_addr: false,
        alignment: 8,
    }));
    let key = projection_key(&addressed, &source, 2);
    let Item::Global(g) = addressed.items.last_mut().unwrap() else {
        unreachable!()
    };
    g.initializer = Some(Val::Global("zil.fn.1".into()));
    assert_ne!(projection_key(&addressed, &source, 2), key);
}

#[test]
fn layout_projection_binds_complete_import_cost_threshold() {
    let (mut module, mut source) = fixture();
    module.items = vec![
        Item::Function(function("zil.fn.0", Some("zil.fn.2"))),
        Item::Function(function("zil.fn.1", Some("zil.fn.2"))),
        Item::Function(function("zil.fn.2", None)),
    ];
    for (index, f) in source.functions.iter_mut().enumerate() {
        f.span.file = FileId(index as u32);
    }
    let mut layout = None;
    let plan = partition_view_recorded(
        module.clone(),
        &source.as_view(),
        EntryPoint::None,
        3,
        true,
        true,
        &mut layout,
    );
    assert_eq!(plan.fallback, None);
    let layout = layout.unwrap();
    assert_eq!(
        layout.units.iter().map(|u| u.imports.len()).sum::<usize>(),
        2
    );
    let key = projection_key(&module, &source, 3);
    let Item::Function(leaf) = &mut module.items[2] else {
        unreachable!()
    };
    for index in 0..5 {
        leaf.blocks.as_mut().unwrap()[0].instructions.insert(
            0,
            Inst::new(
                Some(format!("extra{index}")),
                Op::Freeze(Arg::new(Ty::Int(32), Val::Int(index))),
            ),
        );
    }
    zil_llvm_api::validate_module(&module).unwrap();
    assert_ne!(projection_key(&module, &source, 3), key);
    assert!(!layout.valid(&module, &source.as_view(), 3));
    let mut rejected = None;
    let original = super::super::partition_view_once(
        module.clone(),
        &source.as_view(),
        EntryPoint::None,
        3,
        true,
        true,
        &mut rejected,
    );
    assert_eq!(
        original.fallback,
        Some("complete O2 call closure exceeds total copy cost")
    );
    let ordinary = super::super::partition_view(
        module.clone(),
        &source.as_view(),
        EntryPoint::None,
        3,
        true,
        false,
    );
    let current = partition_view_recorded(
        module,
        &source.as_view(),
        EntryPoint::None,
        3,
        true,
        true,
        &mut rejected,
    );
    assert_eq!(current.units, ordinary.units);
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
        CallImportPolicy::Complete,
        None,
    );
    assert_eq!(plan.fallback, Some("invalid machine declaration prelude"));
    assert!(pending.is_none());
}

#[test]
fn layout_cache_keeps_complete_and_resource_helper_policies_separate() {
    let directory = Directory::new();
    let (module, source) = fixture();
    let (plan, layout) = planned(&module, &source);
    let cache = |imports| Cache {
        root: directory.0.clone(),
        key: action_key(
            &module,
            &source.as_view(),
            EntryPoint::None,
            2,
            true,
            imports,
            Fingerprint::of(b"same-compiler"),
        )
        .unwrap(),
    };
    let complete = cache(CallImportPolicy::Complete);
    let helpers = cache(CallImportPolicy::ResourceHelpers);
    assert_ne!(complete.key, helpers.key);
    complete.pending(&layout).unwrap().pin().unwrap().commit();
    assert!(helpers.load(&module, &source.as_view(), 2).is_none());
    helpers.pending(&layout).unwrap().pin().unwrap().commit();
    for policy in [
        CallImportPolicy::Complete,
        CallImportPolicy::ResourceHelpers,
    ] {
        let loaded = cache(policy).load(&module, &source.as_view(), 2).unwrap();
        assert_eq!(loaded.replay(module.clone()), plan.units);
    }
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
