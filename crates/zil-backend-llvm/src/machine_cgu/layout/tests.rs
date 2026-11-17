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
        imports,
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
        imports: CallImportPolicy::Complete,
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
    let observed = CurrentGraph::read(&current, None).unwrap();
    assert!(
        !restored
            .materialization
            .as_ref()
            .unwrap()
            .has_reuse_inventory()
    );
    assert_eq!(
        restored.materialization,
        Some(
            super::super::Materialization::completed_with_graph(
                current.clone(),
                &layout,
                &restored.plan.units,
                0,
                observed,
                None,
            )
            .defer_grouping(&source.as_view(), EntryPoint::None, 2, &classes)
        )
    );
    let reused = build(current, restored.materialization.as_ref());
    assert!(reused.reused_layout);
    assert!(reused.materialization.as_ref().unwrap().reused_items > 0);
    assert_eq!(reused.plan.units, restored.plan.units);
    assert!(
        restored
            .materialization
            .as_ref()
            .unwrap()
            .has_reuse_inventory()
    );

    // Body changes must select a different actual current key, not the disk
    // recipe's old bodies; replay never deserializes a machine body.
    let mut changed = module;
    let Item::Function(function) = &mut changed.items[2] else {
        unreachable!()
    };
    function.blocks.as_mut().unwrap()[0].instructions[0].op =
        Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(29))));
    GRAPH_WORK.with(|work| work.set((0, 0)));
    let rebuilt = build(changed.clone(), reused.materialization.as_ref());
    assert!(rebuilt.reused_layout);
    GRAPH_WORK.with(|work| assert_eq!(work.get(), (1, changed.items.len() - 1)));
    assert_ne!(rebuilt.layout_key, expected_key);
    assert!(rebuilt.pending.is_some());
    assert_eq!(rebuilt.plan.units, layout.replay(changed.clone()));
    let actual_key = class_cache(&directory, &changed, &source, &classes).key;
    assert_eq!(
        rebuilt.layout_key.as_deref(),
        Some(actual_key.to_hex().as_str())
    );
    assert_eq!(rebuilt.pending.as_ref().unwrap().key(), actual_key);
    // A dropped publication cannot mutate the prior product or install a raw
    // machine/layout receipt. A later cold session still plans current values.
    drop(rebuilt.pending);
    assert!(
        class_cache(&directory, &changed, &source, &classes)
            .read()
            .is_none()
    );
    assert_eq!(build(changed, None).plan.units, rebuilt.plan.units);
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
        let graph = CurrentGraph::read(&module, None).unwrap();
        let grouping = Grouping::read(
            &module,
            &source.as_view(),
            EntryPoint::None,
            2,
            &classes,
            &graph,
        );
        let wrong_memory = super::super::Materialization::with_graph(
            module.clone(),
            &invalid,
            &first.plan.units,
            0,
            graph,
            grouping,
        );
        let checked = super::super::partition_classes_reusing(
            module.clone(),
            &source.as_view(),
            EntryPoint::None,
            2,
            &classes,
            Some(&directory.0),
            Some(&wrong_memory),
        )
        .unwrap();
        assert!(!checked.reused_layout);
        assert_eq!(checked.plan.units, first.plan.units);
        let fallback = build();
        assert!(!fallback.reused_layout);
        assert_eq!(fallback.plan.units, first.plan.units);
        assert_eq!(fallback.layout_key, first.layout_key);
        fallback.pending.unwrap().pin().unwrap().commit();
        assert!(build().reused_layout);
    }
}

#[test]
fn retained_class_layout_rebinds_renamed_classes_and_rejects_changed_grouping() {
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
    let previous = first.materialization.as_ref().unwrap();
    let graph = CurrentGraph::read(&module, None).unwrap();
    let grouping = Grouping::read(
        &module,
        &source.as_view(),
        EntryPoint::None,
        2,
        &classes,
        &graph,
    )
    .unwrap();
    let retained = previous.layout_for(&grouping).unwrap();

    let old_name = classes
        .iter()
        .find(|name| graph.rows.iter().any(|row| row.references.contains(*name)))
        .unwrap();
    let new_name = format!("{old_name}.current");
    let replacements = BTreeMap::from([(old_name.clone(), new_name.clone())]);
    let mut renamed = module.clone();
    // Even equal old callers must resolve symbols in the current table.
    for item in &mut renamed.items {
        if let Item::Function(function) = item
            && function.name == *old_name
        {
            function.name = new_name.clone();
        }
    }
    assert!(CurrentGraph::read(&renamed, previous.graph()).is_none());
    for item in &mut renamed.items {
        if let Item::Function(function) = item {
            super::super::rewrite_direct_calls(function, &replacements);
        }
    }
    let mut current_classes = classes.clone();
    current_classes.remove(old_name);
    current_classes.insert(new_name);
    let current = super::super::partition_classes_reusing(
        renamed.clone(),
        &source.as_view(),
        EntryPoint::None,
        2,
        &current_classes,
        Some(&directory.0),
        Some(previous),
    )
    .unwrap();
    assert!(current.reused_layout);
    assert_eq!(current.plan.units, retained.replay(renamed.clone()));
    for unit in &current.plan.units {
        zil_llvm_api::validate_module(unit).unwrap();
    }
    assert_ne!(current.layout_key, first.layout_key);
    assert_eq!(
        current.pending.as_ref().unwrap().key(),
        class_cache(&directory, &renamed, &source, &current_classes).key
    );

    // File identity is a grouping input even if the modulo bucket is unchanged.
    // Current target policy and source-call graph changes likewise require the
    // original planner, rather than accepting equal machine bodies as authority.
    let mut moved = source.clone();
    moved.functions[0].span.file.0 += 2;
    let mut target = source.clone();
    target.target.rule_version.push_str(".current");
    for changed in [moved, target] {
        let actual = super::super::partition_classes_reusing(
            module.clone(),
            &changed.as_view(),
            EntryPoint::None,
            2,
            &classes,
            Some(&directory.0),
            Some(previous),
        )
        .unwrap();
        let fresh = super::super::partition_classes_reusing(
            module.clone(),
            &changed.as_view(),
            EntryPoint::None,
            2,
            &classes,
            Some(&directory.0),
            None,
        )
        .unwrap();
        assert!(!actual.reused_layout);
        assert_eq!(actual.plan.units, fresh.plan.units);
    }
    let mut changed = module.clone();
    let Item::Function(caller) = &mut changed.items[0] else {
        unreachable!()
    };
    let Op::Call { callee, .. } = &mut caller.blocks.as_mut().unwrap()[0].instructions[0].op else {
        unreachable!()
    };
    *callee = Val::Global("zil.fn.4".into());
    let actual = super::super::partition_classes_reusing(
        changed.clone(),
        &source.as_view(),
        EntryPoint::None,
        2,
        &classes,
        Some(&directory.0),
        Some(previous),
    );
    let fresh = super::super::partition_classes_reusing(
        changed,
        &source.as_view(),
        EntryPoint::None,
        2,
        &classes,
        Some(&directory.0),
        None,
    );
    assert_eq!(
        actual.as_ref().map(|result| &result.plan.units),
        fresh.as_ref().map(|result| &result.plan.units)
    );
    assert!(actual.as_ref().is_none_or(|result| !result.reused_layout));
}

#[test]
fn current_graph_reuse_charges_original_observation_budget() {
    let (module, _, _) = stable_fixture();
    let graph = CurrentGraph::read(&module, None).unwrap();
    let required = module.items.len() + graph.rows.iter().map(|row| row.work).sum::<usize>();
    for previous in [None, Some(&graph)] {
        assert!(CurrentGraph::read_bounded(&module, previous, required).is_some());
        assert!(CurrentGraph::read_bounded(&module, previous, required - 1).is_none());
    }
}

fn split_class_fixture(value: i128) -> (Module, LirModule, BTreeSet<String>) {
    let (_, seed, _) = stable_fixture();
    let mut source = seed.clone();
    source.functions = (0..24)
        .map(|id| {
            let mut function = seed.functions[0].clone();
            function.id = FunctionId(id);
            function.name = format!("f{id}");
            function.span.file = FileId(id % 16);
            function
        })
        .collect();
    let items = (0..24)
        .map(|id| {
            let callee = match id {
                0 => Some("zil.fn.1"),
                3 => Some("zil.fn.2"),
                _ => None,
            };
            let mut body = function(&format!("zil.fn.{id}"), callee);
            if callee.is_none() {
                let constant = match id {
                    1 => value,
                    2 => 7,
                    _ => id * 4 + 11,
                };
                body.blocks.as_mut().unwrap()[0].instructions[0].op =
                    Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(constant))));
            }
            Item::Function(body)
        })
        .collect();
    let mut module = Module {
        triple: source.target.triple.clone(),
        fingerprint: format!("split-{value}"),
        items,
    };
    let (_, classes) =
        crate::machine_call_classes::canonicalize_stable(&mut module, &BTreeSet::new(), &|| false)
            .unwrap();
    zil_llvm_api::validate_module(&module).unwrap();
    (module, source, classes)
}

#[test]
fn real_class_split_merge_rebuilds_only_changed_units_after_disk_restart() {
    let directory = Directory::new();
    let (original, source, classes) = split_class_fixture(7);
    let build = |module: Module,
                 classes: &BTreeSet<String>,
                 previous: Option<&super::super::Materialization>| {
        super::super::partition_classes_reusing(
            module,
            &source.as_view(),
            EntryPoint::None,
            16,
            classes,
            Some(&directory.0),
            previous,
        )
        .unwrap()
    };
    let cold = build(original.clone(), &classes, None);
    let original_key = cold.layout_key.clone();
    cold.pending.unwrap().pin().unwrap().commit();
    drop(cold.materialization);
    let restored = build(original.clone(), &classes, None);
    assert!(restored.reused_layout);
    assert_eq!(restored.plan.units.len(), 16);
    let (changed, _, split) = split_class_fixture(19);
    assert!(split.len() > classes.len());
    GRAPH_WORK.with(|work| work.set((0, 0)));
    let edited = build(changed.clone(), &split, restored.materialization.as_ref());
    assert!(edited.reused_layout);
    GRAPH_WORK.with(|work| {
        let (fresh, reused) = work.get();
        assert!(fresh < changed.items.len() / 2);
        assert!(reused > changed.items.len() / 2);
    });
    assert_ne!(edited.layout_key, original_key);
    assert_eq!(
        edited.layout_key.as_deref(),
        Some(edited.pending.as_ref().unwrap().key().to_hex().as_str())
    );
    let fresh = build(changed.clone(), &split, None);
    assert_eq!(edited.plan.units, fresh.plan.units);
    let unchanged = edited
        .plan
        .units
        .iter()
        .filter(|unit| {
            restored
                .plan
                .units
                .iter()
                .any(|old| unit.items.ptr_eq(&old.items))
        })
        .count();
    assert!(unchanged > 0 && unchanged < edited.plan.units.len());
    assert!(edited.materialization.as_ref().unwrap().reused_items > 0);
    for unit in &edited.plan.units {
        zil_llvm_api::validate_module(unit).unwrap();
    }
    // Publication is still a successful-request transaction. Dropping this
    // pending receipt leaves the restored epoch and the current disk key alone.
    let changed_key = edited.pending.as_ref().unwrap().key();
    drop(edited.pending);
    let current_cache = Cache::new(
        &directory.0,
        &changed,
        &source.as_view(),
        EntryPoint::None,
        16,
        true,
        CallImportPolicy::Complete,
    )
    .unwrap()
    .with_classes(&split);
    assert_eq!(current_cache.key(), changed_key);
    assert!(current_cache.read().is_none());
    let merged = build(original, &classes, edited.materialization.as_ref());
    assert!(merged.reused_layout);
    assert_eq!(merged.layout_key, original_key);
    assert_eq!(merged.plan.units, restored.plan.units);
}

#[test]
fn direct_restart_first_edit_derives_current_roles_after_old_exact_key_misses() {
    let directory = Directory::new();
    let (original, source, classes) = split_class_fixture(7);
    let producer = super::super::partition_classes_reusing(
        original.clone(),
        &source.as_view(),
        EntryPoint::None,
        16,
        &classes,
        Some(&directory.0),
        None,
    )
    .unwrap();
    let old_key = producer.pending.as_ref().unwrap().key();
    producer.pending.unwrap().pin().unwrap().commit();
    drop(producer.materialization);
    drop(producer.plan);
    let old_cache = Cache::new(
        &directory.0,
        &original,
        &source.as_view(),
        EntryPoint::None,
        16,
        true,
        CallImportPolicy::Complete,
    )
    .unwrap()
    .with_classes(&classes);
    let old_layout = old_cache.read().unwrap();
    let (edited, _, split) = split_class_fixture(19);
    assert!(split.len() > classes.len());
    let current_cache = || {
        Cache::new(
            &directory.0,
            &edited,
            &source.as_view(),
            EntryPoint::None,
            16,
            true,
            CallImportPolicy::Complete,
        )
        .unwrap()
        .with_classes(&split)
    };
    assert_ne!(old_key, current_cache().key());
    assert!(current_cache().read().is_none());
    let mut recorded = None;
    let expected = super::super::partition_view_once(
        edited.clone(),
        &source.as_view(),
        EntryPoint::None,
        16,
        true,
        true,
        &mut recorded,
        None,
        &mut 0,
        &split,
    );
    assert!(expected.fallback.is_none());

    // Even a checksummed old recipe filed under the NEW exact key cannot lend
    // its old indices, slots, owner assignments or completed marker. The first
    // actual request is already edited and has NO in-process materialization.
    current_cache()
        .pending(&old_layout)
        .unwrap()
        .pin()
        .unwrap()
        .commit();
    let graph = CurrentGraph::read(&edited, None).unwrap();
    assert!(
        current_cache()
            .load_classes_observed(
                &edited,
                &source.as_view(),
                EntryPoint::None,
                16,
                &split,
                &graph,
            )
            .is_none()
    );
    let current = super::super::partition_classes_reusing(
        edited.clone(),
        &source.as_view(),
        EntryPoint::None,
        16,
        &split,
        Some(&directory.0),
        None,
    )
    .unwrap();
    assert!(
        !current.reused_layout,
        "fresh current planning is not a disk hit"
    );
    assert_eq!(current.plan.units, expected.units);
    assert_eq!(current.materialization.as_ref().unwrap().reused_items, 0);
    assert!(
        !current
            .materialization
            .as_ref()
            .unwrap()
            .has_reuse_inventory()
    );
    assert!(
        current
            .materialization
            .as_ref()
            .unwrap()
            .has_completed_roles()
    );
    let pending = current.pending.unwrap();
    assert_eq!(pending.key(), current_cache().key());
    assert_eq!(
        current.layout_key.as_deref(),
        Some(pending.key().to_hex().as_str())
    );
    assert_eq!(
        current_cache().read().unwrap(),
        old_layout,
        "no early publication"
    );
    pending.pin().unwrap().commit();
    assert_eq!(current_cache().read(), recorded);
    let restarted = super::super::partition_classes_reusing(
        edited.clone(),
        &source.as_view(),
        EntryPoint::None,
        16,
        &split,
        Some(&directory.0),
        None,
    )
    .unwrap();
    assert!(restarted.reused_layout);
    assert_eq!(restarted.plan.units, expected.units);
    assert!(
        !restarted
            .materialization
            .as_ref()
            .unwrap()
            .has_reuse_inventory()
    );
    for unit in &restarted.plan.units {
        zil_llvm_api::validate_module(unit).unwrap();
    }
}

#[test]
fn current_replay_keeps_unrewritten_body_owners_and_refuses_stale_reads() {
    let (module, source) = fixture();
    let (original, layout) = planned(&module, &source);
    let graph = CurrentGraph::read(&module, None).unwrap();
    let (current, reused) = layout.replay_observed(module.clone(), None, Some(&graph));
    assert_eq!(reused, 0);
    assert_eq!(current, original.units);
    let blocks = |module: &Module| {
        module
            .items
            .iter()
            .find_map(|item| match item {
                Item::Function(function) if function.name == "zil.fn.2" => function.blocks.clone(),
                _ => None,
            })
            .unwrap()
    };
    // This definition shares a unit with imported bodies, but has no direct
    // calls to rewrite. Avoiding a mutable traversal preserves its raw owner.
    assert!(blocks(&module).ptr_eq(&blocks(&current[0])));
    assert!(!blocks(&module).ptr_eq(&blocks(&original.units[0])));

    let mut changed = module.clone();
    changed.items[5] = Item::Function(function("zil.fn.2", Some("zil.fn.1")));
    assert!(!graph.matches(&changed));
    let expected = layout.replay(changed.clone());
    let (actual, _) = layout.replay_observed(changed, None, Some(&graph));
    assert_eq!(
        actual, expected,
        "old no-call rows cannot suppress a new call rewrite"
    );
    for unit in &actual {
        zil_llvm_api::validate_module(unit).unwrap();
    }
}

#[test]
fn direct_first_edit_checks_current_callee_header_and_local_state() {
    let (module, source) = fixture();
    let classes = BTreeSet::new();
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
    let old_key = first.pending.as_ref().unwrap().key();
    first.pending.unwrap().pin().unwrap().commit();
    drop(first.materialization);
    drop(first.plan);

    let mut callee = module.clone();
    callee.items[3] = Item::Function(function("zil.fn.0", Some("foreign")));
    let mut header = module.clone();
    let Item::Function(function) = &mut header.items[4] else {
        unreachable!()
    };
    function.attrs.push(Attr::NoInline);
    let mut local_state = module;
    let Item::Function(function) = &mut local_state.items[4] else {
        unreachable!()
    };
    function.blocks.as_mut().unwrap()[0].instructions.insert(
        0,
        Inst::new(
            Some("state.read".into()),
            Op::Load {
                ty: Ty::Int(32),
                ptr: Val::Global("state".into()),
                align: 4,
            },
        ),
    );
    for (label, edited, rejected) in [
        ("callee", callee, false),
        ("header", header, false),
        ("local-state", local_state, true),
    ] {
        zil_llvm_api::validate_module(&edited).unwrap();
        let cache = class_cache(&directory, &edited, &source, &classes);
        assert_ne!(cache.key(), old_key, "{label}");
        assert!(
            cache.read().is_none(),
            "first request already contains the edit"
        );
        let mut recorded = None;
        let original = super::super::partition_view_once(
            edited.clone(),
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
        assert_eq!(original.fallback.is_some(), rejected, "{label}");
        let current = super::super::partition_classes_reusing(
            edited,
            &source.as_view(),
            EntryPoint::None,
            2,
            &classes,
            Some(&directory.0),
            None,
        );
        assert_eq!(current.is_none(), rejected, "{label}");
        if let Some(current) = current {
            assert!(!current.reused_layout);
            assert_eq!(current.plan.units, original.units, "{label}");
            assert!(
                !current
                    .materialization
                    .as_ref()
                    .unwrap()
                    .has_reuse_inventory()
            );
            for unit in &current.plan.units {
                zil_llvm_api::validate_module(unit).unwrap();
            }
        }
    }
}

#[test]
fn current_shared_body_observations_keep_original_work_and_current_symbol_binding() {
    let mut caller = function("caller", Some("callee"));
    let mut other = caller.clone();
    other.name = "other".into();
    let mut module = Module {
        triple: TargetProfile::host_llvm().triple,
        fingerprint: "current".into(),
        items: vec![
            Item::Function(caller.clone()),
            Item::Function(function("callee", None)),
            Item::Function(other),
        ]
        .into(),
    };
    GRAPH_WORK.with(|work| work.set((0, 0)));
    let graph = CurrentGraph::read(&module, None).unwrap();
    GRAPH_WORK.with(|work| assert_eq!(work.get(), (2, 1)));
    assert!(std::sync::Arc::ptr_eq(&graph.rows[0], &graph.rows[2]));
    let required = module.items.len() + graph.rows.iter().map(|row| row.work).sum::<usize>();
    assert!(CurrentGraph::read_bounded(&module, None, required).is_some());
    assert!(CurrentGraph::read_bounded(&module, None, required - 1).is_none());
    let Op::Call { callee, .. } = &mut caller.blocks.as_mut().unwrap()[0].instructions[0].op else {
        unreachable!()
    };
    *callee = Val::Global("other".into());
    module.items[0] = Item::Function(caller);
    let changed = CurrentGraph::read(&module, None).unwrap();
    assert!(!std::sync::Arc::ptr_eq(&changed.rows[0], &changed.rows[2]));
    let Item::Function(callee) = &mut module.items[1] else {
        unreachable!()
    };
    callee.name = "renamed".into();
    assert!(CurrentGraph::read(&module, None).is_none());
}

#[test]
fn class_adaptation_preserves_source_grouping_and_original_copy_budget() {
    let (original, source, classes) = split_class_fixture(7);
    let directory = Directory::new();
    let first = super::super::partition_classes_reusing(
        original,
        &source.as_view(),
        EntryPoint::None,
        16,
        &classes,
        Some(&directory.0),
        None,
    )
    .unwrap();
    first.pending.unwrap().pin().unwrap().commit();
    let (changed, _, split) = split_class_fixture(19);
    let mut moved = source.clone();
    moved.functions[4].span.file = FileId(6);
    let actual = super::super::partition_classes_reusing(
        changed.clone(),
        &moved.as_view(),
        EntryPoint::None,
        16,
        &split,
        None,
        first.materialization.as_ref(),
    )
    .unwrap();
    let fresh = super::super::partition_classes_reusing(
        changed,
        &moved.as_view(),
        EntryPoint::None,
        16,
        &split,
        None,
        None,
    )
    .unwrap();
    assert!(!actual.reused_layout);
    assert_eq!(actual.plan.units, fresh.plan.units);

    let (mut oversized, _, split) = split_class_fixture(7);
    let target = if let Item::Function(caller) = &oversized.items[0] {
        super::super::direct_calls(caller).first().unwrap().clone()
    } else {
        unreachable!()
    };
    let body = oversized
        .items
        .iter_mut()
        .find_map(|item| match item {
            Item::Function(function) if function.name == target => Some(function),
            _ => None,
        })
        .unwrap();
    let instructions = &mut body.blocks.as_mut().unwrap()[0].instructions;
    let last = instructions.pop().unwrap();
    for index in 0..128 {
        instructions.push(Inst::new(
            Some(format!("extra{index}")),
            Op::Binary {
                opcode: "add".into(),
                ty: Ty::Int(32),
                left: Val::Int(index),
                right: Val::Int(1),
            },
        ));
    }
    instructions.push(last);
    zil_llvm_api::validate_module(&oversized).unwrap();
    let mut recorded = None;
    let original = super::super::partition_view_once(
        oversized.clone(),
        &source.as_view(),
        EntryPoint::None,
        16,
        true,
        true,
        &mut recorded,
        None,
        &mut 0,
        &split,
    );
    assert!(original.fallback.is_some());
    assert!(recorded.is_none());
    let current_cache = Cache::new(
        &directory.0,
        &oversized,
        &source.as_view(),
        EntryPoint::None,
        16,
        true,
        CallImportPolicy::Complete,
    )
    .unwrap()
    .with_classes(&split);
    assert!(current_cache.read().is_none());
    for previous in [first.materialization.as_ref(), None] {
        assert!(
            super::super::partition_classes_reusing(
                oversized.clone(),
                &source.as_view(),
                EntryPoint::None,
                16,
                &split,
                Some(&directory.0),
                previous,
            )
            .is_none()
        );
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

fn pad_cost(function: &mut Function, cost: usize) {
    let mut next = function.cost() as i128;
    while function.cost() < cost {
        function.blocks.as_mut().unwrap()[0].instructions.insert(
            0,
            Inst::new(
                Some(format!("padding{next}")),
                Op::Binary {
                    opcode: "add".into(),
                    ty: Ty::Int(32),
                    left: Val::Int(next),
                    right: Val::Int(1),
                },
            ),
        );
        next += 1;
    }
}

fn bounded_program(value: i128) -> (Module, LirModule, BTreeSet<String>) {
    let (_, mut source) = fixture();
    let seed = source.functions[0].clone();
    source.functions = (0..16)
        .map(|id| {
            let mut function = seed.clone();
            function.id = FunctionId(id);
            function.name = format!("f{id}");
            function.span.file = FileId(id);
            function
        })
        .collect();
    let helper = "zil.class.small";
    let mut root = function("zil.fn.0", None);
    let mut instructions = Vec::new();
    let mut sum = Val::Int(0);
    for id in 1..16 {
        instructions.push(Inst::new(
            Some(format!("call{id}")),
            Op::Call {
                result: Ty::Int(32),
                result_attrs: vec![],
                callee: Val::Global(format!("zil.fn.{id}")),
                args: vec![],
                vararg: None,
            },
        ));
        instructions.push(Inst::new(
            Some(format!("sum{id}")),
            Op::Binary {
                opcode: "add".into(),
                ty: Ty::Int(32),
                left: sum,
                right: Val::Local(format!("call{id}")),
            },
        ));
        sum = Val::Local(format!("sum{id}"));
    }
    instructions.push(Inst::new(
        None,
        Op::Return(Some(Arg::new(Ty::Int(32), sum))),
    ));
    root.blocks.as_mut().unwrap()[0].instructions = instructions;
    let mut items = vec![Item::Function(root)];
    for id in 1..16 {
        let mut body = function(&format!("zil.fn.{id}"), Some(helper));
        pad_cost(&mut body, super::super::MAX_CLASS_IMPORT_COST + 1);
        if id == 7 {
            body.blocks.as_mut().unwrap()[0]
                .instructions
                .last_mut()
                .unwrap()
                .op = Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(value))));
        }
        items.push(Item::Function(body));
    }
    items.push(Item::Function(function(helper, None)));
    items.push(Item::Function(function("main", Some("zil.fn.0"))));
    (
        Module {
            triple: source.target.triple.clone(),
            fingerprint: "bounded-program".into(),
            items: items.into(),
        },
        source,
        BTreeSet::from([helper.into()]),
    )
}

fn prune_bounded_program(mut units: Vec<Module>, classes: &BTreeSet<String>) -> Vec<Module> {
    let mut candidates = classes.clone();
    candidates.extend((0..16).map(|id| format!("zil.fn.{id}")));
    super::super::internalize_closed_executable_names(&mut units, &candidates);
    for unit in &mut units {
        crate::machine_closed_prune::prune(unit, &|| false)
            .unwrap()
            .unwrap();
        crate::machine_closed_prune::prune_class_declarations(unit, classes, &|| false)
            .unwrap()
            .unwrap();
        zil_llvm_api::validate_module(unit).unwrap();
    }
    units
}

#[test]
fn bounded_first_edit_keeps_sixteen_live_units_and_entry_work_unchanged() {
    use super::super::partition_classes_reusing_with_policy;
    let directory = Directory::new();
    let (module, source, classes) = bounded_program(7);
    zil_llvm_api::validate_module(&module).unwrap();
    let build = |module, previous| {
        partition_classes_reusing_with_policy(
            module,
            &source.as_view(),
            EntryPoint::ReturnValue(FunctionId(0)),
            16,
            &classes,
            Some(&directory.0),
            previous,
            CallImportPolicy::BoundedClasses,
        )
        .unwrap()
    };
    let first = build(module.clone(), None);
    let mut recorded = None;
    let original = super::super::partition_view_once_observed(
        module.clone(),
        &source.as_view(),
        EntryPoint::ReturnValue(FunctionId(0)),
        16,
        true,
        CallImportPolicy::BoundedClasses,
        &mut recorded,
        None,
        &mut 0,
        &classes,
        None,
    );
    assert!(original.fallback.is_none());
    assert_eq!(first.plan.units, original.units);
    let first_key = first.pending.as_ref().unwrap().key();
    first.pending.unwrap().pin().unwrap().commit();
    let first_units = prune_bounded_program(first.plan.units, &classes);
    assert_eq!(first_units.len(), 16);
    assert!(first_units.iter().all(|unit| {
        unit.items
            .iter()
            .any(|item| matches!(item, Item::Function(f) if f.blocks.is_some()))
    }));
    let complete = super::super::partition_classes_reusing(
        module,
        &source.as_view(),
        EntryPoint::ReturnValue(FunctionId(0)),
        16,
        &classes,
        None,
        None,
    )
    .unwrap();
    let complete_units = prune_bounded_program(complete.plan.units, &classes);
    assert_eq!(
        complete_units
            .iter()
            .filter(|unit| unit
                .items
                .iter()
                .any(|item| matches!(item, Item::Function(f) if f.blocks.is_some())))
            .count(),
        1
    );

    // Discard the process-local product. The next request already has the edit.
    drop(first.materialization);
    let (edited, _, _) = bounded_program(19);
    let current = build(edited, None);
    assert!(!current.reused_layout);
    assert_ne!(current.pending.as_ref().unwrap().key(), first_key);
    assert!(
        !current
            .materialization
            .as_ref()
            .unwrap()
            .has_reuse_inventory()
    );
    let current_units = prune_bounded_program(current.plan.units, &classes);
    let dirty = first_units
        .iter()
        .zip(&current_units)
        .enumerate()
        .filter_map(|(index, (a, b))| (a.items != b.items).then_some(index))
        .collect::<Vec<_>>();
    assert_eq!(dirty, [7]);
    let entry = first_units
        .iter()
        .position(|unit| {
            unit.items.iter().any(
                |item| matches!(item, Item::Function(f) if f.name == "main" && f.blocks.is_some()),
            )
        })
        .unwrap();
    assert_eq!(first_units[entry].items, current_units[entry].items);
}

#[test]
fn bounded_policy_cannot_reuse_complete_disk_or_materialization_roles() {
    use super::super::partition_classes_reusing_with_policy;
    let (module, source) = fixture();
    let classes = BTreeSet::new();
    let directory = Directory::new();
    let complete = super::super::partition_classes_reusing(
        module.clone(),
        &source.as_view(),
        EntryPoint::None,
        2,
        &classes,
        Some(&directory.0),
        None,
    )
    .unwrap();
    let old_key = complete.pending.as_ref().unwrap().key();
    let old_layout = Layout::decode(&complete.pending.as_ref().unwrap().bytes).unwrap();
    complete.pending.unwrap().pin().unwrap().commit();
    let cache = Cache::new(
        &directory.0,
        &module,
        &source.as_view(),
        EntryPoint::None,
        2,
        true,
        CallImportPolicy::BoundedClasses,
    )
    .unwrap()
    .with_classes(&classes);
    assert_ne!(cache.key(), old_key);
    assert!(cache.read().is_none());
    // Even a correctly framed Complete recipe under the new key is rejected.
    Cache::new(
        &directory.0,
        &module,
        &source.as_view(),
        EntryPoint::None,
        2,
        true,
        CallImportPolicy::BoundedClasses,
    )
    .unwrap()
    .with_classes(&classes)
    .pending(&old_layout)
    .unwrap()
    .pin()
    .unwrap()
    .commit();
    assert!(
        cache
            .load_classes(&module, &source.as_view(), EntryPoint::None, 2, &classes)
            .is_none()
    );
    let build = |previous| {
        partition_classes_reusing_with_policy(
            module.clone(),
            &source.as_view(),
            EntryPoint::None,
            2,
            &classes,
            Some(&directory.0),
            previous,
            CallImportPolicy::BoundedClasses,
        )
        .unwrap()
    };
    let current = build(complete.materialization.as_ref());
    assert!(!current.reused_layout);
    assert_eq!(current.materialization.as_ref().unwrap().reused_items, 0);
    assert_ne!(current.plan.units, complete.plan.units);
    let graph = CurrentGraph::read(&module, None).unwrap();
    let grouping = Grouping::read_with_policy(
        &module,
        &source.as_view(),
        EntryPoint::None,
        2,
        &classes,
        &graph,
        CallImportPolicy::BoundedClasses,
    )
    .unwrap();
    assert!(
        complete
            .materialization
            .as_ref()
            .unwrap()
            .layout_for(&grouping)
            .is_none()
    );
    current.pending.unwrap().pin().unwrap().commit();
    let restarted = build(None);
    assert!(restarted.reused_layout);
    assert_eq!(restarted.plan.units, current.plan.units);
    let reused = build(current.materialization.as_ref());
    assert!(reused.reused_layout);
    assert!(reused.materialization.as_ref().unwrap().reused_items > 0);
    let complete_again = super::super::partition_classes_reusing(
        module,
        &source.as_view(),
        EntryPoint::None,
        2,
        &classes,
        Some(&directory.0),
        current.materialization.as_ref(),
    )
    .unwrap();
    assert_eq!(
        complete_again
            .materialization
            .as_ref()
            .unwrap()
            .reused_items,
        0
    );
    assert_eq!(complete_again.plan.units, complete.plan.units);
}

#[test]
fn bounded_helpers_check_cost_headers_local_state_and_canonical_addresses() {
    use super::super::{MAX_CLASS_IMPORT_COST, partition_classes_reusing_with_policy};
    let (mut module, source) = fixture();
    // Helpers and both excluded boundaries occupy bucket 1. The only original
    // direct user is in bucket 0, so membership here proves the traversal.
    let helper_name = |prefix: &str| {
        (0..1000)
            .map(|id| format!("zil.class.{prefix}.{id}"))
            .find(|name| {
                let bytes = Fingerprint::of(name.as_bytes()).0;
                u64::from_le_bytes(bytes[..8].try_into().unwrap()) % 2 == 1
            })
            .unwrap()
    };
    let helper = helper_name("helper");
    let tail = helper_name("tail");
    let large = helper_name("large");
    let beyond_large = helper_name("beyond-large");
    let beyond_wrapper = helper_name("beyond-wrapper");
    let mut body = function(&helper, Some(&tail));
    for (name, callee) in [
        ("wrapper.result", "zil.fn.1"),
        ("large.result", large.as_str()),
    ] {
        body.blocks.as_mut().unwrap()[0].instructions.insert(
            0,
            Inst::new(
                Some(name.into()),
                Op::Call {
                    result: Ty::Int(32),
                    result_attrs: vec![],
                    callee: Val::Global(callee.into()),
                    args: vec![],
                    vararg: None,
                },
            ),
        );
    }
    pad_cost(&mut body, MAX_CLASS_IMPORT_COST);
    module.items[4] = Item::Function(function("zil.fn.1", Some(&beyond_wrapper)));
    let Item::Function(source_body) = &mut module.items[4] else {
        unreachable!()
    };
    pad_cost(source_body, 2 * MAX_CLASS_IMPORT_COST);
    let mut caller = function("zil.fn.0", Some(&helper));
    caller.blocks.as_mut().unwrap()[0].instructions.insert(
        0,
        Inst::new(
            Some("canonical.address".into()),
            Op::Cast {
                opcode: "ptrtoint".into(),
                arg: Arg::new(Ty::Ptr, Val::Global(helper.clone())),
                to: Ty::Int(64),
            },
        ),
    );
    module.items[3] = Item::Function(caller);
    let helper_index = module.items.len();
    module.items.push(Item::Function(body));
    let tail_index = module.items.len();
    let mut leaf = function(&tail, None);
    pad_cost(&mut leaf, 31);
    module.items.push(Item::Function(leaf));
    let large_index = module.items.len();
    let mut large_body = function(&large, Some(&beyond_large));
    pad_cost(&mut large_body, MAX_CLASS_IMPORT_COST + 1);
    module.items.push(Item::Function(large_body));
    let beyond_large_index = module.items.len();
    module
        .items
        .push(Item::Function(function(&beyond_large, None)));
    let beyond_wrapper_index = module.items.len();
    module
        .items
        .push(Item::Function(function(&beyond_wrapper, None)));
    let classes = BTreeSet::from([
        helper.clone(),
        tail.clone(),
        large.clone(),
        beyond_large,
        beyond_wrapper,
    ]);
    let build = |module| {
        partition_classes_reusing_with_policy(
            module,
            &source.as_view(),
            EntryPoint::None,
            2,
            &classes,
            None,
            None,
            CallImportPolicy::BoundedClasses,
        )
    };
    let graph = CurrentGraph::read(&module, None).unwrap();
    let layout = Layout::plan_classes_current(
        &module,
        &source.as_view(),
        EntryPoint::None,
        2,
        &classes,
        &graph,
        CallImportPolicy::BoundedClasses,
    )
    .unwrap();
    assert!(layout.units[0].imports.contains(&helper_index));
    assert!(
        layout.units[0].imports.contains(&tail_index),
        "admitted class dependencies are imported transitively"
    );
    assert!(!layout.units[0].declarations.contains(&tail_index));
    for index in [4, large_index, beyond_large_index, beyond_wrapper_index] {
        assert!(
            !layout.units[0].imports.contains(&index),
            "excluded boundary or its child"
        );
    }
    assert!(layout.units[0].declarations.contains(&4));
    assert!(layout.units[0].declarations.contains(&large_index));
    let selected = build(module.clone()).unwrap();
    for unit in &selected.plan.units {
        zil_llvm_api::validate_module(unit).unwrap();
    }
    let caller = selected.plan.units[0]
        .items
        .iter()
        .find_map(|item| match item {
            Item::Function(f) if f.name == "zil.fn.0" => Some(f),
            _ => None,
        })
        .unwrap();
    assert!(super::super::direct_calls(caller).contains(&format!("zil.import.0.{helper}")));
    assert!(caller.blocks.as_ref().unwrap()[0].instructions.iter().any(
        |inst| matches!(&inst.op, Op::Cast { arg, .. } if arg.value == Val::Global(helper.clone()))
    ));
    let imported_helper = selected.plan.units[0]
        .items
        .iter()
        .find_map(|item| match item {
            Item::Function(f) if f.name == format!("zil.import.0.{helper}") => Some(f),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        super::super::direct_calls(imported_helper),
        BTreeSet::from([
            format!("zil.import.0.{tail}"),
            "zil.fn.1".into(),
            large.clone(),
        ])
    );
    assert!(
        layout
            .valid_classes_observed_with_policy(
                &module,
                &source.as_view(),
                EntryPoint::None,
                2,
                &classes,
                &graph,
                CallImportPolicy::BoundedClasses,
            )
            .is_some()
    );
    // A checksummed one-layer recipe is no longer a valid current graph plan.
    let mut partial = layout.clone();
    partial.units[0]
        .imports
        .retain(|&index| index != tail_index);
    partial.units[0].declarations.push(tail_index);
    partial.units[0].declarations.sort_unstable();
    assert!(
        partial
            .valid_classes_observed_with_policy(
                &module,
                &source.as_view(),
                EntryPoint::None,
                2,
                &classes,
                &graph,
                CallImportPolicy::BoundedClasses,
            )
            .is_none()
    );
    let mut recorded = None;
    let original = super::super::partition_view_once_observed(
        module.clone(),
        &source.as_view(),
        EntryPoint::None,
        2,
        true,
        CallImportPolicy::BoundedClasses,
        &mut recorded,
        None,
        &mut 0,
        &classes,
        None,
    );
    assert!(original.fallback.is_none());
    assert_eq!(selected.plan.units, original.units);

    let mut oversized = module.clone();
    let Item::Function(f) = &mut oversized.items[helper_index] else {
        unreachable!()
    };
    pad_cost(f, MAX_CLASS_IMPORT_COST + 1);
    let mut noinline = module.clone();
    let Item::Function(f) = &mut noinline.items[helper_index] else {
        unreachable!()
    };
    f.attrs.push(Attr::NoInline);
    let mut stateful = module.clone();
    stateful.items.push(Item::Global(Global {
        name: "helper.state".into(),
        linkage: Linkage::Private,
        ty: Ty::Int(32),
        initializer: Some(Val::Int(11)),
        constant: false,
        unnamed_addr: false,
        alignment: 4,
    }));
    let Item::Function(f) = &mut stateful.items[helper_index] else {
        unreachable!()
    };
    f.blocks.as_mut().unwrap()[0].instructions.insert(
        0,
        Inst::new(
            Some("local.state".into()),
            Op::Load {
                ty: Ty::Int(32),
                ptr: Val::Global("helper.state".into()),
                align: 4,
            },
        ),
    );
    // Keep cost at the admitted boundary; it is the local edge that forbids copying.
    f.blocks.as_mut().unwrap()[0].instructions.remove(1);
    for current in [oversized, noinline, stateful] {
        zil_llvm_api::validate_module(&current).unwrap();
        let graph = CurrentGraph::read(&current, None).unwrap();
        assert!(
            layout
                .valid_classes_observed_with_policy(
                    &current,
                    &source.as_view(),
                    EntryPoint::None,
                    2,
                    &classes,
                    &graph,
                    CallImportPolicy::BoundedClasses,
                )
                .is_none(),
            "old imported roles must be refused"
        );
        let fresh = build(current.clone()).unwrap();
        let mut recorded = None;
        let original = super::super::partition_view_once_observed(
            current,
            &source.as_view(),
            EntryPoint::None,
            2,
            true,
            CallImportPolicy::BoundedClasses,
            &mut recorded,
            None,
            &mut 0,
            &classes,
            None,
        );
        assert!(original.fallback.is_none());
        assert_eq!(fresh.plan.units, original.units);
        assert!(!recorded.unwrap().units[0].imports.contains(&helper_index));
        for unit in &fresh.plan.units {
            zil_llvm_api::validate_module(unit).unwrap();
        }
    }
    let mut budget = module.clone();
    budget.items[4] = Item::Function(function("zil.fn.1", Some("foreign")));
    assert!(
        build(budget).is_none(),
        "per-helper cap cannot enlarge the original source copy cap"
    );
    let mut bad_header = module;
    let Item::Function(f) = &mut bad_header.items[2] else {
        unreachable!()
    };
    f.linkage = Linkage::Internal;
    assert!(
        build(bad_header).is_none(),
        "unused original headers still get preflight"
    );
}

#[test]
fn current_partition_rewrites_shared_bodies_once_per_unit_mapping() {
    let leaf = function("leaf", None);
    let mut caller = function("caller", Some("leaf"));
    caller.blocks.as_mut().unwrap()[0].instructions.insert(
        0,
        Inst::new(
            Some("address".into()),
            Op::Freeze(Arg::new(Ty::Ptr, Val::Global("leaf".into()))),
        ),
    );
    let mut items = vec![Item::Function(leaf)];
    for id in 0..64 {
        let mut f = caller.clone();
        f.name = format!("caller.{id}");
        items.push(Item::Function(f));
    }
    let raw = Module {
        triple: "x86_64-unknown-linux-gnu".into(),
        fingerprint: "current".into(),
        items: items.into(),
    };
    zil_llvm_api::validate_module(&raw).unwrap();
    let mut layout = Layout {
        units: vec![
            Unit {
                owner: 0,
                prelude: vec![],
                declarations: vec![],
                imports: vec![0],
                definitions: (1..65).collect(),
            },
            Unit {
                owner: 1,
                prelude: vec![],
                declarations: vec![],
                imports: vec![],
                definitions: vec![0],
            },
        ],
    };
    for owner in [0, 7] {
        layout.units[0].owner = owner;
        // No previous Materialization, exactly the restart-first-request path.
        let actual = layout.replay(raw.clone());
        let mut expected = actual.clone();
        let names = BTreeMap::from([("leaf".into(), format!("zil.import.{owner}.leaf"))]);
        let mut first = None;
        for (slot, original) in (2..66).zip(1..65) {
            let Item::Function(mut ordinary) = raw.items[original].clone() else {
                unreachable!()
            };
            crate::machine_cgu::rewrite_direct_calls(&mut ordinary, &names);
            expected[0].items[slot] = Item::Function(ordinary);
            let Item::Function(f) = &actual[0].items[slot] else {
                unreachable!()
            };
            let blocks = f.blocks.as_ref().unwrap();
            if let Some(first) = first {
                assert!(blocks.ptr_eq(first));
            } else {
                first = Some(blocks);
            }
            assert!(
                matches!(&blocks[0].instructions[0].op, Op::Freeze(Arg { value: Val::Global(n), .. }) if n == "leaf")
            );
        }
        assert_eq!(actual, expected);
        for unit in &actual {
            zil_llvm_api::validate_module(unit).unwrap();
        }
    }
    // Exhaust optional retention: the ordinary rewrite still runs in full.
    let names = BTreeMap::from([("leaf".into(), "other".into())]);
    let mut rewrites = CallRewrites::new(&names);
    rewrites.remaining = 0;
    let mut actual = caller.clone();
    let mut expected = caller;
    rewrites.rewrite(&mut actual);
    crate::machine_cgu::rewrite_direct_calls(&mut expected, &names);
    assert_eq!(actual, expected);
    assert!(rewrites.bodies.is_empty());
}

#[test]
fn shared_layout_digest_binds_complete_values_independent_of_allocation() {
    let digest = |items: &[Item]| {
        let mut state = BufferedHasher::new();
        hash_shared_items(items, &mut state);
        state.digest()
    };
    let first = function("first", Some("target"));
    let mut second = first.clone();
    second.name = "second".into();
    let original = vec![Item::Function(first), Item::Function(second)];
    let key = digest(&original);
    let mut detached = original.clone();
    if let Item::Function(f) = &mut detached[1] {
        f.blocks = Some(f.blocks.as_ref().unwrap().to_vec().into());
    }
    assert_eq!(digest(&detached), key);
    for variant in 0..7 {
        let mut current = original.clone();
        let Item::Function(f) = &mut current[1] else {
            unreachable!()
        };
        match variant {
            0 => f.name.push_str(".changed"),
            1 => f.linkage = Linkage::Internal,
            2 => f.signature.result = Ty::Int(64),
            3 => f.attrs.push(Attr::NoInline),
            4 => f.blocks = None,
            5 => f.blocks = Some(vec![].into()),
            6 => {
                f.blocks.as_mut().unwrap()[0].instructions[0].op =
                    Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(9))))
            }
            _ => unreachable!(),
        }
        assert_ne!(digest(&current), key, "current function field {variant}");
    }
    let mut types = original.clone();
    types.push(Item::NamedType("record".into(), vec![Ty::Int(32)]));
    assert_ne!(digest(&types), key);
    let before = digest(&types);
    *types.last_mut().unwrap() = Item::NamedType("record".into(), vec![Ty::Int(64)]);
    assert_ne!(digest(&types), before);
}
