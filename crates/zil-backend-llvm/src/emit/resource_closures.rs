//! Inline owning environments. Only independently verified operations enter
//! these routines; neither a copied LIR nor a flag grants ownership authority.
use super::*;

pub(super) fn owns(definition: &ClosureDefinition) -> bool {
    definition.captures.iter().any(|ty| {
        matches!(
            ty,
            Type::Managed(_) | Type::Package { .. } | Type::Closure(_)
        )
    })
}

pub(super) fn by_pointer(definition: &ClosureDefinition) -> bool {
    owns(definition)
        || !definition.lending_captures.is_empty()
        || matches!(definition.kind, CallableKind::FnMut | CallableKind::FnOnce)
}

/// Flags are appended after all data fields, grouped by capture order.
/// Package groups are shell followed by direct Managed/complete child fields.
pub(super) fn flag_count(ty: &Type) -> usize {
    match ty {
        Type::Managed(_) | Type::Closure(_) => 1,
        Type::Package { fields, .. } => {
            1 + fields
                .iter()
                .filter(|(_, ty)| matches!(ty, Type::Managed(_) | Type::Package { .. }))
                .count()
        }
        _ => 0,
    }
}

fn flag_index(definition: &ClosureDefinition, capture: u32) -> usize {
    definition.captures.len()
        + definition.captures[..capture as usize]
            .iter()
            .map(flag_count)
            .sum::<usize>()
}

/// Field cleanup follows the fixed source capture order. The local containing
/// this environment has already surrendered its responsibility before entry.
pub(super) fn drop_functions(
    closures: &[ClosureDefinition],
    resources: &[ResourceDefinition],
) -> CompileResult<m::Code> {
    let mut text = m::Code::new();
    // Every concrete environment has an addressable field cleanup entry.
    // Resource-free callbacks simply return; this preserves unique field slots
    // without inventing a capture cleanup obligation.
    for definition in closures {
        let env = environment_type(definition.id);
        text.append(m::Code::from(vec![
            m::Event::Begin(m::Function {
                name: format!("zil.closure.drop.{}", definition.id.0),
                linkage: m::Linkage::Internal,
                signature: m::Signature {
                    result: m::Ty::Void,
                    result_attrs: vec![],
                    params: vec![m::Param {
                        ty: m::Ty::Ptr,
                        name: Some("env".into()),
                        attrs: vec![],
                    }],
                    variadic: false,
                },
                attrs: vec![],
                blocks: None,
            }),
            m::Event::Label("entry".into()),
        ]));
        for (index, ty) in definition.captures.iter().enumerate() {
            if flag_count(ty) == 0 {
                continue;
            }
            let flag = flag_index(definition, index as u32);
            text.append(m::Code::from(vec![
                m::Event::Instruction(m::Inst::new(
                    Some(format!("flag.ptr.{}", index)),
                    m::Op::Gep {
                        ty: (env).clone(),
                        ptr: m::local("env".into()),
                        indices: vec![
                            m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                            m::Arg::new(m::Ty::Int(32), m::val(flag)),
                        ],
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    Some(format!("live.{}", index)),
                    m::Op::Load {
                        ty: m::Ty::Int(1),
                        ptr: m::local(format!("flag.ptr.{}", index)),
                        align: 1,
                    },
                )),
                m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::CondBranch {
                        condition: m::local(format!("live.{}", index)),
                        yes: format!("drop.{}", index),
                        no: format!("done.{}", index),
                    },
                )),
                m::Event::Label(format!("drop.{}", index)),
                m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Store {
                        arg: m::Arg::new(m::Ty::Int(1), m::Val::Bool(false)),
                        ptr: m::local(format!("flag.ptr.{}", index)),
                        align: 1,
                    },
                )),
            ]));
            match ty {
                Type::Managed(payload) => {
                    let teardown = if matches!(payload.as_ref(), Type::OwnedStore { .. }) {
                        Some(dynamic_store::helper(payload, resources, "drop")?)
                    } else {
                        let resource = resources
                            .iter()
                            .find(|resource| &resource.payload == payload.as_ref())
                            .expect("verified captured resource definition");
                        if resource.payload == Type::ByteBuffer {
                            Some("zil.bytes.drop".into())
                        } else {
                            resource.teardown.map(symbol)
                        }
                    };
                    if let Some(teardown) = teardown {
                        text.append(m::Code::from(vec![
                            m::Event::Instruction(m::Inst::new(
                                Some(format!("payload.{}", index)),
                                m::Op::Gep {
                                    ty: (env).clone(),
                                    ptr: m::local("env".into()),
                                    indices: vec![
                                        m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                                        m::Arg::new(m::Ty::Int(32), m::val(index)),
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
                                        m::local(format!("payload.{}", index)),
                                    )],
                                    vararg: None,
                                },
                            )),
                        ]));
                    }
                }
                Type::Closure(child) => {
                    if owns(&closures[child.0 as usize]) {
                        text.append(m::Code::from(vec![
                            m::Event::Instruction(m::Inst::new(
                                Some(format!("child.{}", index)),
                                m::Op::Gep {
                                    ty: (env).clone(),
                                    ptr: m::local("env".into()),
                                    indices: vec![
                                        m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                                        m::Arg::new(m::Ty::Int(32), m::val(index)),
                                    ],
                                },
                            )),
                            m::Event::Instruction(m::Inst::new(
                                None,
                                m::Op::Call {
                                    result: m::Ty::Void,
                                    result_attrs: vec![],
                                    callee: m::global(format!("zil.closure.drop.{}", child.0)),
                                    args: vec![m::Arg::new(
                                        m::Ty::Ptr,
                                        m::local(format!("child.{}", index)),
                                    )],
                                    vararg: None,
                                },
                            )),
                        ]));
                    }
                }
                Type::Package { fields, .. } => {
                    let mut offset = 1;
                    for (field, (_, ty)) in fields.iter().enumerate() {
                        if !matches!(ty, Type::Managed(_) | Type::Package { .. }) {
                            continue;
                        }
                        let field_flag = flag + offset;
                        offset += 1;
                        text.append(m::Code::from(vec![
                            m::Event::Instruction(m::Inst::new(
                                Some(format!("field.flag.{}.{}", index, field)),
                                m::Op::Gep {
                                    ty: (env).clone(),
                                    ptr: m::local("env".into()),
                                    indices: vec![
                                        m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                                        m::Arg::new(m::Ty::Int(32), m::val(field_flag)),
                                    ],
                                },
                            )),
                            m::Event::Instruction(m::Inst::new(
                                Some(format!("field.live.{}.{}", index, field)),
                                m::Op::Load {
                                    ty: m::Ty::Int(1),
                                    ptr: m::local(format!("field.flag.{}.{}", index, field)),
                                    align: 1,
                                },
                            )),
                            m::Event::Instruction(m::Inst::new(
                                None,
                                m::Op::CondBranch {
                                    condition: m::local(format!("field.live.{}.{}", index, field)),
                                    yes: format!("field.drop.{}.{}", index, field),
                                    no: format!("field.done.{}.{}", index, field),
                                },
                            )),
                            m::Event::Label(format!("field.drop.{}.{}", index, field)),
                            m::Event::Instruction(m::Inst::new(
                                None,
                                m::Op::Store {
                                    arg: m::Arg::new(m::Ty::Int(1), m::Val::Bool(false)),
                                    ptr: m::local(format!("field.flag.{}.{}", index, field)),
                                    align: 1,
                                },
                            )),
                        ]));
                        // Discharge the direct child flag before traversing its
                        // complete inline leaves in declaration order. Machine
                        // events serve both direct Builder and text reference.
                        for (leaf, (path, teardown)) in packages::teardown_paths(ty, resources)?
                            .into_iter()
                            .enumerate()
                        {
                            let pointer = if path.is_empty() {
                                format!("payload.{index}.{field}")
                            } else {
                                format!("payload.{index}.{field}.{leaf}")
                            };
                            let mut indices = vec![
                                m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                                m::Arg::new(m::Ty::Int(32), m::val(index)),
                                m::Arg::new(m::Ty::Int(32), m::val(field)),
                            ];
                            indices
                                .extend(path.into_iter().map(|p| m::Arg::new(m::Ty::Int(32), p)));
                            text.append(m::Code::from(vec![
                                m::Event::Instruction(m::Inst::new(
                                    Some(pointer.clone()),
                                    m::Op::Gep {
                                        ty: env.clone(),
                                        ptr: m::local("env".into()),
                                        indices,
                                    },
                                )),
                                m::Event::Instruction(m::Inst::new(
                                    None,
                                    m::Op::Call {
                                        result: m::Ty::Void,
                                        result_attrs: vec![],
                                        callee: m::global(teardown),
                                        args: vec![m::Arg::new(m::Ty::Ptr, m::local(pointer))],
                                        vararg: None,
                                    },
                                )),
                            ]));
                        }
                        text.append(m::Code::from(vec![
                            m::Event::Instruction(m::Inst::new(
                                None,
                                m::Op::Branch(format!("field.done.{}.{}", index, field)),
                            )),
                            m::Event::Label(format!("field.done.{}.{}", index, field)),
                        ]));
                    }
                }
                _ => unreachable!("verified owned capture"),
            }
            text.append(m::Code::from(vec![
                m::Event::Instruction(m::Inst::new(None, m::Op::Branch(format!("done.{}", index)))),
                m::Event::Label(format!("done.{}", index)),
            ]));
        }
        text.append(m::Code::from(vec![
            m::Event::Instruction(m::Inst::new(None, m::Op::Return(None))),
            m::Event::End,
        ]));
    }
    Ok(text)
}

impl FunctionEmitter<'_> {
    pub(super) fn borrow_capture(
        &mut self,
        instruction: &Instruction,
        index: u32,
    ) -> CompileResult<()> {
        let environment = self.function.environment.expect("verified environment");
        let env = environment_type(environment);
        let result = instruction.result.expect("verified capture borrow");
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((value(result)).clone())),
            m::Op::Gep {
                ty: (env).clone(),
                ptr: m::local("env".into()),
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                    m::Arg::new(m::Ty::Int(32), m::val(index)),
                ],
            },
        )));
        Ok(())
    }

    /// Establish pointer names once in the machine entry block. Lexical
    /// Bind/End do not copy the package or initialize/clear its physical flags.
    pub(super) fn package_capture_storage(
        &mut self,
        local: LocalId,
        index: u32,
    ) -> CompileResult<()> {
        let environment = self
            .function
            .environment
            .expect("verified Package environment");
        let definition = self.closure_definition(environment)?;
        let flag = flag_index(definition, index);
        let Type::Package { fields, .. } = &definition.captures[index as usize] else {
            return invalid(
                "E-CODEGEN-CFG",
                "Package view has no matching capture",
                Some(self.function.span),
            );
        };
        let owned = fields
            .iter()
            .enumerate()
            .filter_map(|(field, (_, ty))| {
                matches!(ty, Type::Managed(_) | Type::Package { .. }).then_some(field)
            })
            .collect::<Vec<_>>();
        let env = environment_type(environment);
        self.event(m::Event::Instruction(m::Inst::new(
            Some(format!("local.{}", local.0)),
            m::Op::Gep {
                ty: (env).clone(),
                ptr: m::local("env".into()),
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                    m::Arg::new(m::Ty::Int(32), m::val(index)),
                ],
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(format!("resource.live.{}", local.0)),
            m::Op::Gep {
                ty: (env).clone(),
                ptr: m::local("env".into()),
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                    m::Arg::new(m::Ty::Int(32), m::val(flag)),
                ],
            },
        )));
        for (offset, field) in owned.into_iter().enumerate() {
            self.event(m::Event::Instruction(m::Inst::new(
                Some(format!("package.live.{}.{}", local.0, field)),
                m::Op::Gep {
                    ty: (env).clone(),
                    ptr: m::local("env".into()),
                    indices: vec![
                        m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                        m::Arg::new(m::Ty::Int(32), m::val(flag + 1 + offset)),
                    ],
                },
            )));
        }
        Ok(())
    }

    pub(super) fn owns_resource(&self, ty: &Type) -> bool {
        match ty {
            Type::Managed(_) | Type::Package { .. } => true,
            Type::ErrorUnion { .. } => errors::owned(ty),
            Type::Closure(id) => self.closures.get(id).is_some_and(|definition| {
                owns(definition) || definition.kind == CallableKind::FnOnce
            }),
            _ => false,
        }
    }

    pub(super) fn describe_capture(
        &mut self,
        instruction: &Instruction,
        index: u32,
    ) -> CompileResult<()> {
        let environment = self
            .function
            .environment
            .expect("verified active environment");
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((value(result(instruction)?)).clone())),
            m::Op::Gep {
                ty: (environment_type(environment)).clone(),
                ptr: m::local("env".into()),
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                    m::Arg::new(m::Ty::Int(32), m::val(index)),
                ],
            },
        )));
        Ok(())
    }

    pub(super) fn take_capture(
        &mut self,
        instruction: &Instruction,
        index: u32,
    ) -> CompileResult<()> {
        let environment = self
            .function
            .environment
            .expect("verified active environment");
        let definition = self.closure_definition(environment)?;
        let ty = definition.captures[index as usize].clone();
        let flag = flag_index(definition, index);
        let flag_ptr = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((flag_ptr).clone())),
            m::Op::Gep {
                ty: (environment_type(environment)).clone(),
                ptr: m::local("env".into()),
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                    m::Arg::new(m::Ty::Int(32), m::val(flag)),
                ],
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Store {
                arg: m::Arg::new(m::Ty::Int(1), m::Val::Bool(false)),
                ptr: m::val((flag_ptr).clone()),
                align: 1,
            },
        )));
        let address = self.temp();
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((address).clone())),
            m::Op::Gep {
                ty: (environment_type(environment)).clone(),
                ptr: m::local("env".into()),
                indices: vec![
                    m::Arg::new(m::Ty::Int(32), m::Val::Int(0)),
                    m::Arg::new(m::Ty::Int(32), m::val(index)),
                ],
            },
        )));
        self.event(m::Event::Instruction(m::Inst::new(
            Some(m::name((value(result(instruction)?)).clone())),
            m::Op::Load {
                ty: (llvm_type(&ty)?).clone(),
                ptr: m::val((address).clone()),
                align: (alignment(&ty, self.closures)?) as u32,
            },
        )));
        Ok(())
    }

    pub(super) fn drop_closure(
        &mut self,
        instruction: &Instruction,
        local: LocalId,
    ) -> CompileResult<()> {
        let Type::Closure(environment) = self.local_type(local)? else {
            return invalid(
                "E-CODEGEN-CFG",
                "closure cleanup requires an actual closure local",
                Some(instruction.span),
            );
        };
        let environment = *environment;
        let definition = self.closure_definition(environment)?;
        let has_owned = owns(definition);
        if !has_owned && definition.kind != CallableKind::FnOnce {
            return Ok(());
        }
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
                yes: format!("closure.drop.{}", label),
                no: format!("closure.done.{}", label),
            },
        )));
        self.event(m::Event::Label(format!("closure.drop.{}", label)));
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Store {
                arg: m::Arg::new(m::Ty::Int(1), m::Val::Bool(false)),
                ptr: m::local(format!("resource.live.{}", local.0)),
                align: 1,
            },
        )));
        if has_owned {
            self.event(m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Call {
                    result: m::Ty::Void,
                    result_attrs: vec![],
                    callee: m::global(format!("zil.closure.drop.{}", environment.0)),
                    args: vec![m::Arg::new(
                        m::Ty::Ptr,
                        m::local(format!("local.{}", local.0)),
                    )],
                    vararg: None,
                },
            )));
        }
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Branch(format!("closure.done.{}", label)),
        )));
        self.event(m::Event::Label(format!("closure.done.{}", label)));
        Ok(())
    }

    pub(super) fn call_closure_shared(
        &mut self,
        instruction: &Instruction,
        local: LocalId,
        arguments: &[ValueId],
    ) -> CompileResult<()> {
        let Type::Closure(environment) = self.local_type(local)? else {
            return invalid(
                "E-CODEGEN-CFG",
                "shared closure call requires an actual local",
                Some(instruction.span),
            );
        };
        let environment = *environment;
        let definition = self.closure_definition(environment)?;
        let function = definition.function;
        let result_ty = definition.result.clone();
        let pointer = by_pointer(definition);
        let env_argument = if pointer {
            m::Arg::new(m::Ty::Ptr, m::local(format!("local.{}", local.0)))
        } else {
            let loaded = self.temp();
            self.event(m::Event::Instruction(m::Inst::new(
                Some(m::name((loaded).clone())),
                m::Op::Load {
                    ty: (environment_type(environment)).clone(),
                    ptr: m::local(format!("local.{}", local.0)),
                    align: (alignment(&Type::Closure(environment), self.closures)?) as u32,
                },
            )));
            m::Arg::new(environment_type(environment), m::val(loaded))
        };
        self.emit_resource_closure_call(instruction, function, &result_ty, env_argument, arguments)
    }

    pub(super) fn call_closure_borrowed(
        &mut self,
        instruction: &Instruction,
        callee: ValueId,
        arguments: &[ValueId],
    ) -> CompileResult<()> {
        let Type::Address { inner, .. } = self.value_type(callee)? else {
            return invalid(
                "E-CODEGEN-CFG",
                "borrowed callable requires its real address",
                Some(instruction.span),
            );
        };
        let Type::Closure(environment) = inner.as_ref() else {
            return invalid(
                "E-CODEGEN-CFG",
                "borrowed callable has no concrete environment",
                Some(instruction.span),
            );
        };
        let environment = *environment;
        let definition = self.closure_definition(environment)?;
        let function = definition.function;
        let result_ty = definition.result.clone();
        let env_argument = if by_pointer(definition) {
            m::Arg::new(m::Ty::Ptr, m::val(value(callee)))
        } else {
            // Only the existing non-owning Shared ABI copies its read-only
            // fields. Owning/FnMut/FnOnce environments always stay in place.
            let loaded = self.temp();
            self.event(m::Event::Instruction(m::Inst::new(
                Some(m::name((loaded).clone())),
                m::Op::Load {
                    ty: (environment_type(environment)).clone(),
                    ptr: m::val((value(callee)).clone()),
                    align: (alignment(&Type::Closure(environment), self.closures)?) as u32,
                },
            )));
            m::Arg::new(environment_type(environment), m::val(loaded))
        };
        self.emit_resource_closure_call(instruction, function, &result_ty, env_argument, arguments)
    }

    pub(super) fn call_closure_once(
        &mut self,
        instruction: &Instruction,
        callee: ValueId,
        arguments: &[ValueId],
    ) -> CompileResult<()> {
        let Type::Closure(environment) = self.value_type(callee)? else {
            return invalid(
                "E-CODEGEN-CFG",
                "consuming closure call requires an environment value",
                Some(instruction.span),
            );
        };
        let environment = *environment;
        let definition = self.closure_definition(environment)?;
        let function = definition.function;
        let result_ty = definition.result.clone();
        let has_owned = owns(definition);
        let pointer = by_pointer(definition);
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Store {
                arg: m::Arg::new(
                    (environment_type(environment)).clone(),
                    m::val((value(callee)).clone()),
                ),
                ptr: m::local(format!("closure.once.{}", callee.0)),
                align: (alignment(&Type::Closure(environment), self.closures)?) as u32,
            },
        )));
        self.emit_resource_closure_call(
            instruction,
            function,
            &result_ty,
            if pointer {
                m::Arg::new(m::Ty::Ptr, m::local(format!("closure.once.{}", callee.0)))
            } else {
                m::Arg::new(environment_type(environment), m::val(value(callee)))
            },
            arguments,
        )?;
        if has_owned {
            self.event(m::Event::Instruction(m::Inst::new(
                None,
                m::Op::Call {
                    result: m::Ty::Void,
                    result_attrs: vec![],
                    callee: m::global(format!("zil.closure.drop.{}", environment.0)),
                    args: vec![m::Arg::new(
                        m::Ty::Ptr,
                        m::local(format!("closure.once.{}", callee.0)),
                    )],
                    vararg: None,
                },
            )));
        }
        Ok(())
    }

    fn emit_resource_closure_call(
        &mut self,
        instruction: &Instruction,
        function: FunctionId,
        result_ty: &Type,
        environment: m::Arg,
        arguments: &[ValueId],
    ) -> CompileResult<()> {
        let assignment = if *result_ty == Type::Unit {
            None
        } else {
            Some(format!("v.{}", result(instruction)?.0))
        };
        let mut emitted_arguments = vec![environment];
        for argument in arguments {
            let ty = self.value_type(*argument)?;
            if *ty != Type::Unit {
                emitted_arguments.push(m::Arg::new(llvm_type(ty)?, m::val(value(*argument))));
            }
        }
        self.event(m::Event::Instruction(m::Inst::new(
            (assignment).clone(),
            m::Op::Call {
                result: (llvm_type(result_ty)?).clone(),
                result_attrs: vec![],
                callee: m::global(symbol(function).to_string()),
                args: {
                    let mut args = vec![];
                    args.extend((emitted_arguments.clone()).clone());
                    args
                },
                vararg: None,
            },
        )));
        Ok(())
    }
}
