//! Finite synchronous errors use an inline tag and two fixed payload slots.
//! The inactive payload is zero initialized; no allocation or unwind is hidden
//! here. This deliberately simple representation is not a compact-union ABI.
use super::*;

pub(super) fn owned(ty: &Type) -> bool {
    matches!(ty, Type::ErrorUnion { error, result }
        if matches!(error.as_ref(), Type::Package { .. })
            || matches!(result.as_ref(), Type::Package { .. } | Type::Managed(_)))
}

pub(super) fn supported_type(ty: &Type, resources: &[ResourceDefinition]) -> bool {
    matches!(ty, Type::ErrorUnion { error, result }
        if ((error.is_copy_value() || packages::supported_type(error, resources))
            && (result.is_copy_value() || packages::supported_type(result, resources)))
            || (error.is_copy_value() && matches!(result.as_ref(), Type::Managed(_))
                && resources::supported_type(result, resources)))
}

fn teardowns(
    payload: &Type,
    resources: &[ResourceDefinition],
) -> CompileResult<Vec<(Vec<u32>, String)>> {
    if matches!(payload, Type::Package { .. } | Type::Managed(_)) {
        packages::teardown_paths(payload, resources)
    } else {
        Ok(Vec::new())
    }
}

/// The native boundary consumes the returned error Value. Source main's own
/// cleanup has already run; only this selected returned payload remains owed.
pub(super) fn entry_cleanup(ty: &Type, resources: &[ResourceDefinition]) -> CompileResult<m::Code> {
    let Type::ErrorUnion { error, .. } = ty else {
        return Ok(m::Code::new());
    };
    if !matches!(error.as_ref(), Type::Package { .. }) {
        return Ok(m::Code::new());
    }
    let union = llvm_type(ty)?;
    let payload = llvm_type(error)?;
    let mut text = m::Code::from(vec![
        m::Event::Instruction(m::Inst::new(
            Some("entry.error.value".into()),
            m::Op::Extract {
                arg: m::Arg::new((union).clone(), m::local("entry.result".into())),
                index: 1,
            },
        )),
        m::Event::Instruction(m::Inst::new(
            Some("entry.error.storage".into()),
            m::Op::Alloca {
                ty: (payload).clone(),
                align: 0,
            },
        )),
        m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Store {
                arg: m::Arg::new((payload).clone(), m::local("entry.error.value".into())),
                ptr: m::local("entry.error.storage".into()),
                align: 0,
            },
        )),
    ]);
    for (index, (path, teardown)) in teardowns(error, resources)?.iter().enumerate() {
        let suffix = path
            .iter()
            .map(|index| m::Arg::new(m::Ty::Int(32), *index))
            .collect::<Vec<_>>();
        text.append(m::Code::from(vec![
            m::Event::Instruction(m::Inst::new(
                Some(format!("entry.error.field.{}", index)),
                m::Op::Gep {
                    ty: (payload).clone(),
                    ptr: m::local("entry.error.storage".into()),
                    indices: {
                        let mut args = vec![m::Arg::new(m::Ty::Int(32), m::Val::Int(0))];
                        args.extend((suffix).clone());
                        args
                    },
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
                        m::local(format!("entry.error.field.{}", index)),
                    )],
                    vararg: None,
                },
            )),
        ]));
    }
    Ok(text)
}

impl FunctionEmitter<'_> {
    pub(super) fn drop_error_union(
        &mut self,
        instruction: &Instruction,
        local: LocalId,
    ) -> CompileResult<()> {
        let ty = self.local_type(local)?.clone();
        let Type::ErrorUnion { error, result } = &ty else {
            return invalid(
                "E-CODEGEN-CFG",
                "error cleanup requires an error union local",
                Some(instruction.span),
            );
        };
        let union = llvm_type(&ty)?;
        let error_drops = teardowns(error, self.resources)?;
        let success_drops = teardowns(result, self.resources)?;
        let flag = self.temp();
        let label = self.continuation;
        self.continuation += 1;
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((flag).clone())),
            m::Op::Load {
                ty: m::Ty::Int(1),
                ptr: m::local(format!("resource.live.{}", local.0)),
                align: 1,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::CondBranch {
                condition: m::val((flag).clone()),
                yes: format!("error.drop.{}", label),
                no: format!("error.done.{}", label),
            },
        )));
        self.event(m::Event::Label(format!("error.drop.{}", label)));
        // Cleanup owns the selected carrier before it can enter user teardown.
        // The inactive payload is never read or given a cleanup obligation.
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Store {
                arg: m::Arg::new(m::Ty::Int(1), m::Val::Bool(false)),
                ptr: m::local(format!("resource.live.{}", local.0)),
                align: 1,
            },
        )));
        let tag_pointer = self.temp();
        let tag = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((tag_pointer).clone())),
            m::Op::Gep {
                ty: (union).clone(),
                ptr: m::local(format!("local.{}", local.0)),
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                    m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                ],
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((tag).clone())),
            m::Op::Load {
                ty: m::Ty::Int(1),
                ptr: m::val((tag_pointer).clone()),
                align: 1,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::CondBranch {
                condition: m::val((tag).clone()),
                yes: format!("error.payload.{}", label),
                no: format!("success.payload.{}", label),
            },
        )));
        for (name, arm, payload, drops) in [
            ("error", 1, error.as_ref(), error_drops),
            ("success", 2, result.as_ref(), success_drops),
        ] {
            self.event(m::Event::Label(format!("{}.payload.{}", name, label)));
            if !drops.is_empty() {
                let pointer = self.temp();
                let payload_llvm = llvm_type(payload)?;
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((pointer).clone())),
                    m::Op::Gep {
                        ty: (union).clone(),
                        ptr: m::local(format!("local.{}", local.0)),
                        indices: vec![
                            m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                            m::Arg::new(m::Ty::Int(32), m::val(arm)),
                        ],
                    },
                )));
                for (path, teardown) in drops {
                    let field = self.temp();
                    let suffix = path
                        .iter()
                        .map(|index| m::Arg::new(m::Ty::Int(32), *index))
                        .collect::<Vec<_>>();
                    self.event(m::Event::Instruction(m::Inst::new(
                        Some(m::name((field).clone())),
                        m::Op::Gep {
                            ty: (payload_llvm).clone(),
                            ptr: m::val((pointer).clone()),
                            indices: {
                                let mut args = vec![m::Arg::new(m::Ty::Int(32), m::Val::Int(0))];
                                args.extend((suffix).clone());
                                args
                            },
                        },
                    )));
                    self.event(m::Event::Instruction(m::Inst::new(
                        None,
                        m::Op::Call {
                            result: m::Ty::Void,
                            result_attrs: vec![],
                            callee: m::global(teardown.to_string()),
                            args: vec![m::Arg::new(m::Ty::Ptr, m::val((field).clone()))],
                            vararg: None,
                        },
                    )));
                }
            }
            self.event(m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Branch(format!("error.done.{}", label)),
            )));
        }
        self.event(m::Event::Label(format!("error.done.{}", label)));
        Ok(())
    }

    pub(super) fn make_error_union(
        &mut self,
        instruction: &Instruction,
        is_error: bool,
        source: ValueId,
    ) -> CompileResult<()> {
        let result = result(instruction)?;
        let ty = &self.function.values[result.0 as usize];
        let Type::ErrorUnion {
            error,
            result: success,
        } = ty
        else {
            return unsupported(
                "verified error construction lost its union type",
                Some(instruction.span),
            );
        };
        let payload = if is_error {
            error.as_ref()
        } else {
            success.as_ref()
        };
        let llvm = llvm_type(ty)?;
        let payload_llvm = aggregate_field_type(payload)?;
        let source = if *payload == Type::Unit {
            "zeroinitializer".into()
        } else {
            value(source)
        };
        let tagged = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((tagged).clone())),
            m::Op::Insert {
                arg: m::Arg::new((llvm).clone(), m::Val::Zero),
                index: 0,
                field: m::Arg::new(m::Ty::Int(1), m::val(is_error)),
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((value(result)).clone())),
            m::Op::Insert {
                arg: m::Arg::new((llvm).clone(), m::val((tagged).clone())),
                index: (if is_error { 1 } else { 2 }) as u32,
                field: m::Arg::new((payload_llvm).clone(), m::val((source).clone())),
            },
        )));
        Ok(())
    }

    pub(super) fn error_tag(
        &mut self,
        instruction: &Instruction,
        source: ValueId,
    ) -> CompileResult<()> {
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((value(result(instruction)?)).clone())),
            m::Op::Extract {
                arg: m::Arg::new(
                    (llvm_type(self.value_type(source)?)?).clone(),
                    m::val((value(source)).clone()),
                ),
                index: 0,
            },
        )));
        Ok(())
    }

    pub(super) fn read_error_union(
        &mut self,
        instruction: &Instruction,
        source: ValueId,
        is_error: bool,
    ) -> CompileResult<()> {
        let result = result(instruction)?;
        let llvm = llvm_type(self.value_type(source)?)?;
        let tag = self.temp();
        let mismatch = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((tag).clone())),
            m::Op::Extract {
                arg: m::Arg::new((llvm).clone(), m::val((value(source)).clone())),
                index: 0,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((mismatch).clone())),
            m::Op::Compare {
                predicate: "ne".into(),
                ty: m::Ty::Int(1),
                left: m::val((tag).clone()),
                right: m::val(is_error),
            },
        )));
        // A raw-but-verified projection has defined failure semantics even if
        // its caller did not establish the branch tag statically.
        self.trap_if(&mismatch);
        if self.function.values[result.0 as usize] != Type::Unit {
            self.event(m::Event::Instruction(m::Inst::new(
                Some(m::name((value(result)).clone())),
                m::Op::Extract {
                    arg: m::Arg::new((llvm).clone(), m::val((value(source)).clone())),
                    index: (if is_error { 1 } else { 2 }) as u32,
                },
            )));
        }
        Ok(())
    }
}
