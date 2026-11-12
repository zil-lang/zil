//! A sealed runtime prefix. Whole elements move as their ordinary inline ABI.
//! No element pointers or mutable head fields escape these checked operations.
use super::*;
use zil_base::Limits;

pub(super) fn inline_protocol_element(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Record { .. }
            | Type::Tuple(_)
            | Type::Array { .. }
            | Type::Bool
            | Type::U8
            | Type::U16
            | Type::I32
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::F32
            | Type::F64
    )
}

/// Private representation plans, built only after exact verified-LIR binding.
/// Classify each complete protocol rather than assuming the element size.
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
pub(super) struct Protocols {
    push: Option<copy_aggregates::Slot>,
    pop: Option<copy_aggregates::Slot>,
    stride: u64,
    shape_work: usize,
}
impl Protocols {
    pub(super) fn large(&self) -> bool {
        self.push.is_some() || self.pop.is_some()
    }
}

pub(super) fn plan(
    resources: &[ResourceDefinition],
    limits: &Limits,
    work: &mut usize,
) -> CompileResult<BTreeMap<usize, Protocols>> {
    let mut plans = BTreeMap::new();
    for (index, resource) in resources.iter().enumerate() {
        let Type::OwnedStore { element } = &resource.payload else {
            continue;
        };
        if !inline_protocol_element(element) {
            continue;
        }
        // Reserve complete nominal metadata before cloning either protocol.
        // This also pays the bounded repeated machine-type/layout clones and
        // fixed helper events. Existing Package Stores never enter this path.
        let before = *work;
        copy_aggregates::reserve_enum_metadata(element, work, 0, limits)?;
        let shape_work = before - *work;
        let cost = shape_work.saturating_mul(64).saturating_add(2048);
        *work = work.checked_sub(cost).ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Codegen,
                "POD Store protocol planning budget exceeded",
                None,
            )]
        })?;
        let push = Type::ErrorUnion {
            error: element.clone(),
            result: Box::new(Type::Unit),
        };
        let pop = Type::ErrorUnion {
            error: Box::new(Type::Unit),
            result: element.clone(),
        };
        let push = copy_aggregates::classify(&push, work, limits)?;
        let pop = copy_aggregates::classify(&pop, work, limits)?;
        let (stride, _) = copy_aggregates::layout(&llvm_type(element)?)?;
        if stride == 0 || stride > i64::MAX as u64 {
            return invalid(
                "E-CODEGEN-CFG",
                "POD Store requires a nonzero bounded stride",
                None,
            );
        }
        plans.insert(
            index,
            Protocols {
                push,
                pop,
                stride,
                shape_work,
            },
        );
    }
    Ok(plans)
}

/// Pay the new POD operand/type comparisons and private helper emission
/// before walking or retaining per-instruction machine types. No legacy-only
/// module gets a second instruction scan or any changed Package charge.
pub(super) fn reserve_operations(
    function: &TypedFunction,
    resources: usize,
    plans: &BTreeMap<usize, Protocols>,
    work: &mut usize,
) -> CompileResult<()> {
    if plans.is_empty() {
        return Ok(());
    }
    let pay_scan = |work: &mut usize, cost: usize| -> CompileResult<()> {
        *work = work.checked_sub(cost).ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Codegen,
                "POD Store operation scan budget exceeded",
                Some(function.span),
            )]
        })?;
        Ok(())
    };
    // Prepay the complete plan scan and each block visit before examining
    // metadata, including scalar/legacy instructions that need no POD helper.
    pay_scan(
        work,
        plans
            .len()
            .saturating_add(function.blocks.len())
            .saturating_add(1),
    )?;
    let shape = plans.values().map(|p| p.shape_work).max().unwrap();
    for block in &function.blocks {
        pay_scan(work, block.instructions.len())?;
        for instruction in &block.instructions {
            let store = match &instruction.kind {
                InstructionKind::OwnedStoreReserve { store, .. }
                | InstructionKind::OwnedStorePush { store, .. }
                | InstructionKind::OwnedStorePop { store }
                | InstructionKind::OwnedStoreLen { store }
                | InstructionKind::OwnedStoreCapacity { store }
                | InstructionKind::OwnedStoreBorrow { store, .. }
                | InstructionKind::OwnedStoreBorrowMut { store, .. } => store,
                _ => continue,
            };
            if !matches!(&function.values[store.0 as usize], Type::Desc(payload)
            if matches!(payload.as_ref(), Type::OwnedStore { element }
                if inline_protocol_element(element)))
            {
                continue;
            }
            let cost = resources
                .saturating_add(1)
                .saturating_mul(shape)
                .saturating_mul(32)
                .saturating_add(128);
            *work = work.checked_sub(cost).ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Codegen,
                    "POD Store instruction emission budget exceeded",
                    Some(instruction.span),
                )]
            })?;
        }
    }
    Ok(())
}

pub(super) fn is_resource(ty: &Type, definitions: &[ResourceDefinition]) -> bool {
    matches!(ty, Type::Managed(payload) | Type::Desc(payload)
        if matches!(payload.as_ref(), Type::OwnedStore { .. })
        && definitions.iter().any(|r| r.payload == **payload && r.teardown.is_none()))
}

pub(super) fn helper(
    payload: &Type,
    definitions: &[ResourceDefinition],
    operation: &str,
) -> CompileResult<String> {
    let Some(index) = definitions
        .iter()
        .position(|r| r.payload == *payload && r.teardown.is_none())
    else {
        return invalid(
            "E-CODEGEN-CFG",
            "Store operation has no exact sealed resource definition",
            None,
        );
    };
    if !matches!(payload, Type::OwnedStore { .. }) {
        return invalid(
            "E-CODEGEN-CFG",
            "Store runtime requires its sealed payload",
            None,
        );
    }
    Ok(format!("zil.store.{index}.{operation}"))
}

pub(super) fn runtime(
    lir: &LirView<'_>,
    closures: &BTreeMap<ClosureId, &ClosureDefinition>,
    protocols: &BTreeMap<usize, Protocols>,
) -> CompileResult<m::Code> {
    if !lir
        .resources
        .iter()
        .any(|r| matches!(r.payload, Type::OwnedStore { .. }))
    {
        return Ok(m::Code::new());
    }
    if !lir.target.is_supported_x64_llvm() {
        return unsupported("owned Store requires checked Linux x64 or Win64 LLVM", None);
    }
    if let Some(f) = lir.functions.iter().find(|f| {
        f.foreign
            .as_ref()
            .is_some_and(|a| a.symbol.starts_with("zil.store."))
    }) {
        return invalid(
            "E-CODEGEN-FOREIGN",
            "foreign symbol conflicts with owned Store runtime ABI",
            Some(f.span),
        );
    }
    let mut output = m::Code::new();
    for (index, resource) in lir.resources.iter().enumerate() {
        let Type::OwnedStore { element } = &resource.payload else {
            continue;
        };
        let pod = protocols.get(&index);
        if !matches!(element.as_ref(), Type::Package { .. }) && pod.is_none() {
            return invalid(
                "E-CODEGEN-CFG",
                "Store runtime element lacks its verified representation plan",
                None,
            );
        };
        let item = llvm_type(element)?;
        let align = alignment(element, closures)?;
        let push = llvm_type(&Type::ErrorUnion {
            error: element.clone(),
            result: Box::new(Type::Unit),
        })?;
        let pop = llvm_type(&Type::ErrorUnion {
            error: Box::new(Type::Unit),
            result: element.clone(),
        })?;
        output.append(m::Code::from(vec![
            m::Event::Begin(m::Function {
                name: format!("zil.store.{}.reserve", index),
                linkage: m::Linkage::Internal,
                signature: m::Signature {
                    result: m::Ty::Int(1),
                    result_attrs: vec![],
                    params: vec![
                        m::Param {
                            ty: m::Ty::Ptr,
                            name: Some("head".into()),
                            attrs: vec![],
                        },
                        m::Param {
                            ty: m::Ty::Int(64),
                            name: Some("requested".into()),
                            attrs: vec![],
                        },
                    ],
                    variadic: false,
                },
                attrs: vec![],
                blocks: None,
            }),
            m::Event::Label("entry".into()),
            m::Event::Instruction(m::Inst::new(
                Some("stride".into()),
                m::Op::Cast {
                    opcode: "ptrtoint".into(),
                    arg: m::Arg::new(
                        m::Ty::Ptr,
                        m::Val::Gep {
                            ty: (item).clone(),
                            ptr: Box::new(m::Val::Null),
                            indices: vec![m::Arg::new(m::Ty::Int(64), m::Val::Int(1))],
                        },
                    ),
                    to: m::Ty::Int(64),
                },
            )),
            m::Event::Instruction(m::Inst::new(
                Some("maximum".into()),
                m::Op::Binary {
                    opcode: "udiv".into(),
                    ty: m::Ty::Int(64),
                    left: m::Val::Int(9223372036854775807),
                    right: m::local("stride".into()),
                },
            )),
            m::Event::Instruction(m::Inst::new(
                Some("positive".into()),
                m::Op::Compare {
                    predicate: "sge".into(),
                    ty: m::Ty::Int(64),
                    left: m::local("requested".into()),
                    right: m::Val::Int(0),
                },
            )),
            m::Event::Instruction(m::Inst::new(
                Some("bounded".into()),
                m::Op::Compare {
                    predicate: "ule".into(),
                    ty: m::Ty::Int(64),
                    left: m::local("requested".into()),
                    right: m::local("maximum".into()),
                },
            )),
            m::Event::Instruction(m::Inst::new(
                Some("valid".into()),
                m::Op::Binary {
                    opcode: "and".into(),
                    ty: m::Ty::Int(1),
                    left: m::local("positive".into()),
                    right: m::local("bounded".into()),
                },
            )),
            m::Event::Instruction(m::Inst::new(
                None,
                m::Op::CondBranch {
                    condition: m::local("valid".into()),
                    yes: "capacity".into(),
                    no: "failed".into(),
                },
            )),
            m::Event::Label("capacity".into()),
            m::Event::Instruction(m::Inst::new(
                Some("cap.field".into()),
                m::Op::Gep {
                    ty: m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64), m::Ty::Int(64)]),
                    ptr: m::local("head".into()),
                    indices: vec![
                        m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                        m::Arg::new(m::Ty::Int(32), m::Val::Int(2)),
                    ],
                },
            )),
            m::Event::Instruction(m::Inst::new(
                Some("cap".into()),
                m::Op::Load {
                    ty: m::Ty::Int(64),
                    ptr: m::local("cap.field".into()),
                    align: 8,
                },
            )),
            m::Event::Instruction(m::Inst::new(
                Some("fits".into()),
                m::Op::Compare {
                    predicate: "ule".into(),
                    ty: m::Ty::Int(64),
                    left: m::local("requested".into()),
                    right: m::local("cap".into()),
                },
            )),
            m::Event::Instruction(m::Inst::new(
                None,
                m::Op::CondBranch {
                    condition: m::local("fits".into()),
                    yes: "done".into(),
                    no: "prepare".into(),
                },
            )),
            m::Event::Label("prepare".into()),
            m::Event::Instruction(m::Inst::new(
                Some("ptr.field".into()),
                m::Op::Gep {
                    ty: m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64), m::Ty::Int(64)]),
                    ptr: m::local("head".into()),
                    indices: vec![
                        m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                        m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                    ],
                },
            )),
            m::Event::Instruction(m::Inst::new(
                Some("old".into()),
                m::Op::Load {
                    ty: m::Ty::Ptr,
                    ptr: m::local("ptr.field".into()),
                    align: 8,
                },
            )),
            m::Event::Instruction(m::Inst::new(
                Some("size".into()),
                m::Op::Binary {
                    opcode: "mul".into(),
                    ty: m::Ty::Int(64),
                    left: m::local("requested".into()),
                    right: m::local("stride".into()),
                },
            )),
            m::Event::Instruction(m::Inst::new(
                Some("next".into()),
                m::Op::Call {
                    result: m::Ty::Ptr,
                    result_attrs: vec![],
                    callee: m::global("zil.bytes.reallocate".into()),
                    args: vec![
                        m::Arg::new(m::Ty::Ptr, m::local("old".into())),
                        m::Arg::new(m::Ty::Int(64), m::local("size".into())),
                    ],
                    vararg: None,
                },
            )),
            m::Event::Instruction(m::Inst::new(
                Some("ok".into()),
                m::Op::Compare {
                    predicate: "ne".into(),
                    ty: m::Ty::Ptr,
                    left: m::local("next".into()),
                    right: m::Val::Null,
                },
            )),
            m::Event::Instruction(m::Inst::new(
                None,
                m::Op::CondBranch {
                    condition: m::local("ok".into()),
                    yes: "publish".into(),
                    no: "failed".into(),
                },
            )),
            m::Event::Label("publish".into()),
            m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Store {
                    arg: m::Arg::new(m::Ty::Ptr, m::local("next".into())),
                    ptr: m::local("ptr.field".into()),
                    align: 8,
                },
            )),
            m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Store {
                    arg: m::Arg::new(m::Ty::Int(64), m::local("requested".into())),
                    ptr: m::local("cap.field".into()),
                    align: 8,
                },
            )),
            m::Event::Instruction(m::Inst::new(None, m::Op::Branch("done".into()))),
            m::Event::Label("done".into()),
            m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Return(Some(m::Arg::new(m::Ty::Int(1), m::Val::Bool(true)))),
            )),
            m::Event::Label("failed".into()),
            m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Return(Some(m::Arg::new(m::Ty::Int(1), m::Val::Bool(false)))),
            )),
            m::Event::End,
        ]));
        if let Some(plan) = pod.and_then(|p| p.push.as_ref()) {
            output.append(pod_protocol(index, &item, plan, pod.unwrap().stride, true));
        } else {
            output.append(m::Code::from(vec![
                m::Event::Begin(m::Function {
                    name: format!("zil.store.{}.push", index),
                    linkage: m::Linkage::Internal,
                    signature: m::Signature {
                        result: (push).clone(),
                        result_attrs: vec![],
                        params: vec![
                            m::Param {
                                ty: m::Ty::Ptr,
                                name: Some("head".into()),
                                attrs: vec![],
                            },
                            m::Param {
                                ty: (item).clone(),
                                name: Some("item".into()),
                                attrs: vec![],
                            },
                        ],
                        variadic: false,
                    },
                    attrs: vec![],
                    blocks: None,
                }),
                m::Event::Label("entry".into()),
                m::Event::Instruction(m::Inst::new(
                    Some("len.field".into()),
                    m::Op::Gep {
                        ty: m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64), m::Ty::Int(64)]),
                        ptr: m::local("head".into()),
                        indices: vec![
                            m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                            m::Arg::new(m::Ty::Int(32), m::Val::Int(1)),
                        ],
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    Some("cap.field".into()),
                    m::Op::Gep {
                        ty: m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64), m::Ty::Int(64)]),
                        ptr: m::local("head".into()),
                        indices: vec![
                            m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                            m::Arg::new(m::Ty::Int(32), m::Val::Int(2)),
                        ],
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    Some("len".into()),
                    m::Op::Load {
                        ty: m::Ty::Int(64),
                        ptr: m::local("len.field".into()),
                        align: 8,
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    Some("cap".into()),
                    m::Op::Load {
                        ty: m::Ty::Int(64),
                        ptr: m::local("cap.field".into()),
                        align: 8,
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    Some("room".into()),
                    m::Op::Compare {
                        predicate: "ult".into(),
                        ty: m::Ty::Int(64),
                        left: m::local("len".into()),
                        right: m::local("cap".into()),
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::CondBranch {
                        condition: m::local("room".into()),
                        yes: "initialize".into(),
                        no: "full".into(),
                    },
                )),
                m::Event::Label("initialize".into()),
                m::Event::Instruction(m::Inst::new(
                    Some("ptr.field".into()),
                    m::Op::Gep {
                        ty: m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64), m::Ty::Int(64)]),
                        ptr: m::local("head".into()),
                        indices: vec![
                            m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                            m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                        ],
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    Some("data".into()),
                    m::Op::Load {
                        ty: m::Ty::Ptr,
                        ptr: m::local("ptr.field".into()),
                        align: 8,
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    Some("slot".into()),
                    m::Op::Gep {
                        ty: (item).clone(),
                        ptr: m::local("data".into()),
                        indices: vec![m::Arg::new(m::Ty::Int(64), m::local("len".into()))],
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Store {
                        arg: m::Arg::new((item).clone(), m::local("item".into())),
                        ptr: m::local("slot".into()),
                        align: (align) as u32,
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    Some("next.len".into()),
                    m::Op::Binary {
                        opcode: "add".into(),
                        ty: m::Ty::Int(64),
                        left: m::local("len".into()),
                        right: m::Val::Int(1),
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Store {
                        arg: m::Arg::new(m::Ty::Int(64), m::local("next.len".into())),
                        ptr: m::local("len.field".into()),
                        align: 8,
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Return(Some(m::Arg::new((push).clone(), m::Val::Zero))),
                )),
                m::Event::Label("full".into()),
                m::Event::Instruction(m::Inst::new(
                    Some("tag".into()),
                    m::Op::Insert {
                        arg: m::Arg::new((push).clone(), m::Val::Zero),
                        index: 0,
                        field: m::Arg::new(m::Ty::Int(1), m::Val::Bool(true)),
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    Some("error".into()),
                    m::Op::Insert {
                        arg: m::Arg::new((push).clone(), m::local("tag".into())),
                        index: 1,
                        field: m::Arg::new((item).clone(), m::local("item".into())),
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Return(Some(m::Arg::new((push).clone(), m::local("error".into())))),
                )),
                m::Event::End,
            ]));
        }
        if let Some(plan) = pod.and_then(|p| p.pop.as_ref()) {
            output.append(pod_protocol(index, &item, plan, pod.unwrap().stride, false));
        } else {
            output.append(m::Code::from(vec![
                m::Event::Begin(m::Function {
                    name: format!("zil.store.{}.pop", index),
                    linkage: m::Linkage::Internal,
                    signature: m::Signature {
                        result: (pop).clone(),
                        result_attrs: vec![],
                        params: vec![m::Param {
                            ty: m::Ty::Ptr,
                            name: Some("head".into()),
                            attrs: vec![],
                        }],
                        variadic: false,
                    },
                    attrs: vec![],
                    blocks: None,
                }),
                m::Event::Label("entry".into()),
                m::Event::Instruction(m::Inst::new(
                    Some("len.field".into()),
                    m::Op::Gep {
                        ty: m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64), m::Ty::Int(64)]),
                        ptr: m::local("head".into()),
                        indices: vec![
                            m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                            m::Arg::new(m::Ty::Int(32), m::Val::Int(1)),
                        ],
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    Some("len".into()),
                    m::Op::Load {
                        ty: m::Ty::Int(64),
                        ptr: m::local("len.field".into()),
                        align: 8,
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    Some("live".into()),
                    m::Op::Compare {
                        predicate: "ne".into(),
                        ty: m::Ty::Int(64),
                        left: m::local("len".into()),
                        right: m::Val::Int(0),
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::CondBranch {
                        condition: m::local("live".into()),
                        yes: "take".into(),
                        no: "empty".into(),
                    },
                )),
                m::Event::Label("take".into()),
                m::Event::Instruction(m::Inst::new(
                    Some("next.len".into()),
                    m::Op::Binary {
                        opcode: "sub".into(),
                        ty: m::Ty::Int(64),
                        left: m::local("len".into()),
                        right: m::Val::Int(1),
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Store {
                        arg: m::Arg::new(m::Ty::Int(64), m::local("next.len".into())),
                        ptr: m::local("len.field".into()),
                        align: 8,
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    Some("ptr.field".into()),
                    m::Op::Gep {
                        ty: m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64), m::Ty::Int(64)]),
                        ptr: m::local("head".into()),
                        indices: vec![
                            m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                            m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                        ],
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    Some("data".into()),
                    m::Op::Load {
                        ty: m::Ty::Ptr,
                        ptr: m::local("ptr.field".into()),
                        align: 8,
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    Some("slot".into()),
                    m::Op::Gep {
                        ty: (item).clone(),
                        ptr: m::local("data".into()),
                        indices: vec![m::Arg::new(m::Ty::Int(64), m::local("next.len".into()))],
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    Some("item".into()),
                    m::Op::Load {
                        ty: (item).clone(),
                        ptr: m::local("slot".into()),
                        align: (align) as u32,
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    Some("result".into()),
                    m::Op::Insert {
                        arg: m::Arg::new((pop).clone(), m::Val::Zero),
                        index: 2,
                        field: m::Arg::new((item).clone(), m::local("item".into())),
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Return(Some(m::Arg::new((pop).clone(), m::local("result".into())))),
                )),
                m::Event::Label("empty".into()),
                m::Event::Instruction(m::Inst::new(
                    Some("error".into()),
                    m::Op::Insert {
                        arg: m::Arg::new((pop).clone(), m::Val::Zero),
                        index: 0,
                        field: m::Arg::new(m::Ty::Int(1), m::Val::Bool(true)),
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Return(Some(m::Arg::new((pop).clone(), m::local("error".into())))),
                )),
                m::Event::End,
            ]));
        }
        if pod.is_some() {
            output.append(pod_drop(index));
            continue;
        }
        output.append(m::Code::from(vec![
            m::Event::Begin(m::Function {
                name: format!("zil.store.{}.drop", index),
                linkage: m::Linkage::Internal,
                signature: m::Signature {
                    result: m::Ty::Void,
                    result_attrs: vec![],
                    params: vec![m::Param {
                        ty: m::Ty::Ptr,
                        name: Some("head".into()),
                        attrs: vec![],
                    }],
                    variadic: false,
                },
                attrs: vec![],
                blocks: None,
            }),
            m::Event::Label("entry".into()),
            m::Event::Instruction(m::Inst::new(
                Some("len.field".into()),
                m::Op::Gep {
                    ty: m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64), m::Ty::Int(64)]),
                    ptr: m::local("head".into()),
                    indices: vec![
                        m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                        m::Arg::new(m::Ty::Int(32), m::Val::Int(1)),
                    ],
                },
            )),
            m::Event::Instruction(m::Inst::new(
                Some("ptr.field".into()),
                m::Op::Gep {
                    ty: m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64), m::Ty::Int(64)]),
                    ptr: m::local("head".into()),
                    indices: vec![
                        m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                        m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                    ],
                },
            )),
            m::Event::Instruction(m::Inst::new(
                Some("len".into()),
                m::Op::Load {
                    ty: m::Ty::Int(64),
                    ptr: m::local("len.field".into()),
                    align: 8,
                },
            )),
            m::Event::Instruction(m::Inst::new(
                Some("data".into()),
                m::Op::Load {
                    ty: m::Ty::Ptr,
                    ptr: m::local("ptr.field".into()),
                    align: 8,
                },
            )),
            m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Store {
                    arg: m::Arg::new(m::Ty::Int(64), m::Val::Int(0)),
                    ptr: m::local("len.field".into()),
                    align: 8,
                },
            )),
            m::Event::Instruction(m::Inst::new(None, m::Op::Branch("loop".into()))),
            m::Event::Label("loop".into()),
            m::Event::Instruction(m::Inst::new(
                Some("index".into()),
                m::Op::Phi {
                    ty: m::Ty::Int(64),
                    incoming: vec![
                        (m::Val::Int(0), "entry".into()),
                        (m::local("next".into()), "element".into()),
                    ],
                },
            )),
            m::Event::Instruction(m::Inst::new(
                Some("remaining".into()),
                m::Op::Compare {
                    predicate: "ult".into(),
                    ty: m::Ty::Int(64),
                    left: m::local("index".into()),
                    right: m::local("len".into()),
                },
            )),
            m::Event::Instruction(m::Inst::new(
                None,
                m::Op::CondBranch {
                    condition: m::local("remaining".into()),
                    yes: "element".into(),
                    no: "release".into(),
                },
            )),
            m::Event::Label("element".into()),
            m::Event::Instruction(m::Inst::new(
                Some("slot".into()),
                m::Op::Gep {
                    ty: (item).clone(),
                    ptr: m::local("data".into()),
                    indices: vec![m::Arg::new(m::Ty::Int(64), m::local("index".into()))],
                },
            )),
        ]));
        // Whole Store elements are complete inline values. The same checked
        // declaration-order leaf paths used by ordinary Product cleanup also
        // reach every nested child here; no extra partial flags or allocation.
        for (path, teardown) in packages::teardown_paths(element, lir.resources)? {
            let name = format!(
                "field.{}",
                path.iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(".")
            );
            let mut indices = vec![m::Arg::new(m::Ty::Int(32), m::Val::Int(0))];
            indices.extend(path.iter().map(|index| m::Arg::new(m::Ty::Int(32), *index)));
            output.append(m::Code::from(vec![
                m::Event::Instruction(m::Inst::new(
                    Some(name.clone()),
                    m::Op::Gep {
                        ty: item.clone(),
                        ptr: m::local("slot".into()),
                        indices,
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Call {
                        result: m::Ty::Void,
                        result_attrs: vec![],
                        callee: m::global(teardown),
                        args: vec![m::Arg::new(m::Ty::Ptr, m::local(name))],
                        vararg: None,
                    },
                )),
            ]));
        }
        output.append(m::Code::from(vec![
            m::Event::Instruction(m::Inst::new(
                Some("next".into()),
                m::Op::Binary {
                    opcode: "add".into(),
                    ty: m::Ty::Int(64),
                    left: m::local("index".into()),
                    right: m::Val::Int(1),
                },
            )),
            m::Event::Instruction(m::Inst::new(None, m::Op::Branch("loop".into()))),
            m::Event::Label("release".into()),
            m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Call {
                    result: m::Ty::Void,
                    result_attrs: vec![],
                    callee: m::global("zil.bytes.release".into()),
                    args: vec![m::Arg::new(m::Ty::Ptr, m::local("data".into()))],
                    vararg: None,
                },
            )),
            m::Event::Instruction(m::Inst::new(None, m::Op::Return(None))),
            m::Event::End,
        ]));
    }
    Ok(output)
}

// These helpers only transfer between the initialized Store prefix and a
// private result snapshot. No pointer becomes a source value or escapes a call.
fn pod_inst(code: &mut m::Code, name: Option<&str>, op: m::Op) {
    code.push(m::Event::Instruction(m::Inst::new(
        name.map(str::to_owned),
        op,
    )));
}

fn pod_field(code: &mut m::Code, name: &str, ty: &m::Ty, base: &str, index: i128) {
    pod_inst(
        code,
        Some(name),
        m::Op::Gep {
            ty: ty.clone(),
            ptr: m::local(base.into()),
            indices: vec![
                m::Arg::new(m::Ty::Int(32), 0),
                m::Arg::new(m::Ty::Int(32), index),
            ],
        },
    );
}

fn pod_load(code: &mut m::Code, name: &str, ty: m::Ty, ptr: &str) {
    pod_inst(
        code,
        Some(name),
        m::Op::Load {
            ty,
            ptr: m::local(ptr.into()),
            align: 8,
        },
    );
}

fn pod_bytes(code: &mut m::Code, destination: &str, source: Option<&str>, bytes: u64) {
    let (callee, second) = match source {
        Some(source) => (
            "zil.copy.bytes",
            m::Arg::new(m::Ty::Ptr, m::local(source.into())),
        ),
        None => ("zil.zero.bytes", m::Arg::new(m::Ty::Int(8), 0)),
    };
    pod_inst(
        code,
        None,
        m::Op::Call {
            result: m::Ty::Void,
            result_attrs: vec![],
            callee: m::global(callee.into()),
            args: vec![
                m::Arg::new(m::Ty::Ptr, m::local(destination.into())),
                second,
                m::Arg::new(m::Ty::Int(64), m::Val::Int(i128::from(bytes))),
            ],
            vararg: None,
        },
    );
}

fn pod_protocol(
    index: usize,
    item: &m::Ty,
    plan: &copy_aggregates::Slot,
    stride: u64,
    push: bool,
) -> m::Code {
    let operation = if push { "push" } else { "pop" };
    let head = m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64), m::Ty::Int(64)]);
    let mut code = m::Code::from(vec![
        m::Event::Begin(m::Function {
            name: format!("zil.store.{index}.{operation}.memory"),
            linkage: m::Linkage::Internal,
            signature: m::Signature {
                result: m::Ty::Void,
                result_attrs: vec![],
                params: vec![
                    m::Param::new(m::Ty::Ptr, "out"),
                    m::Param::new(m::Ty::Ptr, "head"),
                ],
                variadic: false,
            },
            attrs: vec![m::Attr::NoInline],
            blocks: None,
        }),
        m::Event::Label("entry".into()),
    ]);
    if !push {
        pod_bytes(&mut code, "out", None, plan.bytes);
    }
    pod_field(&mut code, "len.field", &head, "head", 1);
    pod_load(&mut code, "len", m::Ty::Int(64), "len.field");
    if push {
        pod_field(&mut code, "cap.field", &head, "head", 2);
        pod_load(&mut code, "cap", m::Ty::Int(64), "cap.field");
    }
    pod_inst(
        &mut code,
        Some("available"),
        m::Op::Compare {
            predicate: if push { "ult" } else { "ne" }.into(),
            ty: m::Ty::Int(64),
            left: m::local("len".into()),
            right: if push {
                m::local("cap".into())
            } else {
                m::Val::Int(0)
            },
        },
    );
    pod_inst(
        &mut code,
        None,
        m::Op::CondBranch {
            condition: m::local("available".into()),
            yes: "transfer".into(),
            no: "failed".into(),
        },
    );
    code.push(m::Event::Label("transfer".into()));
    pod_inst(
        &mut code,
        Some("next.len"),
        m::Op::Binary {
            opcode: if push { "add" } else { "sub" }.into(),
            ty: m::Ty::Int(64),
            left: m::local("len".into()),
            right: m::Val::Int(1),
        },
    );
    if !push {
        // Retire the tail before exposing its complete Copy value.
        pod_inst(
            &mut code,
            None,
            m::Op::Store {
                arg: m::Arg::new(m::Ty::Int(64), m::local("next.len".into())),
                ptr: m::local("len.field".into()),
                align: 8,
            },
        );
    }
    pod_field(&mut code, "ptr.field", &head, "head", 0);
    pod_load(&mut code, "data", m::Ty::Ptr, "ptr.field");
    pod_inst(
        &mut code,
        Some("slot"),
        m::Op::Gep {
            ty: item.clone(),
            ptr: m::local("data".into()),
            indices: vec![m::Arg::new(
                m::Ty::Int(64),
                m::local(if push { "len" } else { "next.len" }.into()),
            )],
        },
    );
    pod_field(
        &mut code,
        "payload",
        &plan.ty,
        "out",
        if push { 1 } else { 2 },
    );
    if push {
        pod_bytes(&mut code, "slot", Some("payload"), stride);
        pod_inst(
            &mut code,
            None,
            m::Op::Store {
                arg: m::Arg::new(m::Ty::Int(64), m::local("next.len".into())),
                ptr: m::local("len.field".into()),
                align: 8,
            },
        );
        pod_bytes(&mut code, "out", None, plan.bytes);
    } else {
        pod_bytes(&mut code, "payload", Some("slot"), stride);
    }
    pod_inst(&mut code, None, m::Op::Return(None));
    code.push(m::Event::Label("failed".into()));
    pod_field(&mut code, "tag", &plan.ty, "out", 0);
    pod_inst(
        &mut code,
        None,
        m::Op::Store {
            arg: m::Arg::new(m::Ty::Int(1), m::Val::Bool(true)),
            ptr: m::local("tag".into()),
            align: 1,
        },
    );
    pod_inst(&mut code, None, m::Op::Return(None));
    code.push(m::Event::End);
    code
}

fn pod_drop(index: usize) -> m::Code {
    let head = m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64), m::Ty::Int(64)]);
    let mut code = m::Code::from(vec![
        m::Event::Begin(m::Function {
            name: format!("zil.store.{index}.drop"),
            linkage: m::Linkage::Internal,
            signature: m::Signature {
                result: m::Ty::Void,
                result_attrs: vec![],
                params: vec![m::Param::new(m::Ty::Ptr, "head")],
                variadic: false,
            },
            attrs: vec![],
            blocks: None,
        }),
        m::Event::Label("entry".into()),
    ]);
    pod_field(&mut code, "ptr.field", &head, "head", 0);
    pod_load(&mut code, "data", m::Ty::Ptr, "ptr.field");
    pod_inst(
        &mut code,
        None,
        m::Op::Store {
            arg: m::Arg::new(head, m::Val::Zero),
            ptr: m::local("head".into()),
            align: 8,
        },
    );
    pod_inst(
        &mut code,
        None,
        m::Op::Call {
            result: m::Ty::Void,
            result_attrs: vec![],
            callee: m::global("zil.bytes.release".into()),
            args: vec![m::Arg::new(m::Ty::Ptr, m::local("data".into()))],
            vararg: None,
        },
    );
    pod_inst(&mut code, None, m::Op::Return(None));
    code.push(m::Event::End);
    code
}

impl FunctionEmitter<'_> {
    fn pod_protocol_call(
        &mut self,
        output: ValueId,
        store: ValueId,
        operation: &str,
    ) -> CompileResult<()> {
        let Type::Desc(payload) = self.value_type(store)? else {
            return invalid(
                "E-CODEGEN-CFG",
                "Store snapshot requires a real description",
                None,
            );
        };
        let index = self
            .resources
            .iter()
            .position(|r| r.payload == **payload && r.teardown.is_none());
        let plan = index.and_then(|i| self.abi_plans.store_protocols.get(&i));
        let planned = plan.is_some_and(|p| {
            if operation == "push" {
                p.push.is_some()
            } else {
                p.pop.is_some()
            }
        });
        if !planned {
            return invalid(
                "E-CODEGEN-CFG",
                "Store snapshot requires its prepaid POD protocol",
                None,
            );
        }
        let callee = helper(payload, self.resources, &format!("{operation}.memory"))?;
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Call {
                result: m::Ty::Void,
                result_attrs: vec![],
                callee: m::global(callee),
                args: vec![
                    m::Arg::new(m::Ty::Ptr, copy_aggregates::pointer(output)),
                    m::Arg::new(m::Ty::Ptr, m::val(value(store))),
                ],
                vararg: None,
            },
        )));
        Ok(())
    }
    pub(super) fn dynamic_store_instruction(
        &mut self,
        instruction: &Instruction,
    ) -> CompileResult<()> {
        let output = result(instruction)?;
        if matches!(instruction.kind, InstructionKind::OwnedStoreNew { .. }) {
            self.event(m::Event::Instruction(m::Inst::new(
                Some(m::name((value(output)).clone())),
                m::Op::Freeze(m::Arg::new(
                    m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64), m::Ty::Int(64)]),
                    m::Val::Zero,
                )),
            )));
            return Ok(());
        }
        let store = match instruction.kind {
            InstructionKind::OwnedStoreReserve { store, .. }
            | InstructionKind::OwnedStorePush { store, .. }
            | InstructionKind::OwnedStorePop { store }
            | InstructionKind::OwnedStoreLen { store }
            | InstructionKind::OwnedStoreCapacity { store }
            | InstructionKind::OwnedStoreBorrow { store, .. }
            | InstructionKind::OwnedStoreBorrowMut { store, .. } => store,
            _ => unreachable!("Store instruction dispatch"),
        };
        let Type::Desc(payload) = self.value_type(store)? else {
            return invalid(
                "E-CODEGEN-CFG",
                "Store operation requires a description",
                Some(instruction.span),
            );
        };
        match instruction.kind {
            InstructionKind::OwnedStoreReserve { capacity, .. } => {
                let callee = helper(payload, self.resources, "reserve")?;
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((value(output)).clone())),
                    m::Op::Call {
                        result: m::Ty::Int(1),
                        result_attrs: vec![],
                        callee: m::global(callee.to_string()),
                        args: vec![
                            m::Arg::new(m::Ty::Ptr, m::val((value(store)).clone())),
                            m::Arg::new(m::Ty::Int(64), m::val((value(capacity)).clone())),
                        ],
                        vararg: None,
                    },
                )));
            }
            InstructionKind::OwnedStoreBorrow { index, .. }
            | InstructionKind::OwnedStoreBorrowMut { index, .. } => {
                let Type::OwnedStore { element } = payload.as_ref() else {
                    unreachable!("verified Store payload");
                };
                let element_type = llvm_type(element)?;
                let length_ptr = self.temp();
                let length = self.temp();
                let valid = self.temp();
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((length_ptr).clone())),
                    m::Op::Gep {
                        ty: m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64), m::Ty::Int(64)]),
                        ptr: m::val((value(store)).clone()),
                        indices: vec![
                            m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                            m::Arg::new(m::Ty::Int(32), m::Val::Int(1)),
                        ],
                    },
                )));
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((length).clone())),
                    m::Op::Load {
                        ty: m::Ty::Int(64),
                        ptr: m::val((length_ptr).clone()),
                        align: 8,
                    },
                )));
                // Unsigned comparison also rejects negative i64 indices.
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((valid).clone())),
                    m::Op::Compare {
                        predicate: "ult".into(),
                        ty: m::Ty::Int(64),
                        left: m::val((value(index)).clone()),
                        right: m::val((length).clone()),
                    },
                )));
                let continuation = self.continuation;
                self.continuation += 1;
                self.event(m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::CondBranch {
                        condition: m::val((valid).clone()),
                        yes: format!("store.element.ok.{}", continuation),
                        no: "zil.trap".into(),
                    },
                )));
                self.event(m::Event::Label(format!(
                    "store.element.ok.{}",
                    continuation
                )));
                let data_ptr = self.temp();
                let data = self.temp();
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((data_ptr).clone())),
                    m::Op::Gep {
                        ty: m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64), m::Ty::Int(64)]),
                        ptr: m::val((value(store)).clone()),
                        indices: vec![
                            m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                            m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                        ],
                    },
                )));
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((data).clone())),
                    m::Op::Load {
                        ty: m::Ty::Ptr,
                        ptr: m::val((data_ptr).clone()),
                        align: 8,
                    },
                )));
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((value(output)).clone())),
                    m::Op::Gep {
                        ty: (element_type).clone(),
                        ptr: m::val((data).clone()),
                        indices: vec![m::Arg::new(m::Ty::Int(64), m::val((value(index)).clone()))],
                    },
                )));
            }
            InstructionKind::OwnedStorePush { value: item, .. } => {
                if self.copy_values.contains_key(&output) {
                    self.copy_zero(output);
                    let destination = self.copy_field(output, m::Arg::new(m::Ty::Int(32), 1));
                    self.copy_write(destination, item)?;
                    self.pod_protocol_call(output, store, "push")?;
                    return Ok(());
                }
                let callee = helper(payload, self.resources, "push")?;
                let ty = llvm_type(self.value_type(item)?)?;
                let result = llvm_type(self.value_type(output)?)?;
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((value(output)).clone())),
                    m::Op::Call {
                        result: (result).clone(),
                        result_attrs: vec![],
                        callee: m::global(callee.to_string()),
                        args: vec![
                            m::Arg::new(m::Ty::Ptr, m::val((value(store)).clone())),
                            m::Arg::new((ty).clone(), m::val((value(item)).clone())),
                        ],
                        vararg: None,
                    },
                )));
            }
            InstructionKind::OwnedStorePop { .. } => {
                if self.copy_values.contains_key(&output) {
                    self.pod_protocol_call(output, store, "pop")?;
                    return Ok(());
                }
                let callee = helper(payload, self.resources, "pop")?;
                let result = llvm_type(self.value_type(output)?)?;
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((value(output)).clone())),
                    m::Op::Call {
                        result: (result).clone(),
                        result_attrs: vec![],
                        callee: m::global(callee.to_string()),
                        args: vec![m::Arg::new(m::Ty::Ptr, m::val((value(store)).clone()))],
                        vararg: None,
                    },
                )));
            }
            InstructionKind::OwnedStoreLen { .. } | InstructionKind::OwnedStoreCapacity { .. } => {
                let index = if matches!(instruction.kind, InstructionKind::OwnedStoreLen { .. }) {
                    1
                } else {
                    2
                };
                let pointer = self.temp();
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((pointer).clone())),
                    m::Op::Gep {
                        ty: m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64), m::Ty::Int(64)]),
                        ptr: m::val((value(store)).clone()),
                        indices: vec![
                            m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                            m::Arg::new(m::Ty::Int(32), m::val(index)),
                        ],
                    },
                )));
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((value(output)).clone())),
                    m::Op::Load {
                        ty: m::Ty::Int(64),
                        ptr: m::val((pointer).clone()),
                        align: 8,
                    },
                )));
            }
            _ => unreachable!("checked Store instruction"),
        }
        Ok(())
    }
}

#[cfg(test)]
mod pod_tests {
    use super::*;
    use zil_base::{Fingerprint, ModuleId, ResultCategory};

    fn resource(words: u32) -> ResourceDefinition {
        let element = Type::Record {
            name: format!("Words{words}"),
            fields: vec![(
                "data".into(),
                Type::Array {
                    element: Box::new(Type::U64),
                    length: words,
                },
            )],
        };
        ResourceDefinition {
            name: format!("@builtin:OwnedStore<{}>", element.name()),
            payload: Type::OwnedStore {
                element: Box::new(element),
            },
            teardown: None,
        }
    }

    fn view<'a>(resources: &'a [ResourceDefinition], target: &'a TargetProfile) -> LirView<'a> {
        LirView {
            schema_version: SCHEMA_VERSION,
            module_id: ModuleId(0),
            verified_fingerprint: Fingerprint::ZERO,
            target,
            functions: &[],
            closures: &[],
            resources,
            layouts: std::borrow::Cow::Borrowed(&[]),
        }
    }

    #[test]
    fn full_protocols_cross_snapshot_threshold_independently() {
        // Private physical planner probe. The real Source/raw/Wire admission
        // and native consumers belong to the root's independent machine matrix.
        for (words, expected_push, expected_pop) in [
            (509, None, None),
            (510, None, None),
            (511, Some(4096), Some(4096)),
            (512, Some(4104), Some(4104)),
            (32768, Some(262152), Some(262152)),
        ] {
            let resources = [resource(words)];
            let mut remaining = usize::MAX;
            let plans = plan(&resources, &Limits::default(), &mut remaining).unwrap();
            let p = &plans[&0];
            assert_eq!(p.stride, u64::from(words) * 8);
            assert_eq!(p.push.as_ref().map(|s| s.bytes), expected_push, "{words}");
            assert_eq!(p.pop.as_ref().map(|s| s.bytes), expected_pop, "{words}");
        }
    }

    #[test]
    fn scalar_protocols_have_exact_nonzero_stride() {
        for (element, stride) in [
            (Type::Bool, 1),
            (Type::U8, 1),
            (Type::U16, 2),
            (Type::I32, 4),
            (Type::I64, 8),
            (Type::U64, 8),
            (Type::Usize, 8),
            (Type::F32, 4),
            (Type::F64, 8),
        ] {
            let resources = [ResourceDefinition {
                name: format!("@builtin:OwnedStore<{}>", element.name()),
                payload: Type::OwnedStore {
                    element: Box::new(element),
                },
                teardown: None,
            }];
            let mut remaining = usize::MAX;
            let plans = plan(&resources, &Limits::default(), &mut remaining).unwrap();
            assert_eq!(plans[&0].stride, stride);
            assert!(plans[&0].push.is_none());
            assert!(plans[&0].pop.is_none());
        }
    }

    #[test]
    fn composite_protocols_use_actual_recursive_stride_and_exact_budget() {
        for (element, stride) in [
            (Type::Tuple(vec![Type::U8, Type::I64]), 16),
            (
                Type::Array {
                    element: Box::new(Type::I32),
                    length: 3,
                },
                12,
            ),
        ] {
            let resources = [ResourceDefinition {
                name: "checked composite Store".into(),
                payload: Type::OwnedStore {
                    element: Box::new(element),
                },
                teardown: None,
            }];
            let limits = Limits::default();
            let mut remaining = limits.max_analysis_steps;
            let planned = plan(&resources, &limits, &mut remaining).unwrap();
            assert_eq!(planned[&0].stride, stride);
            let cost = limits.max_analysis_steps - remaining;
            for allowance in [cost, cost - 1, cost] {
                let mut left = allowance;
                let result = plan(&resources, &limits, &mut left);
                assert_eq!(result.is_ok(), allowance == cost);
                if let Err(errors) = result {
                    assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
                }
            }
        }
    }

    #[test]
    fn protocol_plan_has_exact_budget_and_retains_package_cost() {
        let resources = [resource(510), resource(32768)];
        let limits = Limits::default();
        let mut remaining = usize::MAX;
        let planned = plan(&resources, &limits, &mut remaining).unwrap();
        let cost = usize::MAX - remaining;
        for _ in 0..2 {
            let mut exact = cost;
            let fresh = plan(&resources, &limits, &mut exact).unwrap();
            assert_eq!(exact, 0);
            for i in 0..2 {
                assert_eq!(fresh[&i].stride, planned[&i].stride);
                assert_eq!(
                    fresh[&i].push.as_ref().map(|s| s.bytes),
                    planned[&i].push.as_ref().map(|s| s.bytes)
                );
            }
        }
        let errors = plan(&resources, &limits, &mut (cost - 1)).err().unwrap();
        assert_eq!(errors[0].category, ResultCategory::ResourceLimit);
        let old = ResourceDefinition {
            name: "old".into(),
            payload: Type::OwnedStore {
                element: Box::new(Type::Package {
                    name: "Old".into(),
                    fields: vec![("owned".into(), Type::Managed(Box::new(Type::ByteBuffer)))],
                }),
            },
            teardown: None,
        };
        let mut zero = 0;
        assert!(plan(&[old], &limits, &mut zero).unwrap().is_empty());
        assert_eq!(zero, 0);
    }

    #[test]
    fn unused_pod_still_prepays_long_scalar_scans() {
        use zil_ir::{BasicBlock, BlockId, CallKind, PassingMode};
        let span = Span::new(zil_base::FileId(0), 0, 1);
        let scalar = |count: usize| TypedFunction {
            storage_life: None,
            call_kind: CallKind::Direct,
            resume_bound: zil_ir::TaskResumeBound::Local,
            reference_return_bound: None,
            callable_return_contract: None,
            callable_contract: None,
            protocol_contract: None,
            id: FunctionId(0),
            name: "scalar_control".into(),
            foreign: None,
            environment: None,
            parameters: vec![],
            return_type: Type::Unit,
            result_mode: PassingMode::Infer,
            locals: vec![],
            values: vec![Type::Bool; count],
            entry: BlockId(0),
            span,
            blocks: vec![BasicBlock {
                id: BlockId(0),
                terminator: Terminator::Return(None),
                instructions: (0..count)
                    .map(|i| Instruction {
                        result: Some(ValueId(i as u32)),
                        kind: InstructionKind::Constant(Constant::Bool(false)),
                        span,
                    })
                    .collect(),
            }],
        };
        let mut available = usize::MAX;
        let plans = plan(&[resource(1)], &Limits::default(), &mut available).unwrap();
        let mut previous = 0;
        for count in [0, 1, 10000] {
            let function = scalar(count);
            let mut zero = 0;
            assert_eq!(
                reserve_operations(&function, 1, &plans, &mut zero).unwrap_err()[0].category,
                ResultCategory::ResourceLimit
            );
            let mut available = usize::MAX;
            reserve_operations(&function, 1, &plans, &mut available).unwrap();
            let cost = usize::MAX - available;
            assert_eq!(cost, count + 3);
            assert!(cost > previous);
            previous = cost;
            for _ in 0..2 {
                let mut exact = cost;
                reserve_operations(&function, 1, &plans, &mut exact).unwrap();
                assert_eq!(exact, 0);
            }
            let mut short = cost - 1;
            assert_eq!(
                reserve_operations(&function, 1, &plans, &mut short).unwrap_err()[0].category,
                ResultCategory::ResourceLimit
            );
            let mut zero = 0;
            reserve_operations(&function, 1, &BTreeMap::new(), &mut zero).unwrap();
            assert_eq!(zero, 0);
        }
    }

    #[test]
    fn nested_unit_preserves_existing_zero_size_aggregate_layout() {
        for (field, stride) in [
            (Type::Unit, 8),
            (
                Type::Array {
                    element: Box::new(Type::Unit),
                    length: 3,
                },
                8,
            ),
            (Type::Tuple(vec![Type::Unit, Type::I64]), 16),
        ] {
            let r = ResourceDefinition {
                name: "unit-layout".into(),
                payload: Type::OwnedStore {
                    element: Box::new(Type::Record {
                        name: "WithUnit".into(),
                        fields: vec![("value".into(), Type::I64), ("tail".into(), field)],
                    }),
                },
                teardown: None,
            };
            let mut work = usize::MAX;
            assert_eq!(
                plan(&[r], &Limits::default(), &mut work).unwrap()[&0].stride,
                stride
            );
        }
        let mut many_units = resource(1);
        let Type::OwnedStore { element } = &mut many_units.payload else {
            unreachable!()
        };
        let Type::Record { fields, .. } = element.as_mut() else {
            unreachable!()
        };
        fields.push((
            "many_units".into(),
            Type::Array {
                element: Box::new(Type::Array {
                    element: Box::new(Type::Unit),
                    length: u32::MAX,
                }),
                length: u32::MAX,
            },
        ));
        let mut work = usize::MAX;
        assert_eq!(
            plan(&[many_units], &Limits::default(), &mut work).unwrap()[&0].stride,
            8
        );
    }

    #[test]
    fn runtime_uses_pointer_helpers_and_constant_time_pod_drop() {
        let resources = [resource(509), resource(510), resource(511), resource(32768)];
        let target = TargetProfile::default();
        let view = view(&resources, &target);
        // Even an unused large resource needs the transfer helpers declared.
        let planned = copy_aggregates::plan(&view, &Limits::default(), usize::MAX).unwrap();
        assert!(planned.needs_helpers());
        let code = runtime(&view, &BTreeMap::new(), &planned.store_protocols).unwrap();
        let items = code.into_items().unwrap();
        for index in 0..resources.len() {
            let prefix = format!("zil.store.{index}.");
            let functions = items
                .iter()
                .filter_map(|i| {
                    if let m::Item::Function(f) = i {
                        Some(f)
                    } else {
                        None
                    }
                })
                .filter(|f| f.name.starts_with(&prefix))
                .collect::<Vec<_>>();
            let drop = functions
                .iter()
                .find(|f| f.name.ends_with(".drop"))
                .unwrap();
            let blocks = drop.blocks.as_ref().unwrap();
            assert_eq!(blocks.len(), 1);
            assert_eq!(blocks[0].instructions.len(), 5);
            for (operation, memory) in [
                ("push", planned.store_protocols[&index].push.is_some()),
                ("pop", planned.store_protocols[&index].pop.is_some()),
            ] {
                let name = format!("{prefix}{operation}{}", if memory { ".memory" } else { "" });
                let helper = functions.iter().find(|f| f.name == name).unwrap();
                if memory {
                    assert_eq!(helper.signature.result, m::Ty::Void);
                    assert!(helper.signature.params.iter().all(|p| p.ty == m::Ty::Ptr));
                    for instruction in helper
                        .blocks
                        .as_ref()
                        .unwrap()
                        .iter()
                        .flat_map(|b| &b.instructions)
                    {
                        match &instruction.op {
                            m::Op::Alloca { .. } => {
                                panic!("transfer helper must not allocate loop temporaries")
                            }
                            m::Op::Load { ty, .. } => {
                                assert!(matches!(ty, m::Ty::Int(64) | m::Ty::Ptr))
                            }
                            m::Op::Store { arg, .. } => {
                                assert!(matches!(arg.ty, m::Ty::Int(1) | m::Ty::Int(64)))
                            }
                            m::Op::Return(value) => assert!(value.is_none()),
                            _ => {}
                        }
                    }
                    assert!(
                        !functions
                            .iter()
                            .any(|f| f.name == format!("{prefix}{operation}"))
                    );
                }
            }
        }
    }
}
