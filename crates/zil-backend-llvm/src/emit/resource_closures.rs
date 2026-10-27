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
    owns(definition) || matches!(definition.kind, CallableKind::FnMut | CallableKind::FnOnce)
}

/// Flags are appended after all data fields, grouped by capture order.
/// Package groups are shell followed by direct Managed fields in field order.
pub(super) fn flag_count(ty: &Type) -> usize {
    match ty {
        Type::Managed(_) | Type::Closure(_) => 1,
        Type::Package { fields, .. } => {
            1 + fields
                .iter()
                .filter(|(_, ty)| matches!(ty, Type::Managed(_)))
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
) -> CompileResult<String> {
    let mut text = String::new();
    for definition in closures.iter().filter(|definition| owns(definition)) {
        let env = environment_type(definition.id);
        text.push_str(&format!(
            "define internal void @zil.closure.drop.{}(ptr %env) {{\nentry:\n",
            definition.id.0
        ));
        for (index, ty) in definition.captures.iter().enumerate() {
            if flag_count(ty) == 0 {
                continue;
            }
            let flag = flag_index(definition, index as u32);
            text.push_str(&format!(
                "  %flag.ptr.{index} = getelementptr {env}, ptr %env, i32 0, i32 {flag}\n  %live.{index} = load i1, ptr %flag.ptr.{index}, align 1\n  br i1 %live.{index}, label %drop.{index}, label %done.{index}\ndrop.{index}:\n  store i1 false, ptr %flag.ptr.{index}, align 1\n"
            ));
            match ty {
                Type::Managed(payload) => {
                    let resource = resources
                        .iter()
                        .find(|resource| &resource.payload == payload.as_ref())
                        .expect("verified captured resource definition");
                    let teardown = if resource.payload == Type::ByteBuffer {
                        Some("zil.bytes.drop".into())
                    } else {
                        resource.teardown.map(symbol)
                    };
                    if let Some(teardown) = teardown {
                        text.push_str(&format!(
                            "  %payload.{index} = getelementptr {env}, ptr %env, i32 0, i32 {index}\n  call void @{teardown}(ptr %payload.{index})\n"
                        ));
                    }
                }
                Type::Closure(child) => {
                    if owns(&closures[child.0 as usize]) {
                        text.push_str(&format!(
                            "  %child.{index} = getelementptr {env}, ptr %env, i32 0, i32 {index}\n  call void @zil.closure.drop.{}(ptr %child.{index})\n", child.0
                        ));
                    }
                }
                Type::Package { fields, .. } => {
                    let mut offset = 1;
                    for (field, (_, ty)) in fields.iter().enumerate() {
                        let Type::Managed(payload) = ty else { continue };
                        let resource = resources
                            .iter()
                            .find(|resource| &resource.payload == payload.as_ref())
                            .expect("verified captured package resource definition");
                        let field_flag = flag + offset;
                        offset += 1;
                        text.push_str(&format!(
                            "  %field.flag.{index}.{field} = getelementptr {env}, ptr %env, i32 0, i32 {field_flag}\n  %field.live.{index}.{field} = load i1, ptr %field.flag.{index}.{field}, align 1\n  br i1 %field.live.{index}.{field}, label %field.drop.{index}.{field}, label %field.done.{index}.{field}\nfield.drop.{index}.{field}:\n  store i1 false, ptr %field.flag.{index}.{field}, align 1\n"
                        ));
                        let teardown = if resource.payload == Type::ByteBuffer {
                            Some("zil.bytes.drop".into())
                        } else {
                            resource.teardown.map(symbol)
                        };
                        if let Some(teardown) = teardown {
                            text.push_str(&format!(
                                "  %payload.{index}.{field} = getelementptr {env}, ptr %env, i32 0, i32 {index}, i32 {field}\n  call void @{teardown}(ptr %payload.{index}.{field})\n"
                            ));
                        }
                        text.push_str(&format!(
                            "  br label %field.done.{index}.{field}\nfield.done.{index}.{field}:\n"
                        ));
                    }
                }
                _ => unreachable!("verified owned capture"),
            }
            text.push_str(&format!("  br label %done.{index}\ndone.{index}:\n"));
        }
        text.push_str("  ret void\n}\n");
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
        self.line(format!(
            "  {} = getelementptr {env}, ptr %env, i32 0, i32 {index}",
            value(result)
        ));
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
            .filter_map(|(field, (_, ty))| matches!(ty, Type::Managed(_)).then_some(field))
            .collect::<Vec<_>>();
        let env = environment_type(environment);
        self.line(format!(
            "  %local.{} = getelementptr {env}, ptr %env, i32 0, i32 {index}",
            local.0
        ));
        self.line(format!(
            "  %resource.live.{} = getelementptr {env}, ptr %env, i32 0, i32 {flag}",
            local.0
        ));
        for (offset, field) in owned.into_iter().enumerate() {
            self.line(format!(
                "  %package.live.{}.{field} = getelementptr {env}, ptr %env, i32 0, i32 {}",
                local.0,
                flag + 1 + offset
            ));
        }
        Ok(())
    }

    pub(super) fn owns_resource(&self, ty: &Type) -> bool {
        match ty {
            Type::Managed(_) | Type::Package { .. } => true,
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
        self.line(format!(
            "  {} = getelementptr {}, ptr %env, i32 0, i32 {index}",
            value(result(instruction)?),
            environment_type(environment)
        ));
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
        self.line(format!(
            "  {flag_ptr} = getelementptr {}, ptr %env, i32 0, i32 {flag}",
            environment_type(environment)
        ));
        self.line(format!("  store i1 false, ptr {flag_ptr}, align 1"));
        let address = self.temp();
        self.line(format!(
            "  {address} = getelementptr {}, ptr %env, i32 0, i32 {index}",
            environment_type(environment)
        ));
        self.line(format!(
            "  {} = load {}, ptr {address}, align {}",
            value(result(instruction)?),
            llvm_type(&ty)?,
            alignment(&ty, self.closures)?
        ));
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
        self.line(format!(
            "  {flag} = load i1, ptr %resource.live.{}, align 1",
            local.0
        ));
        self.line(format!(
            "  br i1 {flag}, label %closure.drop.{label}, label %closure.done.{label}"
        ));
        self.line(format!("closure.drop.{label}:"));
        self.line(format!(
            "  store i1 false, ptr %resource.live.{}, align 1",
            local.0
        ));
        if has_owned {
            self.line(format!(
                "  call void @zil.closure.drop.{}(ptr %local.{})",
                environment.0, local.0
            ));
        }
        self.line(format!("  br label %closure.done.{label}"));
        self.line(format!("closure.done.{label}:"));
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
            format!("ptr %local.{}", local.0)
        } else {
            let loaded = self.temp();
            self.line(format!(
                "  {loaded} = load {}, ptr %local.{}, align {}",
                environment_type(environment),
                local.0,
                alignment(&Type::Closure(environment), self.closures)?
            ));
            format!("{} {loaded}", environment_type(environment))
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
            format!("ptr {}", value(callee))
        } else {
            // Only the existing non-owning Shared ABI copies its read-only
            // fields. Owning/FnMut/FnOnce environments always stay in place.
            let loaded = self.temp();
            self.line(format!(
                "  {loaded} = load {}, ptr {}, align {}",
                environment_type(environment),
                value(callee),
                alignment(&Type::Closure(environment), self.closures)?
            ));
            format!("{} {loaded}", environment_type(environment))
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
        self.line(format!(
            "  store {} {}, ptr %closure.once.{}, align {}",
            environment_type(environment),
            value(callee),
            callee.0,
            alignment(&Type::Closure(environment), self.closures)?
        ));
        self.emit_resource_closure_call(
            instruction,
            function,
            &result_ty,
            if pointer {
                format!("ptr %closure.once.{}", callee.0)
            } else {
                format!("{} {}", environment_type(environment), value(callee))
            },
            arguments,
        )?;
        if has_owned {
            self.line(format!(
                "  call void @zil.closure.drop.{}(ptr %closure.once.{})",
                environment.0, callee.0
            ));
        }
        Ok(())
    }

    fn emit_resource_closure_call(
        &mut self,
        instruction: &Instruction,
        function: FunctionId,
        result_ty: &Type,
        environment: String,
        arguments: &[ValueId],
    ) -> CompileResult<()> {
        let assignment = if *result_ty == Type::Unit {
            String::new()
        } else {
            format!("{} = ", value(result(instruction)?))
        };
        let mut emitted_arguments = vec![environment];
        for argument in arguments {
            let ty = self.value_type(*argument)?;
            if *ty != Type::Unit {
                emitted_arguments.push(format!("{} {}", llvm_type(ty)?, value(*argument)));
            }
        }
        self.line(format!(
            "  {assignment}call {} @{}({})",
            llvm_type(result_ty)?,
            symbol(function),
            emitted_arguments.join(", ")
        ));
        Ok(())
    }
}
