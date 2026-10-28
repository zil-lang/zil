//! Fixed inline owning arrays. Element flags belong to real local storage;
//! complete SSA values and function ABI values contain only element payloads.
use super::*;

pub(super) fn supported_type(ty: &Type, resources: &[ResourceDefinition]) -> bool {
    matches!(ty, Type::Array { element, length }
        if *length > 0 && (packages::supported_type(element, resources) || matches!(element.as_ref(), Type::Closure(_))))
}

impl FunctionEmitter<'_> {
    pub(super) fn owns_storage(&self, ty: &Type) -> bool {
        self.owns_resource(ty) || supported_type(ty, self.resources)
    }

    pub(super) fn owned_array_flags(
        &mut self,
        local: LocalId,
        allocate: bool,
        live: bool,
    ) -> CompileResult<()> {
        let ty = self.local_type(local)?;
        if !supported_type(ty, self.resources) {
            return Ok(());
        }
        let Type::Array { length, .. } = ty else {
            unreachable!()
        };
        let length = *length;
        for index in 0..length {
            if allocate {
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(format!("array.live.{}.{}", local.0, index)),
                    m::Op::Alloca {
                        ty: m::Ty::Int(1),
                        align: 1,
                    },
                )));
            }
            self.event(m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Store {
                    arg: m::Arg::new(m::Ty::Int(1), m::val(live)),
                    ptr: m::local(format!("array.live.{}.{}", local.0, index)),
                    align: 1,
                },
            )));
        }
        Ok(())
    }

    fn owned_array_shape(&self, local: LocalId) -> CompileResult<(m::Ty, Type, u32)> {
        let ty = self.local_type(local)?;
        let Type::Array { element, length } = ty else {
            return invalid(
                "E-CODEGEN-CFG",
                "owning array operation requires an array local",
                Some(self.function.span),
            );
        };
        if !supported_type(ty, self.resources) {
            return invalid(
                "E-CODEGEN-CFG",
                "owning array element has no supported cleanup",
                Some(self.function.span),
            );
        }
        Ok((llvm_type(ty)?, element.as_ref().clone(), *length))
    }

    pub(super) fn owned_array_instruction(
        &mut self,
        instruction: &Instruction,
    ) -> CompileResult<()> {
        let (local, index) = match instruction.kind {
            InstructionKind::ProjectOwnedArrayField { local, index, .. }
            | InstructionKind::TakeOwnedArrayElement { local, index }
            | InstructionKind::DropOwnedArrayElement { local, index }
            | InstructionKind::StoreOwnedArrayElement { local, index, .. }
            | InstructionKind::BorrowOwnedArrayElement { local, index, .. } => (local, Some(index)),
            InstructionKind::DropOwnedArray { local } => (local, None),
            _ => unreachable!("owning array local operations"),
        };
        let (array, element, length) = self.owned_array_shape(local)?;
        if index.is_some_and(|index| index >= length) {
            return invalid(
                "E-CODEGEN-CFG",
                "owning array index exceeds its fixed length",
                Some(instruction.span),
            );
        }
        match instruction.kind {
            InstructionKind::ProjectOwnedArrayField { index, field, .. } => {
                let Type::Package { fields, .. } = &element else {
                    unreachable!()
                };
                let (_, ty) = fields.get(field as usize).ok_or_else(|| {
                    vec![Diagnostic::error(
                        Phase::Codegen,
                        "E-CODEGEN-CFG",
                        Some(instruction.span),
                        "owning array Package field is absent",
                    )]
                })?;
                if *ty == Type::Unit {
                    return Ok(());
                }
                if matches!(ty, Type::Managed(_)) {
                    self.event(m::Event::Instruction(m::Inst::new(
                        Some(m::name((value(result(instruction)?)).clone())),
                        m::Op::Gep {
                            ty: (array).clone(),
                            ptr: m::local(format!("local.{}", local.0)),
                            indices: vec![
                                m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                                m::Arg::new(m::Ty::Int(32), m::val(index)),
                                m::Arg::new(m::Ty::Int(32), m::val(field)),
                            ],
                        },
                    )));
                } else {
                    let pointer = self.temp();
                    self.event(m::Event::Instruction(m::Inst::new(
                        Some(m::name((pointer).clone())),
                        m::Op::Gep {
                            ty: (array).clone(),
                            ptr: m::local(format!("local.{}", local.0)),
                            indices: vec![
                                m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                                m::Arg::new(m::Ty::Int(32), m::val(index)),
                                m::Arg::new(m::Ty::Int(32), m::val(field)),
                            ],
                        },
                    )));
                    self.event(m::Event::Instruction(m::Inst::new(
                        Some(m::name((value(result(instruction)?)).clone())),
                        m::Op::Load {
                            ty: (llvm_type(ty)?).clone(),
                            ptr: m::val((pointer).clone()),
                            align: (alignment(ty, self.closures)?) as u32,
                        },
                    )));
                }
            }
            InstructionKind::TakeOwnedArrayElement { index, .. } => {
                let pointer = self.temp();
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((pointer).clone())),
                    m::Op::Gep {
                        ty: (array).clone(),
                        ptr: m::local(format!("local.{}", local.0)),
                        indices: vec![
                            m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                            m::Arg::new(m::Ty::Int(32), m::val(index)),
                        ],
                    },
                )));
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((value(result(instruction)?)).clone())),
                    m::Op::Load {
                        ty: (llvm_type(&element)?).clone(),
                        ptr: m::val((pointer).clone()),
                        align: (alignment(&element, self.closures)?) as u32,
                    },
                )));
                self.event(m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Store {
                        arg: m::Arg::new(m::Ty::Int(1), m::Val::Bool(false)),
                        ptr: m::local(format!("array.live.{}.{}", local.0, index)),
                        align: 1,
                    },
                )));
            }
            InstructionKind::StoreOwnedArrayElement {
                index,
                value: input,
                ..
            } => {
                if self.value_type(input)? != &element {
                    return invalid(
                        "E-CODEGEN-CFG",
                        "owning array store type differs",
                        Some(instruction.span),
                    );
                }
                let pointer = self.temp();
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((pointer).clone())),
                    m::Op::Gep {
                        ty: (array).clone(),
                        ptr: m::local(format!("local.{}", local.0)),
                        indices: vec![
                            m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                            m::Arg::new(m::Ty::Int(32), m::val(index)),
                        ],
                    },
                )));
                self.event(m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Store {
                        arg: m::Arg::new(
                            (llvm_type(&element)?).clone(),
                            m::val((value(input)).clone()),
                        ),
                        ptr: m::val((pointer).clone()),
                        align: (alignment(&element, self.closures)?) as u32,
                    },
                )));
                self.event(m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Store {
                        arg: m::Arg::new(m::Ty::Int(1), m::Val::Bool(true)),
                        ptr: m::local(format!("array.live.{}.{}", local.0, index)),
                        align: 1,
                    },
                )));
            }
            InstructionKind::DropOwnedArrayElement { index, .. } => {
                let teardowns = self.owned_array_teardowns(&element)?;
                self.drop_owned_array_element(local, index, &array, &teardowns)?;
            }
            InstructionKind::DropOwnedArray { .. } => {
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
                        yes: format!("array.drop.{}", label),
                        no: format!("array.done.{}", label),
                    },
                )));
                self.event(m::Event::Label(format!("array.drop.{}", label)));
                self.event(m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Store {
                        arg: m::Arg::new(m::Ty::Int(1), m::Val::Bool(false)),
                        ptr: m::local(format!("resource.live.{}", local.0)),
                        align: 1,
                    },
                )));
                let teardowns = self.owned_array_teardowns(&element)?;
                for index in 0..length {
                    self.drop_owned_array_element(local, index, &array, &teardowns)?;
                }
                self.event(m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Branch(format!("array.done.{}", label)),
                )));
                self.event(m::Event::Label(format!("array.done.{}", label)));
            }
            InstructionKind::BorrowOwnedArrayElement { index, .. } => {
                if !matches!(element, Type::Closure(_)) {
                    return invalid(
                        "E-CODEGEN-CFG",
                        "callback array borrow requires a concrete closure",
                        Some(instruction.span),
                    );
                }
                self.event(m::Event::Instruction(m::Inst::new(
                    Some(m::name((value(result(instruction)?)).clone())),
                    m::Op::Gep {
                        ty: (array).clone(),
                        ptr: m::local(format!("local.{}", local.0)),
                        indices: vec![
                            m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                            m::Arg::new(m::Ty::Int(32), m::val(index)),
                        ],
                    },
                )));
            }
            _ => unreachable!("owning array local operation"),
        }
        Ok(())
    }

    fn owned_array_teardowns(&self, element: &Type) -> CompileResult<Vec<(Vec<u32>, String)>> {
        if let Type::Closure(id) = element {
            let definition = self.closures.get(id).ok_or_else(|| {
                vec![Diagnostic::error(
                    Phase::Codegen,
                    "E-CODEGEN-CFG",
                    Some(self.function.span),
                    "callback array environment is absent",
                )]
            })?;
            return Ok(if resource_closures::owns(definition) {
                vec![(Vec::new(), format!("zil.closure.drop.{}", id.0))]
            } else {
                vec![]
            });
        }
        packages::teardown_paths(element, self.resources)
    }

    fn drop_owned_array_element(
        &mut self,
        local: LocalId,
        index: u32,
        array: &m::Ty,
        teardowns: &[(Vec<u32>, String)],
    ) -> CompileResult<()> {
        let flag = self.temp();
        let label = self.continuation;
        self.continuation += 1;
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((flag).clone())),
            m::Op::Load {
                ty: m::Ty::Int(1),
                ptr: m::local(format!("array.live.{}.{}", local.0, index)),
                align: 1,
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::CondBranch {
                condition: m::val((flag).clone()),
                yes: format!("array.element.drop.{}", label),
                no: format!("array.element.done.{}", label),
            },
        )));
        self.event(m::Event::Label(format!("array.element.drop.{}", label)));
        // Retire the element before invoking teardown. Its Package is complete,
        // so no hidden nested partial-state flags are read after any payload free.
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Store {
                arg: m::Arg::new(m::Ty::Int(1), m::Val::Bool(false)),
                ptr: m::local(format!("array.live.{}.{}", local.0, index)),
                align: 1,
            },
        )));
        for (field, teardown) in teardowns {
            let pointer = self.temp();
            let field = field
                .iter()
                .map(|index| m::Arg::new(m::Ty::Int(32), *index))
                .collect::<Vec<_>>();
            self.event(m::Event::Instruction(m::Inst::new(
                Some(m::name((pointer).clone())),
                m::Op::Gep {
                    ty: (array).clone(),
                    ptr: m::local(format!("local.{}", local.0)),
                    indices: {
                        let mut args = vec![
                            m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                            m::Arg::new(m::Ty::Int(32), m::val(index)),
                        ];
                        args.extend((field).clone());
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
                    args: vec![m::Arg::new(m::Ty::Ptr, m::val((pointer).clone()))],
                    vararg: None,
                },
            )));
        }
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Branch(format!("array.element.done.{}", label)),
        )));
        self.event(m::Event::Label(format!("array.element.done.{}", label)));
        Ok(())
    }
}
