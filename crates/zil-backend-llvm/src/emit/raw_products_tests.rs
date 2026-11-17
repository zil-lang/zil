//! Root runs these with the aggregate backend window. No native SDK needed.
use super::*;
use zil_base::{ModuleId, SCHEMA_VERSION, Span};
use zil_ir::*;

fn same_view(left: &ByteView, right: &ByteView) -> bool {
    left.range == right.range
        && match (&left.owner, &right.owner) {
            (ByteOwner::Buffer(left), ByteOwner::Buffer(right)) => Arc::ptr_eq(left, right),
            (ByteOwner::Encoded(left), ByteOwner::Encoded(right)) => Arc::ptr_eq(left, right),
            _ => false,
        }
}

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
        products: BTreeMap::new(),
        old_products: BTreeMap::new(),
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

// Keep the producer's exact bound context, but load a fresh inventory without
// lowering first: these tests observe the very first demand after restart.
fn restart_inventory(cold: Request, root: &Path) -> Request {
    let mut restored = request(root);
    restored.context = cold.context.clone();
    restored.manifest = cold.manifest.clone();
    restored.limits = cold.limits.clone();
    restored.maximum = cold.maximum;
    cold.pending().unwrap().commit(root);
    let (records, aliases, products) = restored.load().unwrap();
    restored.old = records;
    restored.aliases = aliases;
    restored.old_products = products;
    restored.decode_nodes = restored.limits.max_nodes.min(1_000_000);
    restored.enabled = true;
    restored
}

fn decode_costs(record: &RawRecord, limits: &Limits) -> (usize, usize) {
    let (_, input) =
        zil_ir::wire::decode_raw_function_bounded(&record.input, limits, 1_000_000).unwrap();
    let mut remaining = 1_000_000;
    raw_codec::decode(&record.output, limits, &mut remaining).unwrap();
    (input, 1_000_000 - remaining)
}

#[test]
fn changed_inputs_defer_shared_output_until_an_unchanged_alias_demands_it() {
    let directory = Directory::new();
    let verified = fixture(false);
    let source = lir(&verified);
    let mut cold = request(&directory.0);
    lower(&verified, &source, Some(&mut cold)).unwrap();
    let mut restored = restart_inventory(cold, &directory.0);
    let checker = crate::machine_preflight::CurrentPreflight::default();
    restored.use_preflight(checker.body_session());
    let current = fixture(true);
    let functions = current
        .module()
        .functions
        .iter()
        .map(|function| (function.id, function))
        .collect();
    let summaries = BTreeMap::new();
    let index = restored.aliases[&FunctionId(0)];
    assert_eq!(restored.aliases[&FunctionId(3)], index);
    let (input_cost, output_cost) = decode_costs(&restored.old[index], &restored.limits);
    let budget = restored.decode_nodes;
    // Body, locals, value types and header differences all precede output
    // decoding. Repeated changed aliases must not reject the shared record.
    for fault in 0..4 {
        let mut changed = source.functions[0].clone();
        match fault {
            0 => changed = current.module().functions[0].clone(),
            1 => changed.locals[0].mutable = true,
            2 => changed.values[0] = Type::I64,
            _ => changed.return_type = Type::I64,
        }
        assert!(
            restored
                .lookup(&changed, Some(&summaries), &functions, &source.target)
                .is_none()
        );
        assert_eq!(restored.decode_nodes, budget - input_cost);
        assert!(matches!(
            restored.decoded[&index].as_ref().unwrap().output,
            DecodedOutput::Pending
        ));
        assert!(restored.accepted.is_empty());
        assert_eq!(checker.body_session().borrow().body_counts(), (0, 0));
    }
    let first = restored
        .lookup(
            &source.functions[3],
            Some(&summaries),
            &functions,
            &source.target,
        )
        .unwrap();
    let second = restored
        .lookup(
            &source.functions[6],
            Some(&summaries),
            &functions,
            &source.target,
        )
        .unwrap();
    assert_eq!(restored.decode_nodes, budget - input_cost - output_cost);
    assert_eq!(checker.body_session().borrow().body_counts(), (1, 0));
    assert_eq!(restored.decoded.len(), 1, "unused records stay encoded");
    let [m::Item::Function(first)] = first.body.as_slice() else {
        unreachable!()
    };
    let [m::Item::Function(second)] = second.body.as_slice() else {
        unreachable!()
    };
    assert_eq!(first.name, symbol(FunctionId(3)));
    assert_eq!(second.name, symbol(FunctionId(6)));
    assert!(
        first
            .blocks
            .as_ref()
            .unwrap()
            .ptr_eq(second.blocks.as_ref().unwrap())
    );
    // A later changed lookup cannot consume an already decoded shared result.
    assert!(
        restored
            .lookup(
                &current.module().functions[0],
                Some(&summaries),
                &functions,
                &source.target
            )
            .is_none()
    );
    assert!(!restored.accepted.contains_key(&FunctionId(0)));
    assert_eq!(restored.decode_nodes, budget - input_cost - output_cost);
}

#[test]
fn changed_facts_leave_both_input_and_output_undecoded() {
    let directory = Directory::new();
    let verified = chain_fixture(false);
    let source = lir(&verified);
    let functions = source.functions.iter().map(|f| (f.id, f)).collect();
    let mut cold = request(&directory.0);
    lower(&verified, &source, Some(&mut cold)).unwrap();
    let mut restored = restart_inventory(cold, &directory.0);
    let budget = restored.decode_nodes;
    let changed = BTreeMap::from([(FunctionId(0), Constant::I32(7))]);
    for summaries in [None, Some(&changed)] {
        assert!(
            restored
                .lookup(&source.functions[6], summaries, &functions, &source.target)
                .is_none()
        );
        assert_eq!(restored.decode_nodes, budget);
        assert!(restored.decoded.is_empty());
        assert!(restored.accepted.is_empty());
    }
    assert!(
        restored
            .lookup(
                &source.functions[9],
                Some(&BTreeMap::new()),
                &functions,
                &source.target
            )
            .is_some()
    );
    assert_eq!(restored.decoded.len(), 1);
}

#[test]
fn corrupt_raw_input_or_demanded_output_exhausts_decode_budget_and_stays_rejected() {
    let directory = Directory::new();
    let verified = fixture(false);
    let source = lir(&verified);
    let functions = source.functions.iter().map(|f| (f.id, f)).collect();
    let summaries = BTreeMap::new();
    for corrupt_input in [true, false] {
        let mut cold = request(&directory.0);
        lower(&verified, &source, Some(&mut cold)).unwrap();
        let index = cold.current_aliases[&FunctionId(0)];
        let bytes = if corrupt_input {
            &mut cold.current[index].input
        } else {
            &mut cold.current[index].output
        };
        *bytes = bytes[..bytes.len() - 1].to_vec().into();
        // Publish a checksum-correct envelope with a truncated inner record.
        let mut restored = restart_inventory(cold, &directory.0);
        let mut changed = source.functions[0].clone();
        changed.blocks[0].instructions[1].kind = InstructionKind::Constant(Constant::I32(99));
        assert!(
            restored
                .lookup(&changed, Some(&summaries), &functions, &source.target)
                .is_none()
        );
        if corrupt_input {
            assert_eq!(restored.decode_nodes, 0);
            assert!(restored.decoded[&index].is_none());
        } else {
            assert!(
                restored.decode_nodes > 0,
                "unused corrupt output is not decoded"
            );
            assert!(matches!(
                restored.decoded[&index].as_ref().unwrap().output,
                DecodedOutput::Pending
            ));
        }
        for id in [3, 6, 1] {
            assert!(
                restored
                    .lookup(
                        &source.functions[id],
                        Some(&summaries),
                        &functions,
                        &source.target
                    )
                    .is_none()
            );
            assert_eq!(restored.decode_nodes, 0);
        }
        if !corrupt_input {
            assert!(matches!(
                restored.decoded[&index].as_ref().unwrap().output,
                DecodedOutput::Rejected
            ));
        }
        assert!(restored.accepted.is_empty());
    }
}

#[test]
fn lazy_raw_decoders_share_exact_aggregate_budget_and_memoize_failures() {
    let directory = Directory::new();
    let verified = fixture(false);
    let source = lir(&verified);
    let functions = source.functions.iter().map(|f| (f.id, f)).collect();
    let summaries = BTreeMap::new();
    let mut changed = source.functions[0].clone();
    changed.blocks[0].instructions[1].kind = InstructionKind::Constant(Constant::I32(99));
    let mut cold = request(&directory.0);
    lower(&verified, &source, Some(&mut cold)).unwrap();
    let index = cold.current_aliases[&FunctionId(0)];
    let (input, output) = decode_costs(&cold.current[index], &cold.limits);
    let other = cold.current_aliases[&FunctionId(1)];
    let (other_input, other_output) = decode_costs(&cold.current[other], &cold.limits);
    // Republish the same immutable records for independent request budgets.
    for budget in [input - 1, input, input + output - 1, input + output] {
        let mut producer = request(&directory.0);
        lower(&verified, &source, Some(&mut producer)).unwrap();
        let mut restored = restart_inventory(producer, &directory.0);
        restored.decode_nodes = budget;
        assert!(
            restored
                .lookup(&changed, Some(&summaries), &functions, &source.target)
                .is_none()
        );
        assert_eq!(restored.decode_nodes, budget.saturating_sub(input));
        for id in [3, 6] {
            assert_eq!(
                restored
                    .lookup(
                        &source.functions[id],
                        Some(&summaries),
                        &functions,
                        &source.target
                    )
                    .is_some(),
                budget == input + output
            );
            assert_eq!(restored.decode_nodes, 0);
        }
        // A failure on another record cannot evict already decoded results;
        // rejected records cannot retry even if spare budget becomes available.
        assert!(
            restored
                .lookup(
                    &source.functions[1],
                    Some(&summaries),
                    &functions,
                    &source.target
                )
                .is_none()
        );
        restored.decode_nodes = 1_000_000;
        assert_eq!(
            restored
                .lookup(
                    &source.functions[3],
                    Some(&summaries),
                    &functions,
                    &source.target
                )
                .is_some(),
            budget == input + output
        );
        assert!(
            restored
                .lookup(
                    &source.functions[1],
                    Some(&summaries),
                    &functions,
                    &source.target
                )
                .is_none()
        );
        assert_eq!(restored.decode_nodes, 1_000_000);
    }
    let mut restored = restart_inventory(cold, &directory.0);
    restored.decode_nodes = input + other_input + other_output + output - 1;
    assert!(
        restored
            .lookup(&changed, Some(&summaries), &functions, &source.target)
            .is_none()
    );
    assert!(
        restored
            .lookup(
                &source.functions[1],
                Some(&summaries),
                &functions,
                &source.target
            )
            .is_some()
    );
    assert_eq!(restored.decode_nodes, output - 1);
    assert!(
        restored
            .lookup(
                &source.functions[3],
                Some(&summaries),
                &functions,
                &source.target
            )
            .is_none()
    );
    assert_eq!(restored.decode_nodes, 0);
}

#[test]
fn raw_staging_binds_lazy_aliases_and_retains_only_exact_accepted_batches() {
    let directory = Directory::new();
    let verified = fixture(false);
    let source = lir(&verified);
    let functions = source.functions.iter().map(|f| (f.id, f)).collect();
    let summaries = BTreeMap::new();
    let function = &source.functions[3];
    for lazy in [false, true] {
        let mut cold = request(&directory.0);
        lower(&verified, &source, Some(&mut cold)).unwrap();
        let mut restored = restart_inventory(cold, &directory.0);
        let accepted = restored
            .lookup(function, Some(&summaries), &functions, &source.target)
            .unwrap();
        let index = restored.aliases[&function.id];
        let old = restored.old[index].clone();
        let mut output = if lazy {
            let DecodedOutput::Ready(output) = &restored.decoded[&index].as_ref().unwrap().output
            else {
                unreachable!()
            };
            let mut output = output.clone();
            output.alias = Some(symbol(function.id));
            output
        } else {
            accepted.clone()
        };
        let alias = output.alias.clone();
        output.alias = Some(symbol(FunctionId(99)));
        restored.stage(
            function,
            &output,
            scalar_bodies::key(None, function),
            Some(&summaries),
        );
        assert!(
            restored.current.is_empty(),
            "wrong logical symbol is refused"
        );
        output.alias = alias;
        let unchanged = output.clone();
        restored.stage(
            function,
            &output,
            scalar_bodies::key(None, function),
            Some(&summaries),
        );
        assert_eq!(
            output, unchanged,
            "staging cannot mutate the shared template"
        );
        assert_eq!(restored.accepted[&function.id].1, accepted);
        let record = &restored.current[0];
        assert_eq!(same_view(&record.input, &old.input), !lazy);
        assert_eq!(same_view(&record.output, &old.output), !lazy);
        let (input, _) =
            zil_ir::wire::decode_raw_function_bounded(&record.input, &restored.limits, 1_000_000)
                .unwrap();
        let decoded = raw_codec::decode(&record.output, &restored.limits, &mut 1_000_000).unwrap();
        assert!(decoded.alias.is_none());
        assert_eq!(decoded.body_name(), Some(symbol(input.id).as_str()));
        if lazy {
            assert_eq!(input.id, function.id);
        }
    }
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
                .any(|old| same_view(&current.input, &old.input)
                    && same_view(&current.output, &old.output)
                    && same_view(&current.facts, &old.facts)))
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
    let digest = pending.bytes.digest();
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
        alias: None,
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
        let (bytes, _, _) = crate::machine_cache::encode_products(
            &cache,
            &partition.materialization.unwrap(),
            None,
        )
        .unwrap();
        cold.stage_product("machine-products", bytes);
        // Nothing exists until the enclosing native publication succeeds.
        assert!(!cold.directory.exists());
        cold.pending().unwrap().commit(&directory.0);
    }
    // New Verify, raw models, immutable owners and Request. No warm request.
    let verified = chain_fixture(true);
    let source = lir(&verified);
    let mut restarted = request(&directory.0);
    let original = lower(&verified, &source, Some(&mut restarted)).unwrap().0;
    let bytes = restarted.product("machine-products").unwrap();
    let mut recovery = crate::machine_cache::Recovery::new(&bytes, verified.limits()).unwrap();
    let cache = recovery.classes(&original).unwrap();
    let (actual, symbols, reused) = classes(&verified, &source, &mut restarted, Some(&cache));
    let mut expected = original.clone();
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
    let old = recovery
        .partition(&source.as_view(), EntryPoint::None, 4, policy)
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
    let (bytes, encoded, retained) =
        crate::machine_cache::encode_products(&reused, &product, Some(&recovery)).unwrap();
    assert!(
        retained > 0,
        "unchanged actual models must retain their encoding bytes"
    );
    assert!(encoded > 0, "edited models must be encoded anew");
    assert!(
        encoded < retained,
        "a private edit cannot re-encode the entire inventory"
    );
    let mut next = crate::machine_cache::Recovery::new(&bytes, verified.limits()).unwrap();
    assert!(next.classes(&original).is_some());
}

#[test]
fn auxiliary_products_require_context_integrity_and_successful_publication() {
    let directory = Directory::new();
    let verified = fixture(false);
    let source = lir(&verified);
    let mut cancelled = request(&directory.0);
    lower(&verified, &source, Some(&mut cancelled)).unwrap();
    cancelled.stage_product("machine-products", vec![1, 2, 3]);
    drop(cancelled.pending());
    assert!(!directory.0.join("test-installed-identity").exists());
    let mut cold = request(&directory.0);
    lower(&verified, &source, Some(&mut cold)).unwrap();
    cold.stage_product("machine-products", vec![1, 2, 3]);
    cold.pending().unwrap().commit(&directory.0);
    let mut restarted = request(&directory.0);
    lower(&verified, &source, Some(&mut restarted)).unwrap();
    assert_eq!(
        restarted.product("machine-products").as_deref(),
        Some([1, 2, 3].as_slice())
    );
    restarted.context.push(42);
    assert!(restarted.load().is_none());
    restarted.context.pop();
    let manifest = read(&restarted.directory.join("current.manifest"), 44).unwrap();
    let digest = Fingerprint(manifest[8..40].try_into().unwrap());
    let path = restarted.directory.join(format!("{}.raw", digest.to_hex()));
    let mut bytes = read(&path, MAX_BYTES).unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    fs::write(path, bytes).unwrap();
    let mut corrupted = request(&directory.0);
    lower(&verified, &source, Some(&mut corrupted)).unwrap();
    assert!(corrupted.product("machine-products").is_none());
    assert!(
        corrupted.hits == 0,
        "corrupt shared envelope cannot lend emission bodies either"
    );
}

// The prior contiguous serializer is an independent byte-schema oracle.
fn contiguous_payload(request: &Request) -> Vec<u8> {
    let mut out = Writer::bounded(request.maximum);
    out.put(b"ZILRAE02").unwrap();
    out.blob(&request.context).unwrap();
    out.word(request.current.len()).unwrap();
    for record in &request.current {
        out.blob(&record.input).unwrap();
        out.blob(&record.facts).unwrap();
        out.blob(&record.output).unwrap();
    }
    out.word(request.current_aliases.len()).unwrap();
    for (id, index) in &request.current_aliases {
        out.word(id.0 as usize).unwrap();
        out.word(*index).unwrap();
    }
    let products = if request.products.is_empty() {
        &request.old_products
    } else {
        &request.products
    };
    out.word(products.len()).unwrap();
    for (name, bytes) in products {
        out.text(name).unwrap();
        out.blob(bytes).unwrap();
    }
    out.0
}

#[test]
fn shared_file_views_survive_restore_stage_and_byte_identical_streamed_republication() {
    let directory = Directory::new();
    let verified = fixture(false);
    let source = lir(&verified);
    let mut cold = request(&directory.0);
    let expected_machine = lower(&verified, &source, Some(&mut cold)).unwrap();
    let product = vec![37; 128 * 1024];
    let product_ptr = product.as_ptr();
    cold.stage_product("machine-products", product);
    assert_eq!(cold.products["machine-products"].as_ptr(), product_ptr);
    let originals = cold.current.clone();
    let expected = contiguous_payload(&cold);
    let pending = cold.pending().unwrap();
    assert_eq!(pending.bytes.length, expected.len());
    assert_eq!(pending.bytes.digest(), Fingerprint::of(&expected));
    for record in &originals {
        for bytes in [&record.input, &record.facts, &record.output] {
            assert!(
                pending
                    .bytes
                    .pieces
                    .iter()
                    .any(|piece| same_view(piece, bytes))
            );
        }
    }
    assert!(
        pending
            .bytes
            .pieces
            .iter()
            .any(|piece| piece.as_ptr() == product_ptr)
    );
    let namespace = pending.directory.clone();
    let path = namespace.join(format!("{}.raw", pending.bytes.digest().to_hex()));
    pending.commit(&directory.0);
    assert_eq!(fs::read(&path).unwrap(), expected);
    let manifest = read(&namespace.join("current.manifest"), 44).unwrap();
    assert_eq!(&manifest[..8], b"ZILRAM01");
    assert_eq!(&manifest[8..40], &Fingerprint::of(&expected).0);
    assert_eq!(
        u32::from_le_bytes(manifest[40..44].try_into().unwrap()) as usize,
        expected.len()
    );

    let mut restored = request(&directory.0);
    assert_eq!(
        lower(&verified, &source, Some(&mut restored)).unwrap(),
        expected_machine
    );
    let product = restored.product("machine-products").unwrap();
    let ByteOwner::Buffer(owner) = &product.owner else {
        panic!("restored file owner")
    };
    let owner = owner.clone();
    assert_eq!(owner.as_slice(), expected);
    for record in &restored.old {
        for bytes in [&record.input, &record.facts, &record.output] {
            let ByteOwner::Buffer(buffer) = &bytes.owner else {
                panic!("one shared file allocation")
            };
            assert!(Arc::ptr_eq(buffer, &owner));
            assert_eq!(bytes.as_ptr(), owner[bytes.range.clone()].as_ptr());
        }
    }
    let expected_again = contiguous_payload(&restored);
    let pending = restored.pending().unwrap();
    assert_eq!(expected_again, expected);
    assert_eq!(pending.bytes.digest(), Fingerprint::of(&expected));
    assert!(
        pending
            .bytes
            .pieces
            .iter()
            .any(|piece| same_view(piece, &product))
    );
    assert!(matches_payload(&path, MAX_BYTES, &pending.bytes));
    // A retained owner cannot conceal current corruption or a trailing byte.
    let mut corrupted = expected.clone();
    *corrupted.last_mut().unwrap() ^= 1;
    fs::write(&path, &corrupted).unwrap();
    assert!(!matches_payload(&path, MAX_BYTES, &pending.bytes));
    corrupted = expected.clone();
    corrupted.push(0);
    fs::write(&path, corrupted).unwrap();
    assert!(!matches_payload(&path, MAX_BYTES, &pending.bytes));
    pending.commit(&directory.0);
    assert_eq!(fs::read(&path).unwrap(), expected);
    // The returned product is owned independently of Request and Pending.
    assert_eq!(product.as_ref(), &[37; 128 * 1024]);
}

#[test]
fn borrowed_function_decode_keeps_exact_consumption_and_remaining_node_limits() {
    let limits = Limits::default();
    let encoded = zil_ir::wire::encode_raw_function(&function(3, 7), &limits)
        .unwrap()
        .shared_bytes();
    let (expected, receipt) =
        zil_ir::wire::decode_raw_function_shared_owner(encoded.clone(), &limits, limits.max_nodes)
            .unwrap();
    let nodes = receipt.node_cost();
    assert!(nodes > 0);
    let mut enclosing = vec![91; 7];
    enclosing.extend_from_slice(&encoded);
    enclosing.extend_from_slice(&[92; 9]);
    let owner = Arc::new(enclosing);
    let view = ByteView::range(&owner, 7..7 + encoded.len()).unwrap();
    let count = Arc::strong_count(&owner);
    let (actual, cost) = zil_ir::wire::decode_raw_function_bounded(&view, &limits, nodes).unwrap();
    assert_eq!(&actual, expected.as_ref());
    assert_eq!(cost, nodes);
    assert_eq!(Arc::strong_count(&owner), count);
    assert_eq!(view.as_ptr(), owner[7..].as_ptr());
    assert!(zil_ir::wire::decode_raw_function_bounded(&view, &limits, nodes - 1).is_err());
    let short = Limits {
        max_nodes: nodes - 1,
        ..limits.clone()
    };
    assert!(zil_ir::wire::decode_raw_function_bounded(&view, &short, usize::MAX).is_err());
    let short = Limits {
        max_source_bytes: view.len() - 1,
        ..limits.clone()
    };
    assert!(zil_ir::wire::decode_raw_function_bounded(&view, &short, nodes).is_err());
    assert!(
        zil_ir::wire::decode_raw_function_bounded(&view[..view.len() - 1], &limits, nodes).is_err()
    );
    assert!(zil_ir::wire::decode_raw_function_bounded(&owner[7..], &limits, nodes).is_err());
    assert!(ByteView::range(&owner, 0..usize::MAX).is_none());
}

#[test]
fn segmented_payload_counts_repeated_views_and_atomic_failures_leave_old_bytes() {
    let segment: ByteView = vec![7; 64 * 1024].into();
    let mut out = PayloadWriter::new(MAX_BYTES);
    for _ in 0..255 {
        out.blob(segment.clone()).unwrap();
    }
    // 256 references still cost their full logical lengths, despite sharing
    // one 64 KiB allocation. The original aggregate 16 MiB cap is unchanged.
    assert!(out.blob(segment).is_none());

    let bytes: ByteView = vec![1, 2, 3, 4].into();
    let mut exact = PayloadWriter::new(8);
    exact.blob(bytes.clone()).unwrap();
    let exact = exact.finish().unwrap();
    assert_eq!(exact.length, 8);
    assert!(PayloadWriter::new(7).blob(bytes).is_none());
    let directory = Directory::new();
    atomic(&directory.0, "selected", b"old").unwrap();
    assert!(atomic_parts(&directory.0, "selected", 7, exact.slices()).is_none());
    assert!(atomic_parts(&directory.0, "selected", 9, exact.slices()).is_none());
    assert!(atomic_parts(&directory.0, "selected", MAX_BYTES + 1, exact.slices()).is_none());
    assert_eq!(fs::read(directory.0.join("selected")).unwrap(), b"old");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
    #[cfg(unix)]
    {
        let link = directory.0.join("linked");
        std::os::unix::fs::symlink(directory.0.join("selected"), &link).unwrap();
        assert!(read(&link, MAX_BYTES).is_none());
        assert!(!matches_payload(&link, MAX_BYTES, &exact));
        assert!(atomic_parts(&directory.0, "linked", exact.length, exact.slices()).is_none());
        assert_eq!(fs::read(directory.0.join("selected")).unwrap(), b"old");
    }
}
