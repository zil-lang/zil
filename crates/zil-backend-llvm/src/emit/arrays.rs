//! Fixed inline owning arrays. Element flags belong to real local storage;
//! complete SSA values and function ABI values contain only element payloads.
use super::*;

pub(super) fn supported_type(ty: &Type, resources: &[ResourceDefinition]) -> bool {
    matches!(ty, Type::Array { element, length }
        if *length > 0 && packages::supported_type(element, resources))
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
                self.line(format!(
                    "  %array.live.{}.{index} = alloca i1, align 1",
                    local.0
                ));
            }
            self.line(format!(
                "  store i1 {live}, ptr %array.live.{}.{index}, align 1",
                local.0
            ));
        }
        Ok(())
    }

    fn owned_array_shape(&self, local: LocalId) -> CompileResult<(String, Type, u32)> {
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
                    self.line(format!("  {} = getelementptr {array}, ptr %local.{}, i32 0, i32 {index}, i32 {field}", value(result(instruction)?), local.0));
                } else {
                    let pointer = self.temp();
                    self.line(format!("  {pointer} = getelementptr {array}, ptr %local.{}, i32 0, i32 {index}, i32 {field}", local.0));
                    self.line(format!(
                        "  {} = load {}, ptr {pointer}, align {}",
                        value(result(instruction)?),
                        llvm_type(ty)?,
                        alignment(ty, self.closures)?
                    ));
                }
            }
            InstructionKind::TakeOwnedArrayElement { index, .. } => {
                let pointer = self.temp();
                self.line(format!(
                    "  {pointer} = getelementptr {array}, ptr %local.{}, i32 0, i32 {index}",
                    local.0
                ));
                self.line(format!(
                    "  {} = load {}, ptr {pointer}, align {}",
                    value(result(instruction)?),
                    llvm_type(&element)?,
                    alignment(&element, self.closures)?
                ));
                self.line(format!(
                    "  store i1 false, ptr %array.live.{}.{index}, align 1",
                    local.0
                ));
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
                self.line(format!(
                    "  {pointer} = getelementptr {array}, ptr %local.{}, i32 0, i32 {index}",
                    local.0
                ));
                self.line(format!(
                    "  store {} {}, ptr {pointer}, align {}",
                    llvm_type(&element)?,
                    value(input),
                    alignment(&element, self.closures)?
                ));
                self.line(format!(
                    "  store i1 true, ptr %array.live.{}.{index}, align 1",
                    local.0
                ));
            }
            InstructionKind::DropOwnedArrayElement { index, .. } => {
                let teardowns = self.owned_array_teardowns(&element)?;
                self.drop_owned_array_element(local, index, &array, &teardowns)?;
            }
            InstructionKind::DropOwnedArray { .. } => {
                let flag = self.temp();
                let label = self.continuation;
                self.continuation += 1;
                self.line(format!(
                    "  {flag} = load i1, ptr %resource.live.{}, align 1",
                    local.0
                ));
                self.line(format!(
                    "  br i1 {flag}, label %array.drop.{label}, label %array.done.{label}"
                ));
                self.line(format!("array.drop.{label}:"));
                self.line(format!(
                    "  store i1 false, ptr %resource.live.{}, align 1",
                    local.0
                ));
                let teardowns = self.owned_array_teardowns(&element)?;
                for index in 0..length {
                    self.drop_owned_array_element(local, index, &array, &teardowns)?;
                }
                self.line(format!("  br label %array.done.{label}"));
                self.line(format!("array.done.{label}:"));
            }
            InstructionKind::BorrowOwnedArrayElement { .. } => {
                return unsupported(
                    "owning array closure borrowing is outside the Package-only slice",
                    Some(instruction.span),
                );
            }
            _ => unreachable!("owning array local operation"),
        }
        Ok(())
    }

    fn owned_array_teardowns(&self, element: &Type) -> CompileResult<Vec<(usize, String)>> {
        let Type::Package { fields, .. } = element else {
            return invalid(
                "E-CODEGEN-CFG",
                "owning array element cleanup requires Package",
                Some(self.function.span),
            );
        };
        let mut teardowns = Vec::new();
        for (field, (_, ty)) in fields.iter().enumerate() {
            if let Type::Managed(payload) = ty {
                let definition = self
                    .resources
                    .iter()
                    .find(|r| &r.payload == payload.as_ref())
                    .ok_or_else(|| {
                        vec![Diagnostic::error(
                            Phase::Codegen,
                            "E-CODEGEN-CFG",
                            Some(self.function.span),
                            "owning array element resource definition is absent",
                        )]
                    })?;
                let teardown = if definition.payload == Type::ByteBuffer {
                    Some("zil.bytes.drop".into())
                } else {
                    definition.teardown.map(symbol)
                };
                if let Some(teardown) = teardown {
                    teardowns.push((field, teardown));
                }
            }
        }
        Ok(teardowns)
    }

    fn drop_owned_array_element(
        &mut self,
        local: LocalId,
        index: u32,
        array: &str,
        teardowns: &[(usize, String)],
    ) -> CompileResult<()> {
        let flag = self.temp();
        let label = self.continuation;
        self.continuation += 1;
        self.line(format!(
            "  {flag} = load i1, ptr %array.live.{}.{index}, align 1",
            local.0
        ));
        self.line(format!(
            "  br i1 {flag}, label %array.element.drop.{label}, label %array.element.done.{label}"
        ));
        self.line(format!("array.element.drop.{label}:"));
        // Retire the element before invoking teardown. Its Package is complete,
        // so no hidden nested partial-state flags are read after any payload free.
        self.line(format!(
            "  store i1 false, ptr %array.live.{}.{index}, align 1",
            local.0
        ));
        for (field, teardown) in teardowns {
            let pointer = self.temp();
            self.line(format!("  {pointer} = getelementptr {array}, ptr %local.{}, i32 0, i32 {index}, i32 {field}", local.0));
            self.line(format!("  call void @{teardown}(ptr {pointer})"));
        }
        self.line(format!("  br label %array.element.done.{label}"));
        self.line(format!("array.element.done.{label}:"));
        Ok(())
    }
}
