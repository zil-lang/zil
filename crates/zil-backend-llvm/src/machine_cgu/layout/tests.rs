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
        blocks: Some(
            (vec![Block {
                name: "entry".into(),
                instructions,
            }])
            .into(),
        ),
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
                locals: (vec![]).into(),
                values: (vec![]).into(),
                blocks: (vec![]).into(),
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
        items: (vec![
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
        ])
        .into(),
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

#[test]
fn stable_class_layout_publishes_the_actual_plan_only_on_commit() {
    let (_, source) = fixture();
    let mut module = Module {
        triple: source.target.triple.clone(),
        fingerprint: "stable-classes".into(),
        items: vec![
            Item::Function(function("zil.fn.0", Some("zil.fn.1"))),
            Item::Function(function("zil.fn.1", None)),
            Item::Function(function("zil.fn.2", None)),
        ]
        .into(),
    };
    let (_, classes) =
        crate::machine_call_classes::canonicalize_stable(&mut module, &BTreeSet::new(), &|| false)
            .unwrap();
    assert!(!classes.is_empty());
    let dir = Directory::new();
    let build = || {
        crate::machine_cgu::partition_classes_reusing(
            module.clone(),
            &source.as_view(),
            EntryPoint::None,
            2,
            &classes,
            Some(&dir.0),
            None,
        )
        .unwrap()
    };
    let first = build();
    assert!(!first.reused_layout);
    let plan = first.plan;
    drop(first.pending.unwrap());
    assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 0);
    let planned = build();
    let reported_key = planned.layout_key.unwrap();
    let pending = planned.pending.unwrap().pin().unwrap();
    let path = pending.path().to_owned();
    assert!(!path.exists());
    pending.commit();
    let bytes = fs::read(path).unwrap();
    assert_eq!(&bytes[..8], MAGIC);
    assert_eq!(Fingerprint::of(&bytes[HEADER..]).0, bytes[40..HEADER]);
    let actual = Layout::decode(&bytes[HEADER..]).unwrap();
    assert_eq!(actual.replay(module.clone()), plan.units);
    let ordinary = Cache::new(
        &dir.0,
        &module,
        &source.as_view(),
        EntryPoint::None,
        2,
        true,
        CallImportPolicy::Complete,
    )
    .unwrap();
    let ordinary_key = ordinary.key;
    let class_key = Cache::new(
        &dir.0,
        &module,
        &source.as_view(),
        EntryPoint::None,
        2,
        true,
        CallImportPolicy::Complete,
    )
    .unwrap()
    .with_classes(&classes)
    .key;
    assert_ne!(ordinary_key, class_key);
    assert_eq!(bytes[8..40], class_key.0);
    assert_eq!(reported_key, class_key.to_hex());
    assert!(ordinary.load(&module, &source.as_view(), 2).is_none());
    let restored = build();
    assert!(restored.reused_layout);
    assert!(restored.pending.is_none());
    assert_eq!(restored.layout_key.as_deref(), Some(reported_key.as_str()));
    assert_eq!(restored.plan.units, plan.units);
    assert!(restored.materialization.is_some());
}

fn stable_fixture() -> (Module, LirModule, BTreeSet<String>) {
    let (_, mut source) = fixture();
    for id in 3..5 {
        let mut function = source.functions[0].clone();
        function.id = FunctionId(id);
        function.name = format!("f{id}");
        function.span.file = FileId(id % 2);
        source.functions.push(function);
    }
    let mut items = vec![Item::Function(function("zil.fn.0", Some("zil.fn.1")))];
    for id in 1..5 {
        let mut body = function(&format!("zil.fn.{id}"), (id == 3).then_some("zil.fn.1"));
        if id != 3 {
            body.blocks.as_mut().unwrap()[0].instructions[0].op =
                Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(id * 4 + 3))));
        }
        items.push(Item::Function(body));
    }
    items.push(Item::Function(
        function("unused.foreign", None).declaration(),
    ));
    let mut module = Module {
        triple: source.target.triple.clone(),
        fingerprint: "class-producer".into(),
        items: items.into(),
    };
    let (_, classes) =
        crate::machine_call_classes::canonicalize_stable(&mut module, &BTreeSet::new(), &|| false)
            .unwrap();
    assert!(!classes.is_empty());
    zil_llvm_api::validate_module(&module).unwrap();
    (module, source, classes)
}

fn class_cache(
    directory: &Directory,
    module: &Module,
    source: &LirModule,
    classes: &BTreeSet<String>,
) -> Cache {
    Cache::new(
        &directory.0,
        module,
        &source.as_view(),
        EntryPoint::None,
        2,
        true,
        CallImportPolicy::Complete,
    )
    .unwrap()
    .with_classes(classes)
}

#[test]
fn class_disk_restart_replays_current_values_and_restores_materialization() {
    let (module, source, classes) = stable_fixture();
    let directory = Directory::new();
    let build = |module: Module, previous: Option<&super::super::Materialization>| {
        super::super::partition_classes_reusing(
            module,
            &source.as_view(),
            EntryPoint::None,
            2,
            &classes,
            Some(&directory.0),
            previous,
        )
        .unwrap()
    };
    let first = build(module.clone(), None);
    assert!(!first.reused_layout);
    let expected_key = first.layout_key.clone();
    first.pending.unwrap().pin().unwrap().commit();
    let mut current = module.clone();
    current.fingerprint = "current-independent-verification".into();
    let restored = build(current.clone(), None);
    assert!(restored.reused_layout);
    assert!(restored.pending.is_none());
    assert_eq!(restored.layout_key, expected_key);
    assert!(
        restored
            .plan
            .units
            .iter()
            .all(|unit| unit.fingerprint == current.fingerprint)
    );
    for unit in &restored.plan.units {
        zil_llvm_api::validate_module(unit).unwrap();
    }
    let cache = class_cache(&directory, &current, &source, &classes);
    let (layout, _) = cache
        .load_classes(&current, &source.as_view(), EntryPoint::None, 2, &classes)
        .unwrap();
    assert_eq!(restored.plan.units, layout.replay(current.clone()));
    assert_eq!(
        restored.materialization,
        Some(super::super::Materialization::new(
            current.clone(),
            &layout,
            &restored.plan.units,
            0
        ))
    );
    let reused = build(current, restored.materialization.as_ref());
    assert!(reused.reused_layout);
    assert!(reused.materialization.as_ref().unwrap().reused_items > 0);
    assert_eq!(reused.plan.units, restored.plan.units);

    // Body changes must select a different actual current key, not the disk
    // recipe's old bodies; replay never deserializes a machine body.
    let mut changed = module;
    let Item::Function(function) = &mut changed.items[2] else {
        unreachable!()
    };
    function.blocks.as_mut().unwrap()[0].instructions[0].op =
        Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(29))));
    let rebuilt = build(changed, reused.materialization.as_ref());
    assert!(!rebuilt.reused_layout);
    assert_ne!(rebuilt.layout_key, expected_key);
    assert!(rebuilt.pending.is_some());
}

#[test]
fn class_cache_rejects_checksummed_partial_extra_and_unreferenced_roles() {
    let (module, source, classes) = stable_fixture();
    let directory = Directory::new();
    let build = || {
        super::super::partition_classes_reusing(
            module.clone(),
            &source.as_view(),
            EntryPoint::None,
            2,
            &classes,
            Some(&directory.0),
            None,
        )
        .unwrap()
    };
    let first = build();
    first.pending.unwrap().pin().unwrap().commit();
    let cache = || class_cache(&directory, &module, &source, &classes);
    let (layout, _) = cache()
        .load_classes(&module, &source.as_view(), EntryPoint::None, 2, &classes)
        .unwrap();
    let mut missing = layout.clone();
    let unit = missing
        .units
        .iter_mut()
        .find(|unit| !unit.imports.is_empty())
        .unwrap();
    let removed = unit.imports.pop().unwrap();
    unit.declarations.push(removed);
    unit.declarations.sort_unstable();

    let mut extra = layout.clone();
    let (owner, index) = extra
        .units
        .iter()
        .enumerate()
        .find_map(|(owner, unit)| {
            module.items.iter().enumerate().find_map(|(index, item)| {
                (matches!(item, Item::Function(f) if f.blocks.is_some() && f.name == "zil.fn.3")
                    && unit.definitions.binary_search(&index).is_err()
                    && unit.imports.binary_search(&index).is_err())
                .then_some((owner, index))
            })
        })
        .unwrap();
    extra.units[owner].imports.push(index);
    extra.units[owner].imports.sort_unstable();

    let mut prelude = layout.clone();
    let unused = module
        .items
        .iter()
        .position(|item| matches!(item, Item::Function(f) if f.name == "unused.foreign"))
        .unwrap();
    prelude.units[0].prelude.push(unused);
    prelude.units[0].prelude.sort_unstable();

    for invalid in [missing, extra, prelude] {
        assert!(invalid.valid_in_domain(&module, &source.as_view(), 2, Some(&classes)));
        assert!(
            invalid
                .valid_classes(&module, &source.as_view(), EntryPoint::None, 2, &classes)
                .is_none()
        );
        // Commit a correct checksum/current key around the wrong graph roles.
        // The envelope alone must not admit incomplete or gratuitous imports.
        cache().pending(&invalid).unwrap().pin().unwrap().commit();
        assert!(
            cache()
                .load_classes(&module, &source.as_view(), EntryPoint::None, 2, &classes)
                .is_none()
        );
        let fallback = build();
        assert!(!fallback.reused_layout);
        assert_eq!(fallback.plan.units, first.plan.units);
        assert_eq!(fallback.layout_key, first.layout_key);
        fallback.pending.unwrap().pin().unwrap().commit();
        assert!(build().reused_layout);
    }
}

#[test]
fn class_restore_keeps_original_source_copy_bound_and_actual_definition_domain() {
    let (module, source, classes) = stable_fixture();
    let mut recorded = None;
    let plan = super::super::partition_view_once(
        module.clone(),
        &source.as_view(),
        EntryPoint::None,
        2,
        true,
        true,
        &mut recorded,
        None,
        &mut 0,
        &classes,
    );
    assert!(plan.fallback.is_none());
    let layout = recorded.unwrap();
    // Real classes imported by the plan do not become authorized merely from
    // their generated spelling when the actual class domain is absent.
    assert!(
        layout
            .valid_classes(
                &module,
                &source.as_view(),
                EntryPoint::None,
                2,
                &BTreeSet::new()
            )
            .is_none()
    );
    let mut overflow = layout.clone();
    for unit in &mut overflow.units {
        unit.declarations.clear();
        unit.imports = module
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                (matches!(item, Item::Function(f) if f.blocks.is_some())
                    && unit.definitions.binary_search(&index).is_err())
                .then_some(index)
            })
            .collect();
    }
    assert!(!overflow.valid_in_domain(&module, &source.as_view(), 2, Some(&classes)));
    assert!(
        overflow
            .valid_classes(&module, &source.as_view(), EntryPoint::None, 2, &classes)
            .is_none()
    );
    let mut absent = classes.clone();
    absent.insert("zil.class.not-an-actual-definition".into());
    assert!(
        layout
            .valid_classes(&module, &source.as_view(), EntryPoint::None, 2, &absent)
            .is_none()
    );
    let mut declaration = module.clone();
    let class = declaration
        .items
        .iter_mut()
        .find_map(|item| match item {
            Item::Function(f) if classes.contains(&f.name) => Some(f),
            _ => None,
        })
        .unwrap();
    class.blocks = None;
    assert!(
        layout
            .valid_classes(
                &declaration,
                &source.as_view(),
                EntryPoint::None,
                2,
                &classes
            )
            .is_none()
    );
}

#[test]
fn class_restore_rejects_split_local_state_and_retains_address_declarations() {
    let (module, source) = fixture();
    let (_, layout) = planned(&module, &source);
    let classes = BTreeSet::new();
    assert!(
        layout
            .valid_classes(&module, &source.as_view(), EntryPoint::None, 2, &classes)
            .is_some()
    );
    let mut split = layout.clone();
    split.units[0].definitions.retain(|&index| index != 6);
    split.units[1].definitions.push(6);
    assert!(split.valid_in_domain(&module, &source.as_view(), 2, Some(&classes)));
    assert!(
        split
            .valid_classes(&module, &source.as_view(), EntryPoint::None, 2, &classes)
            .is_none()
    );

    let mut addressed = module;
    let Item::Function(user) = &mut addressed.items[5] else {
        unreachable!()
    };
    user.blocks.as_mut().unwrap()[0].instructions[0].op =
        Op::Return(Some(Arg::new(Ty::Ptr, Val::Global("zil.fn.1".into()))));
    zil_llvm_api::validate_module(&addressed).unwrap();
    assert!(
        layout
            .valid_classes(&addressed, &source.as_view(), EntryPoint::None, 2, &classes)
            .is_some()
    );
    let mut no_import = layout;
    no_import.units[0].imports.clear();
    // An address prototype still cannot replace the independent direct-call
    // implementation import required by zil.fn.0 in the same current unit.
    no_import.units[0].declarations.push(4);
    assert!(
        no_import
            .valid_classes(&addressed, &source.as_view(), EntryPoint::None, 2, &classes)
            .is_none()
    );
}

#[test]
fn class_cache_key_binds_class_set_and_current_lir_projection() {
    let (module, mut source, classes) = stable_fixture();
    let directory = Directory::new();
    let cache = class_cache(&directory, &module, &source, &classes);
    let key = cache.key;
    let mut recorded = None;
    super::super::partition_view_once(
        module.clone(),
        &source.as_view(),
        EntryPoint::None,
        2,
        true,
        true,
        &mut recorded,
        None,
        &mut 0,
        &classes,
    );
    cache
        .pending(&recorded.unwrap())
        .unwrap()
        .pin()
        .unwrap()
        .commit();
    let mut other_classes = classes.clone();
    other_classes.pop_first();
    let changed = class_cache(&directory, &module, &source, &other_classes);
    assert_ne!(changed.key, key);
    assert!(
        changed
            .load_classes(
                &module,
                &source.as_view(),
                EntryPoint::None,
                2,
                &other_classes
            )
            .is_none()
    );
    source.functions[0].span.file = FileId(9);
    let changed = class_cache(&directory, &module, &source, &classes);
    assert_ne!(changed.key, key);
    assert!(
        changed
            .load_classes(&module, &source.as_view(), EntryPoint::None, 2, &classes)
            .is_none()
    );
}

#[test]
fn class_cache_cannot_hide_invalid_unused_declarations_or_clone_collisions() {
    let (module, source, classes) = stable_fixture();
    let directory = Directory::new();
    let first = super::super::partition_classes_reusing(
        module.clone(),
        &source.as_view(),
        EntryPoint::None,
        2,
        &classes,
        Some(&directory.0),
        None,
    )
    .unwrap();
    first.pending.unwrap().pin().unwrap().commit();
    let (layout, _) = class_cache(&directory, &module, &source, &classes)
        .load_classes(&module, &source.as_view(), EntryPoint::None, 2, &classes)
        .unwrap();
    let mut invalid = module.clone();
    let unused = invalid
        .items
        .iter_mut()
        .find_map(|item| match item {
            Item::Function(f) if f.name == "unused.foreign" => Some(f),
            _ => None,
        })
        .unwrap();
    unused.linkage = Linkage::Internal;
    assert!(zil_llvm_api::validate_module(&invalid).is_err());
    // The bad declaration is absent from every unit, but the original current
    // declaration preflight must still reject even a correctly framed record.
    class_cache(&directory, &invalid, &source, &classes)
        .pending(&layout)
        .unwrap()
        .pin()
        .unwrap()
        .commit();
    assert!(
        super::super::partition_classes_reusing(
            invalid,
            &source.as_view(),
            EntryPoint::None,
            2,
            &classes,
            Some(&directory.0),
            None,
        )
        .is_none()
    );

    let unit = layout
        .units
        .iter()
        .find(|unit| !unit.imports.is_empty())
        .unwrap();
    let Item::Function(imported) = &module.items[unit.imports[0]] else {
        unreachable!()
    };
    let clone_name = format!("zil.import.{}.{}", unit.owner, imported.name);
    let mut collision = module;
    collision
        .items
        .push(Item::Function(function(&clone_name, None)));
    let mut complete_ownership = layout.clone();
    complete_ownership.units[0]
        .definitions
        .push(collision.items.len() - 1);
    assert!(zil_llvm_api::validate_module(&collision).is_ok());
    assert!(
        complete_ownership
            .valid_classes(&collision, &source.as_view(), EntryPoint::None, 2, &classes,)
            .is_none()
    );
}
