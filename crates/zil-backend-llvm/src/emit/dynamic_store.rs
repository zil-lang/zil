//! A sealed runtime prefix. Whole elements move as their ordinary inline ABI.
//! No element pointers or mutable head fields escape these checked operations.
use super::*;

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

pub(super) fn runtime(lir: &LirModule) -> CompileResult<m::Code> {
    if !lir
        .resources
        .iter()
        .any(|r| matches!(r.payload, Type::OwnedStore { .. }))
    {
        return Ok(m::Code::new());
    }
    if lir.target.triple != "x86_64-unknown-linux-gnu" {
        return unsupported("owned Store currently requires Linux x64 LLVM", None);
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
        let Type::Package { fields, .. } = element.as_ref() else {
            return invalid(
                "E-CODEGEN-CFG",
                "Store runtime element is not a Package",
                None,
            );
        };
        let item = llvm_type(element)?;
        let align = alignment(element, &BTreeMap::new())?;
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
        for (field_index, (_, ty)) in fields.iter().enumerate() {
            let Type::Managed(payload) = ty else { continue };
            let definition = lir
                .resources
                .iter()
                .find(|r| r.payload == **payload)
                .ok_or_else(|| {
                    vec![Diagnostic::error(
                        Phase::Codegen,
                        "E-CODEGEN-CFG",
                        None,
                        "Store element field lacks its resource definition",
                    )]
                })?;
            let teardown = if definition.payload == Type::ByteBuffer {
                Some("zil.bytes.drop".into())
            } else {
                definition.teardown.map(symbol)
            };
            if let Some(teardown) = teardown {
                output.append(m::Code::from(vec![
                    m::Event::Instruction(m::Inst::new(
                        Some(format!("field.{}", field_index)),
                        m::Op::Gep {
                            ty: (item).clone(),
                            ptr: m::local("slot".into()),
                            indices: vec![
                                m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                                m::Arg::new(m::Ty::Int(32), m::val(field_index)),
                            ],
                        },
                    )),
                    m::Event::Instruction(m::Inst::new(
                        None,
                        m::Op::Call {
                            result: m::Ty::Void,
                            result_attrs: vec![],
                            callee: m::global(teardown.to_string()),
                            args: vec![m::Arg::new(
                                m::Ty::Ptr,
                                m::local(format!("field.{}", field_index)),
                            )],
                            vararg: None,
                        },
                    )),
                ]));
            }
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

impl FunctionEmitter<'_> {
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
            | InstructionKind::OwnedStoreBorrow { store, .. } => store,
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
            InstructionKind::OwnedStoreBorrow { index, .. } => {
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
