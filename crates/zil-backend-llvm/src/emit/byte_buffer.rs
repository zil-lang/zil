//! Explicit sealed byte storage. Only independently verified primitives reach
//! these helpers; no public pointer/length/capacity construction is exposed.
use super::*;

pub(super) fn is_resource(ty: &Type, definitions: &[ResourceDefinition]) -> bool {
    matches!(ty, Type::Managed(payload) | Type::Desc(payload)
        if payload.as_ref() == &Type::ByteBuffer
        && definitions.iter().any(|definition|
            definition.name == zil_ir::BYTE_BUFFER_RESOURCE_NAME
            && definition.payload == Type::ByteBuffer
            && definition.teardown.is_none()))
}

/// Emit allocator declarations once, checking rather than silently overriding
/// a user's foreign ABI. Buffer primitives never import a C source host.
pub(super) fn runtime(lir: &LirView<'_>) -> CompileResult<m::Code> {
    if !lir
        .resources
        .iter()
        .any(|r| r.payload == Type::ByteBuffer || matches!(r.payload, Type::OwnedStore { .. }))
    {
        return Ok(m::Code::new());
    }
    let windows = lir.target.triple == "x86_64-pc-windows-msvc";
    let declarations = if windows {
        vec![
            ("GetProcessHeap", vec![], Type::ForeignHandle),
            (
                "HeapAlloc",
                vec![Type::ForeignHandle, Type::I32, Type::I64],
                Type::ForeignHandle,
            ),
            (
                "HeapReAlloc",
                vec![
                    Type::ForeignHandle,
                    Type::I32,
                    Type::ForeignHandle,
                    Type::I64,
                ],
                Type::ForeignHandle,
            ),
            (
                "HeapFree",
                vec![Type::ForeignHandle, Type::I32, Type::ForeignHandle],
                Type::I32,
            ),
        ]
    } else {
        vec![
            (
                "realloc",
                vec![Type::ForeignHandle, Type::I64],
                Type::ForeignHandle,
            ),
            ("free", vec![Type::ForeignHandle], Type::Unit),
        ]
    };
    for function in lir.functions {
        if function
            .foreign
            .as_ref()
            .is_some_and(|f| f.symbol.starts_with("zil.bytes."))
        {
            return invalid(
                "E-CODEGEN-FOREIGN",
                "foreign symbol conflicts with byte-storage internal ABI",
                Some(function.span),
            );
        }
    }
    let mut output = m::Code::new();
    for (name, parameters, result) in declarations {
        let existing = lir
            .functions
            .iter()
            .find(|function| function.foreign.as_ref().is_some_and(|f| f.symbol == name));
        if let Some(function) = existing {
            let actual = function
                .parameters
                .iter()
                .map(|id| function.locals[id.0 as usize].ty.clone())
                .collect::<Vec<_>>();
            if actual != parameters || function.return_type != result {
                return invalid(
                    "E-CODEGEN-FOREIGN",
                    format!("{name} conflicts with the byte-storage allocator ABI"),
                    Some(function.span),
                );
            }
        } else {
            let parameters = parameters
                .iter()
                .map(|ty| Ok(m::Param::unnamed(llvm_type(ty)?)))
                .collect::<CompileResult<Vec<_>>>()?;
            output.append(m::Code::from(vec![m::Event::Declare(m::Function {
                name: name.to_string(),
                linkage: m::Linkage::External,
                signature: m::Signature {
                    result: (llvm_type(&result)?).clone(),
                    result_attrs: vec![],
                    params: (parameters).clone(),
                    variadic: false,
                },
                attrs: vec![],
                blocks: None,
            })]));
        }
    }
    if windows {
        output.append(windows_allocation());
    } else {
        output.append(linux_allocation());
    }
    output.append(storage_operations());
    Ok(output)
}

fn linux_allocation() -> m::Code {
    m::Code::from(vec![
        m::Event::Begin(m::Function {
            name: "zil.bytes.reallocate".into(),
            linkage: m::Linkage::Internal,
            signature: m::Signature {
                result: m::Ty::Ptr,
                result_attrs: vec![],
                params: vec![
                    m::Param {
                        ty: m::Ty::Ptr,
                        name: Some("old".into()),
                        attrs: vec![],
                    },
                    m::Param {
                        ty: m::Ty::Int(64),
                        name: Some("size".into()),
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
            Some("result".into()),
            m::Op::Call {
                result: m::Ty::Ptr,
                result_attrs: vec![],
                callee: m::global("realloc".into()),
                args: vec![
                    m::Arg::new(m::Ty::Ptr, m::local("old".into())),
                    m::Arg::new(m::Ty::Int(64), m::local("size".into())),
                ],
                vararg: None,
            },
        )),
        m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Return(Some(m::Arg::new(m::Ty::Ptr, m::local("result".into())))),
        )),
        m::Event::End,
        m::Event::Begin(m::Function {
            name: "zil.bytes.release".into(),
            linkage: m::Linkage::Internal,
            signature: m::Signature {
                result: m::Ty::Void,
                result_attrs: vec![],
                params: vec![m::Param {
                    ty: m::Ty::Ptr,
                    name: Some("payload".into()),
                    attrs: vec![],
                }],
                variadic: false,
            },
            attrs: vec![],
            blocks: None,
        }),
        m::Event::Label("entry".into()),
        m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Call {
                result: m::Ty::Void,
                result_attrs: vec![],
                callee: m::global("free".into()),
                args: vec![m::Arg::new(m::Ty::Ptr, m::local("payload".into()))],
                vararg: None,
            },
        )),
        m::Event::Instruction(m::Inst::new(None, m::Op::Return(None))),
        m::Event::End,
    ])
}

fn windows_allocation() -> m::Code {
    m::Code::from(vec![
        m::Event::Begin(m::Function {
            name: "zil.bytes.reallocate".into(),
            linkage: m::Linkage::Internal,
            signature: m::Signature {
                result: m::Ty::Ptr,
                result_attrs: vec![],
                params: vec![
                    m::Param {
                        ty: m::Ty::Ptr,
                        name: Some("old".into()),
                        attrs: vec![],
                    },
                    m::Param {
                        ty: m::Ty::Int(64),
                        name: Some("size".into()),
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
            Some("heap".into()),
            m::Op::Call {
                result: m::Ty::Ptr,
                result_attrs: vec![],
                callee: m::global("GetProcessHeap".into()),
                args: vec![],
                vararg: None,
            },
        )),
        m::Event::Instruction(m::Inst::new(
            Some("heap.ok".into()),
            m::Op::Compare {
                predicate: "ne".into(),
                ty: m::Ty::Ptr,
                left: m::local("heap".into()),
                right: m::Val::Null,
            },
        )),
        m::Event::Instruction(m::Inst::new(
            None,
            m::Op::CondBranch {
                condition: m::local("heap.ok".into()),
                yes: "choose".into(),
                no: "failed".into(),
            },
        )),
        m::Event::Label("choose".into()),
        m::Event::Instruction(m::Inst::new(
            Some("empty".into()),
            m::Op::Compare {
                predicate: "eq".into(),
                ty: m::Ty::Ptr,
                left: m::local("old".into()),
                right: m::Val::Null,
            },
        )),
        m::Event::Instruction(m::Inst::new(
            None,
            m::Op::CondBranch {
                condition: m::local("empty".into()),
                yes: "allocate".into(),
                no: "reallocate".into(),
            },
        )),
        m::Event::Label("allocate".into()),
        m::Event::Instruction(m::Inst::new(
            Some("fresh".into()),
            m::Op::Call {
                result: m::Ty::Ptr,
                result_attrs: vec![],
                callee: m::global("HeapAlloc".into()),
                args: vec![
                    m::Arg::new(m::Ty::Ptr, m::local("heap".into())),
                    m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                    m::Arg::new(m::Ty::Int(64), m::local("size".into())),
                ],
                vararg: None,
            },
        )),
        m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Return(Some(m::Arg::new(m::Ty::Ptr, m::local("fresh".into())))),
        )),
        m::Event::Label("reallocate".into()),
        m::Event::Instruction(m::Inst::new(
            Some("grown".into()),
            m::Op::Call {
                result: m::Ty::Ptr,
                result_attrs: vec![],
                callee: m::global("HeapReAlloc".into()),
                args: vec![
                    m::Arg::new(m::Ty::Ptr, m::local("heap".into())),
                    m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                    m::Arg::new(m::Ty::Ptr, m::local("old".into())),
                    m::Arg::new(m::Ty::Int(64), m::local("size".into())),
                ],
                vararg: None,
            },
        )),
        m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Return(Some(m::Arg::new(m::Ty::Ptr, m::local("grown".into())))),
        )),
        m::Event::Label("failed".into()),
        m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Return(Some(m::Arg::new(m::Ty::Ptr, m::Val::Null))),
        )),
        m::Event::End,
        m::Event::Begin(m::Function {
            name: "zil.bytes.release".into(),
            linkage: m::Linkage::Internal,
            signature: m::Signature {
                result: m::Ty::Void,
                result_attrs: vec![],
                params: vec![m::Param {
                    ty: m::Ty::Ptr,
                    name: Some("payload".into()),
                    attrs: vec![],
                }],
                variadic: false,
            },
            attrs: vec![],
            blocks: None,
        }),
        m::Event::Label("entry".into()),
        m::Event::Instruction(m::Inst::new(
            Some("empty".into()),
            m::Op::Compare {
                predicate: "eq".into(),
                ty: m::Ty::Ptr,
                left: m::local("payload".into()),
                right: m::Val::Null,
            },
        )),
        m::Event::Instruction(m::Inst::new(
            None,
            m::Op::CondBranch {
                condition: m::local("empty".into()),
                yes: "done".into(),
                no: "release".into(),
            },
        )),
        m::Event::Label("release".into()),
        m::Event::Instruction(m::Inst::new(
            Some("heap".into()),
            m::Op::Call {
                result: m::Ty::Ptr,
                result_attrs: vec![],
                callee: m::global("GetProcessHeap".into()),
                args: vec![],
                vararg: None,
            },
        )),
        m::Event::Instruction(m::Inst::new(
            Some("released".into()),
            m::Op::Call {
                result: m::Ty::Int(32),
                result_attrs: vec![],
                callee: m::global("HeapFree".into()),
                args: vec![
                    m::Arg::new(m::Ty::Ptr, m::local("heap".into())),
                    m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                    m::Arg::new(m::Ty::Ptr, m::local("payload".into())),
                ],
                vararg: None,
            },
        )),
        m::Event::Instruction(m::Inst::new(
            Some("ok".into()),
            m::Op::Compare {
                predicate: "ne".into(),
                ty: m::Ty::Int(32),
                left: m::local("released".into()),
                right: m::Val::Int(0),
            },
        )),
        m::Event::Instruction(m::Inst::new(
            None,
            m::Op::CondBranch {
                condition: m::local("ok".into()),
                yes: "done".into(),
                no: "failed".into(),
            },
        )),
        m::Event::Label("failed".into()),
        m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Call {
                result: m::Ty::Void,
                result_attrs: vec![],
                callee: m::global("llvm.trap".into()),
                args: vec![],
                vararg: None,
            },
        )),
        m::Event::Instruction(m::Inst::new(None, m::Op::Unreachable)),
        m::Event::Label("done".into()),
        m::Event::Instruction(m::Inst::new(None, m::Op::Return(None))),
        m::Event::End,
    ])
}

fn storage_operations() -> m::Code {
    m::Code::from(vec![
        m::Event::Begin(m::Function {
            name: "zil.bytes.reserve".into(),
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
            Some("negative".into()),
            m::Op::Compare {
                predicate: "slt".into(),
                ty: m::Ty::Int(64),
                left: m::local("requested".into()),
                right: m::Val::Int(0),
            },
        )),
        m::Event::Instruction(m::Inst::new(
            None,
            m::Op::CondBranch {
                condition: m::local("negative".into()),
                yes: "failed".into(),
                no: "capacity".into(),
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
            Some("next".into()),
            m::Op::Call {
                result: m::Ty::Ptr,
                result_attrs: vec![],
                callee: m::global("zil.bytes.reallocate".into()),
                args: vec![
                    m::Arg::new(m::Ty::Ptr, m::local("old".into())),
                    m::Arg::new(m::Ty::Int(64), m::local("requested".into())),
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
            name: "zil.bytes.truncate".into(),
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
            Some("nonnegative".into()),
            m::Op::Compare {
                predicate: "sge".into(),
                ty: m::Ty::Int(64),
                left: m::local("requested".into()),
                right: m::Val::Int(0),
            },
        )),
        m::Event::Instruction(m::Inst::new(
            Some("within".into()),
            m::Op::Compare {
                predicate: "ule".into(),
                ty: m::Ty::Int(64),
                left: m::local("requested".into()),
                right: m::local("len".into()),
            },
        )),
        m::Event::Instruction(m::Inst::new(
            Some("valid".into()),
            m::Op::Binary {
                opcode: "and".into(),
                ty: m::Ty::Int(1),
                left: m::local("nonnegative".into()),
                right: m::local("within".into()),
            },
        )),
        m::Event::Instruction(m::Inst::new(
            None,
            m::Op::CondBranch {
                condition: m::local("valid".into()),
                yes: "publish".into(),
                no: "failed".into(),
            },
        )),
        m::Event::Label("publish".into()),
        m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Store {
                arg: m::Arg::new(m::Ty::Int(64), m::local("requested".into())),
                ptr: m::local("len.field".into()),
                align: 8,
            },
        )),
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
            name: "zil.bytes.push".into(),
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
                        ty: m::Ty::Int(8),
                        name: Some("byte".into()),
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
            Some("payload".into()),
            m::Op::Load {
                ty: m::Ty::Ptr,
                ptr: m::local("ptr.field".into()),
                align: 8,
            },
        )),
        m::Event::Instruction(m::Inst::new(
            Some("slot".into()),
            m::Op::Gep {
                ty: m::Ty::Int(8),
                ptr: m::local("payload".into()),
                indices: vec![m::Arg::new(m::Ty::Int(64), m::local("len".into()))],
            },
        )),
        m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Store {
                arg: m::Arg::new(m::Ty::Int(8), m::local("byte".into())),
                ptr: m::local("slot".into()),
                align: 1,
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
            m::Op::Return(Some(m::Arg::new(m::Ty::Int(1), m::Val::Bool(true)))),
        )),
        m::Event::Label("full".into()),
        m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Return(Some(m::Arg::new(m::Ty::Int(1), m::Val::Bool(false)))),
        )),
        m::Event::End,
        m::Event::Begin(m::Function {
            name: "zil.bytes.drop".into(),
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
            Some("payload".into()),
            m::Op::Load {
                ty: m::Ty::Ptr,
                ptr: m::local("ptr.field".into()),
                align: 8,
            },
        )),
        m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Call {
                result: m::Ty::Void,
                result_attrs: vec![],
                callee: m::global("zil.bytes.release".into()),
                args: vec![m::Arg::new(m::Ty::Ptr, m::local("payload".into()))],
                vararg: None,
            },
        )),
        m::Event::Instruction(m::Inst::new(None, m::Op::Return(None))),
        m::Event::End,
    ])
}

impl FunctionEmitter<'_> {
    pub(super) fn byte_buffer_instruction(
        &mut self,
        instruction: &Instruction,
    ) -> CompileResult<()> {
        let output = result(instruction)?;
        match instruction.kind {
            InstructionKind::ByteBufferNew => {
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((value(output)).clone())),
                    m::Op::Freeze(m::Arg::new(
                        m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64), m::Ty::Int(64)]),
                        m::Val::Zero,
                    )),
                )));
            }
            InstructionKind::ByteBufferReserve { buffer, capacity } => {
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((value(output)).clone())),
                    m::Op::Call {
                        result: m::Ty::Int(1),
                        result_attrs: vec![],
                        callee: m::global("zil.bytes.reserve".into()),
                        args: vec![
                            m::Arg::new(m::Ty::Ptr, m::val((value(buffer)).clone())),
                            m::Arg::new(m::Ty::Int(64), m::val((value(capacity)).clone())),
                        ],
                        vararg: None,
                    },
                )));
            }
            InstructionKind::ByteBufferTruncate { buffer, length } => {
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((value(output)).clone())),
                    m::Op::Call {
                        result: m::Ty::Int(1),
                        result_attrs: vec![],
                        callee: m::global("zil.bytes.truncate".into()),
                        args: vec![
                            m::Arg::new(m::Ty::Ptr, m::val((value(buffer)).clone())),
                            m::Arg::new(m::Ty::Int(64), m::val((value(length)).clone())),
                        ],
                        vararg: None,
                    },
                )));
            }
            InstructionKind::ByteBufferPush {
                buffer,
                value: byte,
            } => {
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((value(output)).clone())),
                    m::Op::Call {
                        result: m::Ty::Int(1),
                        result_attrs: vec![],
                        callee: m::global("zil.bytes.push".into()),
                        args: vec![
                            m::Arg::new(m::Ty::Ptr, m::val((value(buffer)).clone())),
                            m::Arg::new(m::Ty::Int(8), m::val((value(byte)).clone())),
                        ],
                        vararg: None,
                    },
                )));
            }
            InstructionKind::ByteBufferLen { buffer }
            | InstructionKind::ByteBufferCapacity { buffer } => {
                let index = if matches!(instruction.kind, InstructionKind::ByteBufferLen { .. }) {
                    1
                } else {
                    2
                };
                let field = self.temp();
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((field).clone())),
                    m::Op::Gep {
                        ty: m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64), m::Ty::Int(64)]),
                        ptr: m::val((value(buffer)).clone()),
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
                        ptr: m::val((field).clone()),
                        align: 8,
                    },
                )));
            }
            InstructionKind::ByteBufferBorrow { buffer, .. } => {
                let pointer = self.temp();
                let length_field = self.temp();
                let length = self.temp();
                let pair = self.temp();
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((pointer).clone())),
                    m::Op::Load {
                        ty: m::Ty::Ptr,
                        ptr: m::val((value(buffer)).clone()),
                        align: 8,
                    },
                )));
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((length_field).clone())),
                    m::Op::Gep {
                        ty: m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64), m::Ty::Int(64)]),
                        ptr: m::val((value(buffer)).clone()),
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
                        ptr: m::val((length_field).clone()),
                        align: 8,
                    },
                )));
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((pair).clone())),
                    m::Op::Insert {
                        arg: m::Arg::new(
                            m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64)]),
                            m::Val::Poison,
                        ),
                        index: 0,
                        field: m::Arg::new(m::Ty::Ptr, m::val((pointer).clone())),
                    },
                )));
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((value(output)).clone())),
                    m::Op::Insert {
                        arg: m::Arg::new(
                            m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64)]),
                            m::val((pair).clone()),
                        ),
                        index: 1,
                        field: m::Arg::new(m::Ty::Int(64), m::val((length).clone())),
                    },
                )));
            }
            _ => {
                return invalid(
                    "E-CODEGEN-CFG",
                    "expected byte-storage primitive",
                    Some(instruction.span),
                );
            }
        }
        Ok(())
    }
}
