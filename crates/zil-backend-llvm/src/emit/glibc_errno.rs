//! A scalar-only unsafe C ABI for glibc's thread-local conversion status.
//! The libc pointer never becomes a language value or a borrowed reference.
use super::*;

pub(super) const EXCHANGE: &str = "zil_glibc_errno_exchange";

pub(super) fn runtime(lir: &LirView<'_>) -> CompileResult<m::Code> {
    let mut used = false;
    let mut location_declared = false;
    for function in lir.functions {
        let Some(foreign) = &function.foreign else {
            continue;
        };
        if foreign.symbol == EXCHANGE {
            if lir.target.triple != "x86_64-unknown-linux-gnu"
                || function.return_type != Type::I32
                || function.parameters.len() != 1
                || function.locals[function.parameters[0].0 as usize].ty != Type::I32
            {
                return invalid(
                    "E-CODEGEN-FOREIGN",
                    "zil_glibc_errno_exchange requires the Linux/glibc i32(i32) ABI",
                    Some(function.span),
                );
            }
            used = true;
        }
        if foreign.symbol == "__errno_location" {
            if !function.parameters.is_empty() || function.return_type != Type::ForeignHandle {
                return invalid(
                    "E-CODEGEN-FOREIGN",
                    "__errno_location conflicts with the Linux/glibc TLS ABI",
                    Some(function.span),
                );
            }
            location_declared = true;
        }
    }
    if !used {
        return Ok(m::Code::new());
    }
    let mut code = m::Code::new();
    if !location_declared {
        code.push(m::Event::Declare(m::Function {
            name: "__errno_location".into(),
            linkage: m::Linkage::External,
            signature: m::Signature {
                result: m::Ty::Ptr,
                result_attrs: vec![],
                params: vec![],
                variadic: false,
            },
            attrs: vec![],
            blocks: None,
        }));
    }
    code.append(m::Code::from(vec![
        m::Event::Begin(m::Function {
            name: EXCHANGE.into(),
            linkage: m::Linkage::Internal,
            signature: m::Signature {
                result: m::Ty::Int(32),
                result_attrs: vec![],
                params: vec![m::Param {
                    ty: m::Ty::Int(32),
                    name: Some("next".into()),
                    attrs: vec![],
                }],
                variadic: false,
            },
            attrs: vec![],
            blocks: None,
        }),
        m::Event::Label("entry".into()),
        m::Event::Instruction(m::Inst::new(
            Some("location".into()),
            m::Op::Call {
                result: m::Ty::Ptr,
                result_attrs: vec![],
                callee: m::global("__errno_location".into()),
                args: vec![],
                vararg: None,
            },
        )),
        m::Event::Instruction(m::Inst::new(
            Some("previous".into()),
            m::Op::Load {
                ty: m::Ty::Int(32),
                ptr: m::local("location".into()),
                align: 4,
            },
        )),
        m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Store {
                arg: m::Arg::new(m::Ty::Int(32), m::local("next".into())),
                ptr: m::local("location".into()),
                align: 4,
            },
        )),
        m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Return(Some(m::Arg::new(
                m::Ty::Int(32),
                m::local("previous".into()),
            ))),
        )),
        m::Event::End,
    ]));
    Ok(code)
}
