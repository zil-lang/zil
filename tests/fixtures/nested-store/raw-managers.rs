// Complete raw candidates and handwritten Holds. No producer or Analyze
// helper is used to construct the two independently owned prefix domains.
use zil_base::{FileId, Fingerprint, ModuleId, SCHEMA_VERSION, Span, TargetProfile};
use zil_ir::*;

fn owner(element: Type) -> Type {
    Type::Managed(Box::new(Type::OwnedStore {
        element: Box::new(element),
    }))
}
fn desc(element: Type) -> Type {
    Type::Desc(Box::new(Type::OwnedStore {
        element: Box::new(element),
    }))
}
fn address(inner: Type, mutable: bool) -> Type {
    Type::Address {
        mutable,
        inner: Box::new(inner),
    }
}
fn leaf() -> Type {
    Type::Record {
        name: "NestedLeaf".into(),
        fields: vec![("value".into(), Type::I64)],
    }
}
fn row() -> Type {
    Type::Package {
        name: "NestedRow".into(),
        fields: vec![("inner".into(), owner(leaf()))],
    }
}
#[derive(Clone, Copy, Default)]
pub struct Scenario {
    pub mutable_parent: bool,
    pub mutable_leaf: bool,
    pub reserve_before: bool,
    pub reserve_inner_live: bool,
    pub reserve_outer_live: bool,
    pub retire_outer_live: bool,
    pub through_helper: bool,
}
struct Body<'a> {
    function: &'a mut TypedFunction,
    holds: Vec<Vec<HoldRoot>>,
}
impl Body<'_> {
    fn op(&mut self, kind: InstructionKind) {
        self.function.blocks[0].instructions.push(Instruction {
            result: None,
            kind,
            span: self.function.span,
        });
    }
    fn value(&mut self, ty: Type, kind: InstructionKind, root: Option<u32>) -> ValueId {
        let value = ValueId(self.function.values.len() as u32);
        let description = matches!(ty, Type::Desc(_));
        self.function.values.push(ty);
        self.holds.push(if description {
            Vec::new()
        } else {
            root.map(|local| vec![HoldRoot::Local(LocalId(local))])
                .unwrap_or_default()
        });
        self.function.blocks[0].instructions.push(Instruction {
            result: Some(value),
            kind,
            span: self.function.span,
        });
        value
    }
}
pub fn request(scenario: Scenario) -> (TypedModule, CandidatePlan) {
    let span = Span::new(FileId(0), 0, 1);
    let mut module = TypedModule {
        dyn_declarations: Vec::new(),
        dyn_interfaces: Vec::new(),
        dyn_vtables: Vec::new(),
        life_declarations: Vec::new(),
        life_instances: Vec::new(),
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"finite nested Store public manager input"),
        target: TargetProfile::host_llvm(),
        closures: Vec::new(),
        unsupported: Vec::new(),
        resources: vec![
            ResourceDefinition {
                name: "@builtin:OwnedStore<NestedRow>".into(),
                payload: Type::OwnedStore {
                    element: Box::new(row()),
                },
                teardown: None,
            },
            ResourceDefinition {
                name: "@builtin:OwnedStore<NestedLeaf>".into(),
                payload: Type::OwnedStore {
                    element: Box::new(leaf()),
                },
                teardown: None,
            },
        ],
        functions: vec![TypedFunction {
            dyn_instance: None,
            retained_return_sources: None,
            storage_life: None,
            call_kind: CallKind::Direct,
            resume_bound: TaskResumeBound::Local,
            reference_return_bound: None,
            callable_return_contract: None,
            callable_contract: None,
            protocol_contracts: Vec::new(),
            result_mode: PassingMode::Infer,
            foreign: None,
            environment: None,
            id: FunctionId(0),
            name: "observe".into(),
            parameters: Vec::new(),
            return_type: Type::I64,
            locals: vec![LocalDecl {
                id: LocalId(0),
                name: "base".into(),
                ty: owner(row()),
                mutable: true,
                span,
            }],
            values: Vec::new(),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: Vec::new(),
                terminator: Terminator::Return(None),
            }],
            entry: BlockId(0),
            span,
        }],
    };
    let mut plan = CandidatePlan::empty_for(&module);
    plan.functions.push(FunctionPlan {
        function: FunctionId(0),
        holds: Vec::new(),
        blocks: vec![BlockPlan {
            block: BlockId(0),
            initialized_in: Vec::new(),
            initialized_out: Vec::new(),
        }],
    });
    let f = &mut module.functions[0];
    let base = f.locals[0].clone();
    let outcome = Type::ErrorUnion {
        error: Box::new(row()),
        result: Box::new(Type::Unit),
    };
    f.locals = [owner(leaf()), owner(row()), outcome.clone()]
        .into_iter()
        .enumerate()
        .map(|(index, ty)| LocalDecl {
            id: LocalId(index as u32),
            name: format!("slot{index}"),
            ty,
            ..base.clone()
        })
        .collect();
    f.values.clear();
    f.blocks[0].instructions.clear();
    f.return_type = Type::I64;
    let mut b = Body {
        function: f,
        holds: Vec::new(),
    };
    b.op(InstructionKind::ScopeEnter { scope: ScopeId(0) });
    for local in 0..3 {
        b.op(InstructionKind::StorageLive(LocalId(local)));
    }
    let made = b.value(
        owner(leaf()),
        InstructionKind::OwnedStoreNew { element: leaf() },
        None,
    );
    b.op(InstructionKind::Store {
        local: LocalId(0),
        value: made,
    });
    let inner = b.value(
        desc(leaf()),
        InstructionKind::DescribeLocal { local: LocalId(0) },
        Some(0),
    );
    let one = b.value(Type::I64, InstructionKind::Constant(Constant::I64(1)), None);
    b.value(
        Type::Bool,
        InstructionKind::OwnedStoreReserve {
            store: inner,
            capacity: one,
        },
        None,
    );
    let number = b.value(
        Type::I64,
        InstructionKind::Constant(Constant::I64(42)),
        None,
    );
    let item = b.value(
        leaf(),
        InstructionKind::MakeAggregate {
            fields: vec![number],
        },
        None,
    );
    b.value(
        Type::ErrorUnion {
            error: Box::new(leaf()),
            result: Box::new(Type::Unit),
        },
        InstructionKind::OwnedStorePush {
            store: inner,
            value: item,
        },
        None,
    );
    let outer = b.value(
        owner(row()),
        InstructionKind::OwnedStoreNew { element: row() },
        None,
    );
    b.op(InstructionKind::Store {
        local: LocalId(1),
        value: outer,
    });
    let outer = b.value(
        desc(row()),
        InstructionKind::DescribeLocal { local: LocalId(1) },
        Some(1),
    );
    b.value(
        Type::Bool,
        InstructionKind::OwnedStoreReserve {
            store: outer,
            capacity: one,
        },
        None,
    );
    let moved = b.value(
        owner(leaf()),
        InstructionKind::TakeLocal { local: LocalId(0) },
        None,
    );
    let element = b.value(
        row(),
        InstructionKind::MakePackage {
            fields: vec![moved],
        },
        None,
    );
    let pushed = b.value(
        outcome,
        InstructionKind::OwnedStorePush {
            store: outer,
            value: element,
        },
        None,
    );
    b.op(InstructionKind::Store {
        local: LocalId(2),
        value: pushed,
    });
    b.op(InstructionKind::DropErrorUnion { local: LocalId(2) });
    b.op(InstructionKind::StorageDead(LocalId(2)));
    b.op(InstructionKind::StorageDead(LocalId(0)));
    let zero = b.value(Type::I64, InstructionKind::Constant(Constant::I64(0)), None);
    let borrow = if scenario.mutable_parent {
        InstructionKind::OwnedStoreBorrowMut {
            store: outer,
            index: zero,
        }
    } else {
        InstructionKind::OwnedStoreBorrow {
            store: outer,
            index: zero,
        }
    };
    let parent = b.value(address(row(), scenario.mutable_parent), borrow, Some(1));
    let header = b.value(
        address(owner(leaf()), scenario.mutable_parent),
        InstructionKind::AddressField {
            address: parent,
            index: 0,
        },
        Some(1),
    );
    let nested = b.value(
        desc(leaf()),
        InstructionKind::DescribeAddress { address: header },
        Some(1),
    );
    if scenario.reserve_before {
        let kind = if scenario.through_helper {
            InstructionKind::Call {
                function: FunctionId(1),
                arguments: vec![nested],
            }
        } else {
            InstructionKind::OwnedStoreReserve {
                store: nested,
                capacity: one,
            }
        };
        b.value(Type::Bool, kind, None);
    }
    let borrow = if scenario.mutable_leaf {
        InstructionKind::OwnedStoreBorrowMut {
            store: nested,
            index: zero,
        }
    } else {
        InstructionKind::OwnedStoreBorrow {
            store: nested,
            index: zero,
        }
    };
    let child = b.value(address(leaf(), scenario.mutable_leaf), borrow, Some(1));
    let pointer = b.value(
        address(Type::I64, scenario.mutable_leaf),
        InstructionKind::AddressField {
            address: child,
            index: 0,
        },
        Some(1),
    );
    if scenario.reserve_inner_live {
        b.value(
            Type::Bool,
            InstructionKind::OwnedStoreReserve {
                store: nested,
                capacity: one,
            },
            None,
        );
    }
    if scenario.reserve_outer_live {
        b.value(
            Type::Bool,
            InstructionKind::OwnedStoreReserve {
                store: outer,
                capacity: one,
            },
            None,
        );
    }
    if scenario.retire_outer_live {
        b.op(InstructionKind::DropManaged { local: LocalId(1) });
        b.op(InstructionKind::StorageDead(LocalId(1)));
    }
    let read = b.value(
        Type::I64,
        InstructionKind::ReadAddress { address: pointer },
        None,
    );
    if !scenario.retire_outer_live {
        b.op(InstructionKind::DropManaged { local: LocalId(1) });
        b.op(InstructionKind::StorageDead(LocalId(1)));
    }
    b.op(InstructionKind::ScopeExit {
        scope: ScopeId(0),
        kind: CleanupExitKind::Return,
    });
    b.function.blocks[0].terminator = Terminator::Return(Some(read));
    plan.functions[0].holds = b
        .holds
        .into_iter()
        .enumerate()
        .map(|(index, roots)| ValueHold {
            value: ValueId(index as u32),
            roots,
        })
        .collect();
    plan.module_fingerprint = module.fingerprint();
    if scenario.through_helper {
        let mut helper = module.functions[0].clone();
        helper.id = FunctionId(1);
        helper.name = "grow".into();
        helper.parameters = vec![LocalId(0)];
        helper.return_type = Type::Bool;
        helper.locals = vec![LocalDecl {
            id: LocalId(0),
            name: "items".into(),
            ty: desc(leaf()),
            mutable: false,
            ..helper.locals[0].clone()
        }];
        helper.values = vec![desc(leaf()), Type::I64, Type::Bool];
        let span = helper.span;
        helper.blocks = vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction {
                    result: None,
                    kind: InstructionKind::ScopeEnter { scope: ScopeId(0) },
                    span,
                },
                Instruction {
                    result: Some(ValueId(0)),
                    kind: InstructionKind::Load(LocalId(0)),
                    span,
                },
                Instruction {
                    result: Some(ValueId(1)),
                    kind: InstructionKind::Constant(Constant::I64(4)),
                    span,
                },
                Instruction {
                    result: Some(ValueId(2)),
                    kind: InstructionKind::OwnedStoreReserve {
                        store: ValueId(0),
                        capacity: ValueId(1),
                    },
                    span,
                },
                Instruction {
                    result: None,
                    kind: InstructionKind::StorageDead(LocalId(0)),
                    span,
                },
                Instruction {
                    result: None,
                    kind: InstructionKind::ScopeExit {
                        scope: ScopeId(0),
                        kind: CleanupExitKind::Return,
                    },
                    span,
                },
            ],
            terminator: Terminator::Return(Some(ValueId(2))),
        }];
        module.functions.push(helper);
        plan.functions.push(FunctionPlan {
            function: FunctionId(1),
            holds: vec![
                ValueHold {
                    value: ValueId(0),
                    // Actual Store description: private manager identity,
                    // with no public strong Address debt.
                    roots: Vec::new(),
                },
                ValueHold {
                    value: ValueId(1),
                    roots: vec![],
                },
                ValueHold {
                    value: ValueId(2),
                    roots: vec![],
                },
            ],
            blocks: vec![BlockPlan {
                block: BlockId(0),
                initialized_in: vec![LocalId(0)],
                initialized_out: vec![],
            }],
        });
        plan.module_fingerprint = module.fingerprint();
    }
    (module, plan)
}
