//! Root runs these with the aggregate backend window. No native SDK needed.
use super::*;
use zil_base::{ModuleId, SCHEMA_VERSION, Span};
use zil_ir::*;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "zil-raw-emission-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        assert!(private_directory(&root, true));
        Self(root)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn request(root: &Path) -> Request {
    Request {
        inputs: None,
        validation: None,
        directory: root.join("test-installed-identity"),
        manifest: String::new(),
        context: vec![],
        limits: Limits::default(),
        old: vec![],
        aliases: BTreeMap::new(),
        decoded: BTreeMap::new(),
        accepted: BTreeMap::new(),
        decode_nodes: 0,
        products: vec![],
        current: vec![],
        current_aliases: BTreeMap::new(),
        owners: Default::default(),
        bytes: 0,
        enabled: false,
        maximum: MAX_BYTES,
        hits: 0,
        misses: 0,
    }
}
fn function(id: u32, value: i32) -> TypedFunction {
    TypedFunction {
        storage_life: None,
        call_kind: CallKind::Direct,
        reference_return_bound: None,
        callable_return_contract: None,
        callable_contract: None,
        protocol_contract: None,
        id: FunctionId(id),
        name: format!("raw_{id}"),
        foreign: None,
        environment: None,
        parameters: vec![LocalId(0)],
        return_type: Type::I32,
        result_mode: PassingMode::Infer,
        locals: vec![LocalDecl {
            id: LocalId(0),
            name: "input".into(),
            ty: Type::I32,
            mutable: false,
            span: Span::default(),
        }]
        .into(),
        values: vec![Type::I32; 3].into(),
        entry: BlockId(0),
        span: Span::default(),
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction {
                    result: Some(ValueId(0)),
                    kind: InstructionKind::Load(LocalId(0)),
                    span: Span::default(),
                },
                Instruction {
                    result: Some(ValueId(1)),
                    kind: InstructionKind::Constant(Constant::I32(value)),
                    span: Span::default(),
                },
                Instruction {
                    result: Some(ValueId(2)),
                    kind: InstructionKind::Binary {
                        op: BinaryOp::Multiply,
                        left: ValueId(0),
                        right: ValueId(1),
                    },
                    span: Span::default(),
                },
            ],
            terminator: Terminator::Return(Some(ValueId(2))),
        }]
        .into(),
    }
}
fn fixture(edit: bool) -> VerifiedModule {
    let module = TypedModule {
        dyn_declarations: vec![],
        dyn_interfaces: vec![],
        dyn_vtables: vec![],
        life_declarations: vec![],
        life_instances: vec![],
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(if edit { b"edited" } else { b"parent" }),
        target: TargetProfile::host_llvm(),
        functions: (0..12)
            .map(|id| {
                function(
                    id,
                    if edit && id == 0 {
                        99
                    } else {
                        3 + (id % 3) as i32
                    },
                )
            })
            .collect(),
        resources: vec![],
        closures: vec![],
        unsupported: vec![],
    };
    checked(module)
}
fn checked(module: TypedModule) -> VerifiedModule {
    let mut plan = CandidatePlan::empty_for(&module);
    plan.functions = module
        .functions
        .iter()
        .map(|function| FunctionPlan {
            function: function.id,
            holds: (0..function.values.len())
                .map(|id| ValueHold {
                    value: ValueId(id as u32),
                    roots: vec![],
                })
                .collect(),
            blocks: vec![BlockPlan {
                block: BlockId(0),
                initialized_in: vec![LocalId(0)],
                initialized_out: vec![LocalId(0)],
            }]
            .into(),
        })
        .collect();
    let target = module.target.clone();
    zil_verify::verify_module(module, plan, &target, &Limits::default()).unwrap()
}
fn lir(verified: &VerifiedModule) -> LirModule {
    let source = verified.module();
    LirModule {
        schema_version: source.schema_version,
        module_id: source.module_id,
        verified_fingerprint: verified.fingerprint(),
        target: source.target.clone(),
        functions: source.functions.clone(),
        resources: vec![],
        closures: vec![],
        layouts: [
            (Type::Unit, 0, 1),
            (Type::Bool, 1, 1),
            (Type::I32, 4, 4),
            (Type::I64, 8, 8),
            (Type::F32, 4, 4),
            (Type::F64, 8, 8),
            (Type::U8, 1, 1),
            (Type::U16, 2, 2),
            (Type::U32, 4, 4),
            (Type::I8, 1, 1),
            (Type::I16, 2, 2),
            (Type::Isize, 8, 8),
            (Type::ForeignHandle, 8, 8),
            (Type::CWideString, 8, 8),
        ]
        .into_iter()
        .map(|(ty, size, alignment)| ScalarLayout {
            ty,
            size,
            alignment,
        })
        .collect(),
    }
}
type Output = CompileResult<(m::Module, bool, usize)>;
fn lower(verified: &VerifiedModule, raw: &LirModule, cache: Option<&mut Request>) -> Output {
    let mut session = incremental::Session::default();
    super::super::lower_output_with_raw_products(
        verified,
        &raw.as_view(),
        &EmitOptions {
            entry: EntryPoint::None,
        },
        true,
        false,
        1,
        Some(&mut session),
        false,
        cache,
    )
    .map(|(module, collected, work)| (module.to_module(), collected, work))
}

#[test]
fn restarted_raw_products_match_original_emitter_and_edit_only_one_representative() {
    let directory = Directory::new();
    let verified = fixture(false);
    let source = lir(&verified);
    let expected = lower(&verified, &source, None).unwrap();
    let mut cold = request(&directory.0);
    assert_eq!(
        lower(&verified, &source, Some(&mut cold)).unwrap(),
        expected
    );
    assert_eq!(cold.current.len(), 3);
    assert_eq!(cold.current_aliases.len(), 12);
    assert_eq!(cold.misses, 3);
    cold.pending().unwrap().commit(&directory.0);
    // These are new owners and a fresh Session: no in-memory producer survives.
    let edited = fixture(true);
    let mut current = lir(&edited);
    // Real lowering may wrap the verified arrays in distinct function nodes.
    // Those wrappers must reach current semantic admissions and raw staging.
    current.functions = current.functions.iter().cloned().collect();
    assert!(!std::ptr::eq(
        &current.functions[0],
        &edited.module().functions[0]
    ));
    let mut restored = request(&directory.0);
    let mut checker = crate::machine_preflight::CurrentPreflight::default();
    restored.use_preflight(checker.body_session());
    let actual = lower(&edited, &current, Some(&mut restored)).unwrap();
    for (source, lowered) in edited.module().functions.iter().zip(&current.functions) {
        let inputs = restored.inputs.as_deref();
        let source_key = scalar_bodies::key(inputs, source);
        assert!(matches!(source_key, scalar_bodies::Key::Semantic(_)));
        assert_eq!(scalar_bodies::key(inputs, lowered), source_key);
    }
    let before = checker.body_session().borrow().body_counts();
    assert_eq!(before, (3, 0));
    checker.check(&actual.0).unwrap();
    let after = checker.body_session().borrow().body_counts();
    assert_eq!(
        after.0 - before.0,
        1,
        "only the freshly produced changed body is newly checked"
    );
    assert_eq!(
        after.1 - before.1,
        3,
        "all restored raw bodies already passed this current checker"
    );
    assert_eq!(actual, lower(&edited, &current, None).unwrap());
    assert_eq!((restored.hits, restored.misses), (3, 1));
    assert_eq!(restored.current.len(), 4);
    let bodies = actual
        .0
        .items
        .iter()
        .filter_map(|item| match item {
            m::Item::Function(function) => {
                Some((function.name.as_str(), function.blocks.as_ref()?))
            }
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    assert!(bodies["zil.fn.1"].ptr_eq(bodies["zil.fn.4"]));
    zil_llvm_api::validate_module(&actual.0).unwrap();
    // Accepted raw bytes survive current publication with immutable ownership;
    // the unchanged large body is never walked by either encoder again.
    assert!(
        restored
            .current
            .iter()
            .any(|current| restored
                .old
                .iter()
                .any(|old| Arc::ptr_eq(&current.input, &old.input)
                    && Arc::ptr_eq(&current.output, &old.output)
                    && Arc::ptr_eq(&current.facts, &old.facts)))
    );
}

#[test]
fn exact_context_malformed_machine_and_changed_summary_facts_refuse_raw_data() {
    let directory = Directory::new();
    let verified = fixture(false);
    let source = lir(&verified);
    let mut cold = request(&directory.0);
    lower(&verified, &source, Some(&mut cold)).unwrap();
    // Even a checksum-correct payload cannot substitute a different context.
    cold.context[0] ^= 1;
    cold.pending().unwrap().commit(&directory.0);
    let mut restored = request(&directory.0);
    assert_eq!(
        lower(&verified, &source, Some(&mut restored)),
        lower(&verified, &source, None)
    );
    assert_eq!(restored.hits, 0);
    let limits = Limits {
        max_source_bytes: 64,
        ..Limits::default()
    };
    let mut narrow = request(&directory.0);
    narrow.bind(
        Some(vec![0]),
        &limits,
        incremental::WholeOptions {
            emit: EmitOptions {
                entry: EntryPoint::None,
            },
            scalar_policy: true,
            closed_collection: false,
            requested_jobs: 1,
        },
        None,
    );
    assert_eq!(narrow.maximum, 64);
    assert!(narrow.old.is_empty());
    let mut cold = request(&directory.0);
    lower(&verified, &source, Some(&mut cold)).unwrap();
    // A decodable but malformed machine must run the original producer rather
    // than turn a valid current program into a machine-preflight failure.
    let mut batch =
        raw_codec::decode(&cold.current[0].output, verified.limits(), &mut 1_000_000).unwrap();
    let m::Item::Function(function) = &mut batch.body[0] else {
        unreachable!()
    };
    function.signature.result = m::Ty::Int(64);
    cold.current[0].output = raw_codec::encode(&batch).unwrap().into();
    cold.pending().unwrap().commit(&directory.0);
    let mut restored = request(&directory.0);
    assert_eq!(
        lower(&verified, &source, Some(&mut restored)),
        lower(&verified, &source, None)
    );
    assert_eq!((restored.hits, restored.misses), (2, 1));
    let mut caller = function_input_with_call();
    let first = BTreeMap::from([(FunctionId(9), Constant::I32(7))]);
    let second = BTreeMap::from([(FunctionId(9), Constant::I32(8))]);
    assert_ne!(facts(&caller, Some(&first)), facts(&caller, Some(&second)));
    caller.id = FunctionId(44);
    caller.name = "another_outer".into();
    assert_eq!(
        facts(&caller, Some(&first)),
        facts(&function_input_with_call(), Some(&first))
    );
}

fn function_input_with_call() -> TypedFunction {
    let mut caller = function(0, 3);
    caller.blocks[0].instructions[1].kind = InstructionKind::Call {
        function: FunctionId(9),
        arguments: vec![],
    };
    caller
}

#[test]
fn repeated_raw_calls_check_every_abi_occurrence_and_current_callee() {
    let mut source = function(0, 3);
    source.blocks[0].instructions.truncate(1);
    source.values = vec![Type::I32; 129].into();
    for id in 1..=128 {
        source.blocks[0].instructions.push(Instruction {
            result: Some(ValueId(id)),
            kind: InstructionKind::Call {
                function: FunctionId(9),
                arguments: vec![ValueId(0)],
            },
            span: Span::default(),
        });
    }
    source.blocks[0].terminator = Terminator::Return(Some(ValueId(128)));
    let callee = function(9, 2);
    let mut instructions = (0..128)
        .map(|id| {
            m::Inst::new(
                Some(format!("r{id}")),
                m::Op::Call {
                    result: m::Ty::Int(32),
                    result_attrs: vec![],
                    callee: m::Val::Global(symbol(callee.id)),
                    args: vec![m::Arg::new(m::Ty::Int(32), m::Val::Local("input".into()))],
                    vararg: None,
                },
            )
        })
        .collect::<Vec<_>>();
    instructions.push(m::Inst::new(
        None,
        m::Op::Return(Some(m::Arg::new(
            m::Ty::Int(32),
            m::Val::Local("r127".into()),
        ))),
    ));
    let machine = m::Function {
        name: symbol(source.id),
        linkage: m::Linkage::External,
        signature: m::Signature::new(
            m::Ty::Int(32),
            vec![m::Param {
                ty: m::Ty::Int(32),
                name: Some("input".into()),
                attrs: vec![],
            }],
        ),
        attrs: vec![],
        blocks: Some(
            vec![m::Block {
                name: "entry".into(),
                instructions,
            }]
            .into(),
        ),
    };
    let check = |machine: &m::Function, callee: &TypedFunction| {
        valid_machine(
            machine,
            &source,
            &BTreeMap::from([(callee.id, callee)]),
            &TargetProfile::host_llvm(),
            &BTreeSet::new(),
            None,
        )
    };
    assert!(check(&machine, &callee));
    for fault in 0..5 {
        let mut invalid = machine.clone();
        let m::Op::Call {
            result,
            result_attrs,
            args,
            vararg,
            ..
        } = &mut invalid.blocks.as_mut().unwrap()[0].instructions[127].op
        else {
            unreachable!()
        };
        match fault {
            0 => args[0].attrs.push(m::Attr::SignExt),
            1 => result_attrs.push(m::Attr::SignExt),
            2 => *result = m::Ty::Int(64),
            3 => *vararg = Some(vec![]),
            _ => args.clear(),
        }
        assert!(
            !check(&invalid, &callee),
            "late repeated call fault {fault}"
        );
    }
    let mut changed = callee.clone();
    changed.return_type = Type::I64;
    assert!(!check(&machine, &changed));
    changed = callee.clone();
    changed.id = FunctionId(10);
    assert!(!check(&machine, &changed));
}

#[test]
fn raw_hits_keep_original_budget_prefix_lir_binding_and_context_refusals() {
    let mut first = crate::BuildOptions::new("same-output");
    let mut second = first.clone();
    first.trusted_emission_cache = Some("/proc/self/fd/41/.".into());
    second.trusted_emission_cache = Some("/proc/self/fd/52/.".into());
    assert_eq!(first, second);
    second.trusted_emission_cache = None;
    assert_ne!(first, second);
    let directory = Directory::new();
    let verified = fixture(false);
    let source = lir(&verified);
    let mut cold = request(&directory.0);
    lower(&verified, &source, Some(&mut cold)).unwrap();
    cold.pending().unwrap().commit(&directory.0);
    for work in [0, 1, 8, 64, 512, 4096, 1_000_000] {
        let expected = incremental::with_initial_work(work, || lower(&verified, &source, None));
        let mut restored = request(&directory.0);
        let actual =
            incremental::with_initial_work(work, || lower(&verified, &source, Some(&mut restored)));
        assert_eq!(actual, expected, "current reservation prefix {work}");
    }
    let mut invalid = source.clone();
    invalid.functions[0].blocks[0].instructions[1].kind =
        InstructionKind::Constant(Constant::I32(901));
    let mut restored = request(&directory.0);
    assert!(lower(&verified, &invalid, Some(&mut restored)).is_err());
    assert_eq!(restored.hits, 0);
    // Context is complete raw data, including all request limits and headers.
    let mut changed_header = source.clone();
    changed_header.functions[0].name = "another".into();
    let mut session = incremental::Session::default();
    // A direct forged LIR cannot manufacture an admitted context.
    assert!(
        super::super::lower_output_with_raw_products(
            &verified,
            &changed_header.as_view(),
            &EmitOptions {
                entry: EntryPoint::None
            },
            true,
            false,
            1,
            Some(&mut session),
            false,
            Some(&mut request(&directory.0))
        )
        .is_err()
    );
}

#[test]
fn uncommitted_corrupt_and_guarded_records_are_only_misses() {
    let directory = Directory::new();
    let verified = fixture(false);
    let source = lir(&verified);
    let mut dropped = request(&directory.0);
    lower(&verified, &source, Some(&mut dropped)).unwrap();
    drop(dropped.pending());
    assert!(
        !directory
            .0
            .join("test-installed-identity/current.manifest")
            .exists()
    );
    let mut cold = request(&directory.0);
    lower(&verified, &source, Some(&mut cold)).unwrap();
    let pending = cold.pending().unwrap();
    let namespace = pending.directory.clone();
    let digest = Fingerprint::of(&pending.bytes);
    pending.commit(&directory.0);
    let payload = namespace.join(format!("{}.raw", digest.to_hex()));
    let original = fs::read(&payload).unwrap();
    fs::write(&payload, b"incomplete raw product").unwrap();
    let mut restored = request(&directory.0);
    assert_eq!(
        lower(&verified, &source, Some(&mut restored)),
        lower(&verified, &source, None)
    );
    assert_eq!(restored.hits, 0);
    fs::write(&payload, original).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&payload, fs::Permissions::from_mode(0o666)).unwrap();
        let mut restored = request(&directory.0);
        assert_eq!(
            lower(&verified, &source, Some(&mut restored)),
            lower(&verified, &source, None)
        );
        assert_eq!(restored.hits, 0);
    }
    // Exact body equality, not the alias/ID selector, admits a restored record.
    let mut raw = function(0, 3);
    let prior = raw.clone();
    raw.blocks[0].instructions[1].kind = InstructionKind::Constant(Constant::I32(4));
    assert!(!incremental::same_body_input(&prior, &raw));
}

#[test]
fn scalar_raw_codec_roundtrips_every_supported_field_and_refuses_truncation_and_quota() {
    let arg = || m::Arg {
        ty: m::Ty::Int(32),
        value: m::Val::Int(-9),
        attrs: vec![m::Attr::SignExt],
    };
    let operations = vec![
        m::Op::Alloca {
            ty: m::Ty::Int(32),
            align: 8,
        },
        m::Op::Load {
            ty: m::Ty::Int(32),
            ptr: m::Val::Local("slot".into()),
            align: 4,
        },
        m::Op::Store {
            arg: arg(),
            ptr: m::Val::Null,
            align: 2,
        },
        m::Op::Binary {
            opcode: "add".into(),
            ty: m::Ty::Int(32),
            left: m::Val::Undef,
            right: m::Val::Poison,
        },
        m::Op::Compare {
            predicate: "slt".into(),
            ty: m::Ty::Int(32),
            left: m::Val::Zero,
            right: m::Val::Int(i128::MIN),
        },
        m::Op::Cast {
            opcode: "sext".into(),
            arg: arg(),
            to: m::Ty::Int(64),
        },
        m::Op::Extract {
            arg: m::Arg::new(
                m::Ty::Struct(vec![m::Ty::Int(32), m::Ty::Int(1)]),
                m::Val::Local("pair".into()),
            ),
            index: 1,
        },
        m::Op::Insert {
            arg: arg(),
            field: arg(),
            index: 0,
        },
        m::Op::Call {
            result: m::Ty::Int(32),
            result_attrs: vec![m::Attr::ZeroExt],
            callee: m::Val::Global("callee".into()),
            args: vec![arg()],
            vararg: Some(vec![m::Ty::Ptr, m::Ty::Struct(vec![])]),
        },
        m::Op::Freeze(arg()),
        m::Op::Phi {
            ty: m::Ty::Int(32),
            incoming: vec![
                (m::Val::Int(9), "left".into()),
                (m::Val::Int(11), "right".into()),
            ],
        },
        m::Op::Branch("next".into()),
        m::Op::CondBranch {
            condition: m::Val::Bool(false),
            yes: "a".into(),
            no: "b".into(),
        },
        m::Op::Return(Some(arg())),
        m::Op::Return(None),
        m::Op::Unreachable,
        m::Op::Select {
            ty: m::Ty::Int(1),
            condition: m::Val::Bool(true),
            yes: m::Val::Bool(true),
            no: m::Val::Bool(false),
        },
    ];
    let output = FunctionBatch {
        body: vec![m::Item::Function(m::Function {
            name: "zil.fn.5".into(),
            linkage: m::Linkage::External,
            signature: m::Signature {
                result: m::Ty::Int(32),
                result_attrs: vec![m::Attr::SignExt],
                variadic: true,
                params: vec![
                    m::Param {
                        ty: m::Ty::Int(32),
                        name: Some("x".into()),
                        attrs: vec![m::Attr::ZeroExt],
                    },
                    m::Param::unnamed(m::Ty::Ptr),
                ],
            },
            attrs: vec![
                m::Attr::NonLazyBind,
                m::Attr::NoInline,
                m::Attr::NoUnwind,
                m::Attr::WillReturn,
                m::Attr::MemoryNone,
            ],
            blocks: Some(
                vec![m::Block {
                    name: "entry".into(),
                    instructions: operations
                        .into_iter()
                        .enumerate()
                        .map(|(index, op)| m::Inst {
                            result: (index % 2 == 0).then(|| format!("r{index}")),
                            op,
                        })
                        .collect(),
                }]
                .into(),
            ),
        })]
        .into(),
        intrinsics: BTreeSet::from([("sadd", 32), ("umul", 64)]),
        task_machines: vec![],
        foreign_adapters: BTreeSet::new(),
    };
    let bytes = raw_codec::encode(&output).unwrap();
    let limits = Limits::default();
    let mut nodes = limits.max_nodes;
    assert_eq!(
        raw_codec::decode(&bytes, &limits, &mut nodes),
        Some(output.clone())
    );
    assert!(nodes < limits.max_nodes);
    for length in 0..bytes.len() {
        let mut nodes = limits.max_nodes;
        assert!(raw_codec::decode(&bytes[..length], &limits, &mut nodes).is_none());
    }
    assert!(raw_codec::decode(&bytes, &limits, &mut 0).is_none());
    assert!(raw_codec::encode_bounded(&output, 64).is_none());
    let mut trailing = bytes;
    trailing.push(0);
    let mut nodes = limits.max_nodes;
    assert!(raw_codec::decode(&trailing, &limits, &mut nodes).is_none());
    let mut unsupported = output;
    let m::Item::Function(function) = &mut unsupported.body[0] else {
        unreachable!()
    };
    function.signature.result = m::Ty::F64;
    assert!(raw_codec::encode(&unsupported).is_none());
}

fn chain_fixture(edit: bool) -> VerifiedModule {
    let mut module = fixture(edit).module().clone();
    for id in 6..12 {
        module.functions[id].blocks[0].instructions[2].kind = InstructionKind::Call {
            function: FunctionId((id % 3) as u32),
            arguments: vec![ValueId(0)],
        };
    }
    checked(module)
}
fn classes(
    verified: &VerifiedModule,
    source: &LirModule,
    request: &mut Request,
    previous: Option<&crate::machine_call_classes::Cache>,
) -> (
    m::Module,
    BTreeSet<String>,
    crate::machine_call_classes::Cache,
) {
    let mut checker = crate::machine_preflight::CurrentPreflight::default();
    request.use_preflight(checker.body_session());
    let mut machine = lower(verified, source, Some(request)).unwrap().0;
    checker.check(&machine).unwrap(); // COMPLETE current unpruned model.
    let (_, symbols, cache) = crate::machine_call_classes::canonicalize_stable_cached(
        &mut machine,
        &BTreeSet::new(),
        &|| false,
        previous,
    )
    .unwrap();
    assert!(!symbols.is_empty());
    (machine, symbols, cache)
}

#[test]
fn cold_publish_exit_first_edit_restores_classes_and_actual_partition_consumers() {
    let directory = Directory::new();
    {
        let verified = chain_fixture(false);
        let source = lir(&verified);
        let mut cold = request(&directory.0);
        let (machine, symbols, cache) = classes(&verified, &source, &mut cold, None);
        let partition = crate::machine_cgu::partition_classes_reusing_with_policy(
            machine,
            &source.as_view(),
            EntryPoint::None,
            4,
            &symbols,
            None,
            None,
            crate::machine_cgu::CallImportPolicy::BoundedClasses(None),
        )
        .unwrap();
        cold.stage_product("call-classes", cache.encode_raw().unwrap());
        cold.stage_product(
            "class-partition",
            partition.materialization.unwrap().encode_raw().unwrap(),
        );
        // Nothing exists until the enclosing native publication succeeds.
        assert!(!cold.directory.exists());
        cold.pending().unwrap().commit(&directory.0);
    }
    // New Verify, raw models, immutable owners and Request. No warm request.
    let verified = chain_fixture(true);
    let source = lir(&verified);
    let mut restarted = request(&directory.0);
    let original = lower(&verified, &source, Some(&mut restarted)).unwrap().0;
    let cache = crate::machine_call_classes::Cache::decode_raw(
        &restarted.product("call-classes").unwrap(),
        verified.limits(),
    )
    .unwrap();
    let (actual, symbols, reused) = classes(&verified, &source, &mut restarted, Some(&cache));
    let mut expected = original;
    crate::machine_call_classes::canonicalize_stable_cached(
        &mut expected,
        &BTreeSet::new(),
        &|| false,
        None,
    )
    .unwrap();
    assert_eq!(actual, expected);
    assert!(reused.reused_keys > 0);
    assert!(
        reused.rebuilt_keys > 0,
        "the actual changed input must miss"
    );
    let policy = crate::machine_cgu::CallImportPolicy::BoundedClasses(None);
    let old = crate::machine_cgu::Materialization::decode_raw(
        &restarted.product("class-partition").unwrap(),
        verified.limits(),
        &source.as_view(),
        EntryPoint::None,
        4,
        policy,
    )
    .unwrap();
    let partition = crate::machine_cgu::partition_classes_reusing_with_policy(
        actual,
        &source.as_view(),
        EntryPoint::None,
        4,
        &symbols,
        None,
        Some(&old),
        policy,
    )
    .unwrap();
    let product = partition.materialization.unwrap();
    assert!(
        product.reused_items > 0,
        "actual CGU values must consume restored items"
    );
    assert!(
        product.constructed_items > 0,
        "the edited body cannot reuse stale bytes"
    );
    for unit in partition.plan.units {
        zil_llvm_api::validate_module(&unit).unwrap();
    }
    // The adapted successful epoch can itself be published, avoiding a history
    // chain that silently keeps consuming the original cold input forever.
    assert!(product.encode_raw().is_some());
}

#[test]
fn auxiliary_products_require_context_integrity_and_successful_publication() {
    let directory = Directory::new();
    let verified = fixture(false);
    let source = lir(&verified);
    let mut cancelled = request(&directory.0);
    lower(&verified, &source, Some(&mut cancelled)).unwrap();
    cancelled.stage_product("call-classes", vec![1, 2, 3]);
    drop(cancelled.pending()); // Failed/cancelled enclosing transaction.
    assert!(!directory.0.join("test-installed-identity").exists());
    let mut cold = request(&directory.0);
    lower(&verified, &source, Some(&mut cold)).unwrap();
    cold.stage_product("call-classes", vec![1, 2, 3]);
    cold.pending().unwrap().commit(&directory.0);
    let mut restarted = request(&directory.0);
    lower(&verified, &source, Some(&mut restarted)).unwrap();
    assert_eq!(restarted.product("call-classes"), Some(vec![1, 2, 3]));
    restarted.context.push(42);
    assert!(restarted.product("call-classes").is_none());
    restarted.context.pop();
    let manifest = read(&restarted.directory.join("call-classes.manifest"), 44).unwrap();
    let digest = Fingerprint(manifest[8..40].try_into().unwrap());
    let path = restarted.directory.join(format!("{}.raw", digest.to_hex()));
    let mut bytes = read(&path, MAX_BYTES).unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    fs::write(path, bytes).unwrap();
    assert!(restarted.product("call-classes").is_none());
}
