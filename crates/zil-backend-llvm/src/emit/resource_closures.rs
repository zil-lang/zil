//! Inline owning environments. Only independently verified operations enter
//! these routines; neither a copied LIR nor a flag grants ownership authority.
use super::*;

pub(super) fn owns(definition: &ClosureDefinition) -> bool {
    definition.captures.iter().any(|ty| layout::capture_flag_count(ty)!=0)
}

pub(super) fn by_pointer(definition: &ClosureDefinition) -> bool {
    owns(definition)
        || !definition.lending_captures.is_empty()
        || matches!(definition.kind, CallableKind::FnMut | CallableKind::FnOnce)
}

pub(super) fn capture_drop_symbol(id:ClosureId,index:u32)->String{format!("zil.closure.capture.drop.{}.{index}",id.0)}

/// Field cleanup follows the fixed source capture order. The local containing
/// this environment has already surrendered its responsibility before entry.
pub(super) fn drop_functions(
    layouts: &LayoutContext<'_>,
    closures: &[ClosureDefinition],
    resources: &[ResourceDefinition],
) -> CompileResult<m::Code> {
    let mut text = m::Code::new();
    // Every concrete environment has an addressable field cleanup entry.
    // Resource-free callbacks simply return; this preserves unique field slots
    // without inventing a capture cleanup obligation.
    for definition in closures {
        layouts.reserve_environment_emission(definition.id,256)?;
        for (index,ty) in definition.captures.iter().enumerate() {
            if layout::capture_flag_count(ty)==0 {continue;}
        let env = environment_type(definition.id);
        text.append(m::Code::from(vec![
            m::Event::Begin(m::Function {
                name: capture_drop_symbol(definition.id,index as u32),
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

            let flag = layouts.flag_start(definition.id, index as u32)?;
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
            match enums::storage_shape(ty) {
                Type::OwnedDyn { interface } => {
                    text.push(m::Event::Instruction(m::Inst::new(
                        Some(format!("payload.{index}")),
                        m::Op::Gep {
                            ty: env.clone(),
                            ptr: m::local("env".into()),
                            indices: vec![
                                m::Arg::new(m::Ty::Int(32), 0),
                                m::Arg::new(m::Ty::Int(32), index as u64),
                            ],
                        },
                    )));
                    text.push(m::Event::Instruction(m::Inst::new(
                        None,
                        m::Op::Call {
                            result: m::Ty::Void,
                            result_attrs: vec![],
                            callee: m::global(dyn_dispatch::owner_drop_symbol(*interface)),
                            args: vec![m::Arg::new(
                                m::Ty::Ptr,
                                m::local(format!("payload.{index}")),
                            )],
                            vararg: None,
                        },
                    )));
                }
                Type::Managed(payload) => {
                    let teardown = if matches!(payload.as_ref(),Type::Mutex{..}|Type::AsyncRegistration{..}) {
                        Some(drop_plan::manager_symbol(enums::storage_shape(ty)))
                    } else if matches!(payload.as_ref(), Type::OwnedStore { .. }) {
                        Some(dynamic_store::helper(payload, resources, "drop")?)
                    } else {
                        let resource = resources
                            .iter()
                            .find(|resource| &resource.payload == payload.as_ref())
                            .expect("verified captured resource definition");
                        if matches!(resource.payload, Type::Registration { .. }) {
                            Some("zil.registration.drop".into())
                        } else if matches!(resource.payload, Type::TaskI32 | Type::Task { .. }) {
                            Some("zil.task.drop".into())
                        } else if resource.payload == Type::ByteBuffer {
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
                ty if drop_plan::owned_sum(ty) => {
                    text.append(m::Code::from(vec![
                        m::Event::Instruction(m::Inst::new(
                            Some(format!("sum.payload.{index}")),
                            m::Op::Gep {
                                ty: env.clone(),
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
                                callee: m::global(drop_plan::symbol(ty)),
                                args: vec![m::Arg::new(
                                    m::Ty::Ptr,
                                    m::local(format!("sum.payload.{index}")),
                                )],
                                vararg: None,
                            },
                        )),
                    ]));
                }
                Type::Package { fields, .. } => {
                    let mut offset = 1;
                    for (field, (_, ty)) in fields.iter().enumerate() {
                        if !packages::owned_field(ty) {
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
                text.append(m::Code::from(vec![
            m::Event::Instruction(m::Inst::new(None, m::Op::Return(None))),
            m::Event::End,
        ]));
            }
        text.push(m::Event::Begin(m::Function{name:format!("zil.closure.drop.{}",definition.id.0),linkage:m::Linkage::Internal,signature:m::Signature::new(m::Ty::Void,vec![m::Param::new(m::Ty::Ptr,"env")]),attrs:vec![],blocks:None}));
        text.push(m::Event::Label("entry".into()));
        for(index,ty)in definition.captures.iter().enumerate(){
            if layout::capture_flag_count(ty)!=0 {
                text.push(m::Event::Instruction(m::Inst::new(None,m::Op::Call{result:m::Ty::Void,result_attrs:vec![],callee:m::global(capture_drop_symbol(definition.id,index as u32)),args:vec![m::Arg::new(m::Ty::Ptr,m::local("env".into()))],vararg:None})));
            }
        }
        text.push(m::Event::Instruction(m::Inst::new(None,m::Op::Return(None))));text.push(m::Event::End);
}
    Ok(text)
}

impl FunctionEmitter<'_> {
    pub(super) fn refill_capture_flags(
        &mut self,
        environment: ClosureId,
        index: u32,
    ) -> CompileResult<()> {
        let recipe=self.layouts.environment(environment)?;
        self.layouts.meter().spend(recipe.fields.len())?;
        let flags=recipe.fields.iter().filter_map(|field|match field.role {
            layout::FieldRole::CaptureFlag{capture,..}if capture==index=>Some(field.index),_=>None
        }).collect::<Vec<_>>();
        for flag in flags {
            self.layouts.meter().spend(16)?;let pointer=self.temp();
            self.event(m::Event::Instruction(m::Inst::new(Some(m::name(pointer.clone())),m::Op::Gep{ty:environment_type(environment),ptr:m::local("env".into()),indices:vec![m::Arg::new(m::Ty::Int(32),0),m::Arg::new(m::Ty::Int(32),flag)]})));
            self.event(m::Event::Instruction(m::Inst::new(None,m::Op::Store{arg:m::Arg::new(m::Ty::Int(1),true),ptr:m::val(pointer),align:1})));
        }
        Ok(())
    }

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
        let flag = self.layouts.flag_start(definition.id, index)?;
        let Some(Type::Package { fields, .. }) =
            packages::physical_package(&definition.captures[index as usize])
        else {
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
                packages::owned_field(ty).then_some(field)
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
            Type::Managed(_)
            | Type::Package { .. }
            | Type::MutexGuard { .. }
            | Type::OwnedDyn { .. } => true,
            Type::BorrowedStorage { storage,.. }=> {
                packages::physical_package(storage).is_some()||drop_plan::scoped_debt(storage)
            }
            Type::ErrorUnion { .. } => errors::owned(ty),
            Type::Enum { .. } | Type::Tuple(_) => !ty.is_copy_value(),
            Type::Record{..}=>drop_plan::scoped_debt(ty),
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
        // An affine borrowed pointer has no destructor flag. Its unique Take
        // is proved before code generation; only owning payloads carry flags.
        if layout::capture_flag_count(&ty) > 0 {
            let flag = self.layouts.flag_start(definition.id, index)?;
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
        }
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
                ty: (self.layouts.value_type(&ty)?).clone(),
                ptr: m::val((address).clone()),
                align: (self.layouts.alignment(&ty)?) as u32,
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
        let group = if self.task_ownership.is_some() {
            tasks::cleanup_group(self.dyn_module, &Type::Closure(environment), self.machine_meter)?
        } else { None };
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
        if let Some(group) = group {
            // Keep the actual root flag/node until every live capture is done.
            self.event(m::Event::Instruction(m::Inst::new(None, m::Op::Call {
                result: m::Ty::Void, result_attrs: vec![],
                callee: m::global(tasks::group_marker(group)),
                args: vec![m::Arg::new(m::Ty::Ptr,m::local(format!("local.{}",local.0)))],
                vararg: None,
            })));
        }
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Store {
                arg: m::Arg::new(m::Ty::Int(1), m::Val::Bool(false)),
                ptr: m::local(format!("resource.live.{}", local.0)),
                align: 1,
            },
        )));
        if has_owned && group.is_none() {
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
                    align: (self.layouts.alignment(&Type::Closure(environment))?) as u32,
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
                    align: (self.layouts.alignment(&Type::Closure(environment))?) as u32,
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
        let cleanup = self.current_instruction.and_then(|(block, index)| {
            self.task_ownership.and_then(|plan| plan.actions_at(block, index))
        }).filter(|actions| actions.invocation_owner().is_some());
        let group = if cleanup.is_some() {
            Some(tasks::cleanup_group(self.dyn_module, &Type::Closure(environment), self.machine_meter)?
                .ok_or_else(|| vec![Diagnostic::error(Phase::Codegen, "E-CODEGEN-TASK", Some(instruction.span),
                    "verified consuming cleanup has no actual environment group")])?)
        } else { None };
        self.event(m::Event::Instruction(m::Inst::new(
            None,
            m::Op::Store {
                arg: m::Arg::new(
                    (environment_type(environment)).clone(),
                    m::val((value(callee)).clone()),
                ),
                ptr: m::local(format!("closure.once.{}", callee.0)),
                align: (self.layouts.alignment(&Type::Closure(environment))?) as u32,
            },
        )));
        if let Some(owner) = cleanup.and_then(|actions| actions.invocation_owner()) {
            // The body mutates this real spill's capture flags. Keep the same
            // affine node, pointing at those flags rather than the old copy.
            self.event(tasks::retarget_marker(owner,
                m::local(format!("closure.once.{}", callee.0))));
        }
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
        if let Some(actions) = cleanup {
            for &action in actions.after_invoke() {
                self.event(tasks::owner_marker(action));
            }
        }
        if let Some(group) = group {
            self.event(m::Event::Instruction(m::Inst::new(None, m::Op::Call {
                result: m::Ty::Void, result_attrs: vec![],
                callee: m::global(tasks::group_marker(group)),
                args: vec![m::Arg::new(m::Ty::Ptr, m::local(format!("closure.once.{}", callee.0)))],
                vararg: None,
            })));
        } else if has_owned {
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

    pub(super) fn emit_resource_closure_call(
        &mut self,
        instruction: &Instruction,
        function: FunctionId,
        result_ty: &Type,
        environment: m::Arg,
        arguments: &[ValueId],
    ) -> CompileResult<()> {
        let signature = &self.abi_plans.functions[&function];
        if signature.large() {
            let signature = signature.clone();
            return self.large_copy_call_with_prefix(
                instruction,
                &signature,
                m::global(self.abi_symbol(function)),
                vec![environment],
                arguments,
            );
        }
        let assignment = if *result_ty == Type::Unit {
            None
        } else {
            Some(format!("v.{}", result(instruction)?.0))
        };
        let mut emitted_arguments = vec![environment];
        for argument in arguments {
            let ty = self.value_type(*argument)?;
            if *ty != Type::Unit {
                emitted_arguments.push(m::Arg::new(self.layouts.value_type(ty)?, m::val(value(*argument))));
            }
        }
        self.event(m::Event::Instruction(m::Inst::new(
            (assignment).clone(),
            m::Op::Call {
                result: (self.layouts.value_type(result_ty)?).clone(),
                result_attrs: vec![],
                callee: m::global(self.abi_symbol(function)),
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

#[cfg(test)]
mod capture_cleanup_tests {
    use super::*;

    #[test]
    fn callable_package_capture_emits_guarded_child_cleanup() {
        let interface = zil_ir::DynInterfaceId { module: zil_base::ModuleId(0), index: 9 };
        let package = Type::Package {
            name: "Callbacks".into(),
            fields: vec![
                ("callback".into(), Type::Closure(ClosureId(7))),
                ("dynamic".into(), Type::OwnedDyn { interface }),
                ("count".into(), Type::I32),
            ],
        };
        let limits = zil_base::Limits::default();
        let mut work = limits.max_analysis_steps;
        assert!(packages::capture_admission(&package, &[], &limits, &mut work).unwrap().0);
        assert_eq!(layout::capture_flag_count(&package), 3, "shell and both independently movable children");
        let definition = ClosureDefinition {
            storage_life: None, id: ClosureId(0), function: FunctionId(0),
            captures: vec![package.clone()], parameters: vec![], result: Type::Unit,
            kind: CallableKind::FnOnce, lending_captures: vec![],
        };
        let mut captures=vec![vec![];8];captures[0]=definition.captures.clone();
        captures[7]=vec![Type::Managed(Box::new(Type::ByteBuffer))];
        let mut directory=layout::test_environments(captures);
        directory.resources=vec![ResourceDefinition{name:zil_ir::BYTE_BUFFER_RESOURCE_NAME.into(),payload:Type::ByteBuffer,teardown:None}];
        let layouts=&LayoutContext::for_test_module(&directory,&limits).unwrap();
        let code = drop_functions(layouts,&directory.closures,&directory.resources).unwrap();
        // Inspect the actual shared machine stream, not just the helper plan:
        // each owning child must have its own live branch and exactly one call.
        for (field, symbol) in [(0, "zil.closure.drop.7".to_string()),
            (1, dyn_dispatch::owner_drop_symbol(interface))] {
            let drop_label = format!("field.drop.0.{field}");
            assert!(code.0.iter().any(|event| matches!(event,
                m::Event::Instruction(m::Inst { op: m::Op::CondBranch { yes, .. }, .. })
                if yes == &drop_label)));
            let begin = code.0.iter().position(|event|
                matches!(event, m::Event::Label(label) if label == &drop_label)).unwrap();
            let end = code.0.iter().position(|event|
                matches!(event, m::Event::Label(label) if label == &format!("field.done.0.{field}"))).unwrap();
            let child = &code.0[begin..end];
            assert!(child.iter().any(|event| matches!(event,
                m::Event::Instruction(m::Inst { op: m::Op::Store { arg, .. }, .. })
                if arg.value == m::Val::Bool(false))));
            assert_eq!(child.iter().filter(|event| matches!(event,
                m::Event::Instruction(m::Inst { op: m::Op::Call { callee, .. }, .. })
                if *callee == m::global(symbol.clone()))).count(), 1);
        }
        // Complete physical Record/Array compositions now use the same owner
        // recipe; this component check supplies no language capability.
        let Type::Package { name, fields } = package.clone() else { unreachable!() };
        let record = Type::Record { name, fields };
        assert!(packages::capture_admission(&record, &[], &limits, &mut work).unwrap().0);
        assert!(drop_plan::value_type(&record, &[]));
        let array = Type::Array { element: Box::new(package), length: 2 };
        assert!(drop_plan::value_type(&array, &[]),"complete callable arrays retain their opaque owner cleanup recipe");
    }
}
