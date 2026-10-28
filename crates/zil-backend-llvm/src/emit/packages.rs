//! Complete inline values, with field obligations only in local storage.
//! Preparation temporaries retain ownership until an explicit cleanup/commit.
use super::*;

pub(super) fn supported_type(ty: &Type, resources: &[ResourceDefinition]) -> bool {
    let Type::Package { fields, .. } = ty else {
        return false;
    };
    fields.iter().any(|(_, ty)| matches!(ty, Type::Managed(_)))
        && fields.iter().all(|(_, ty)| {
            ty.is_copy_value()
                || matches!(ty, Type::Closure(_))
                || matches!(ty, Type::Managed(_) if resources::supported_type(ty, resources))
                || matches!(ty, Type::Array { element, length } if *length > 0 && supported_type(element, resources) && !has_array_fields(element) && !has_store_fields(element))
        })
}

/// Complete nested Products use the ordinary value ABI. The old predicate
/// remains the only admission route for captures, arrays and error payloads.
pub(super) fn supported_value_type(ty: &Type, resources: &[ResourceDefinition]) -> bool {
    supported_type(ty, resources) || product_type(ty, resources)
}

fn product_type(ty: &Type, resources: &[ResourceDefinition]) -> bool {
    let Type::Package { fields, .. } = ty else {
        return false;
    };
    let mut owns = false;
    for (_, field) in fields {
        match field {
            Type::Managed(payload)
                if !matches!(payload.as_ref(), Type::OwnedStore { .. })
                    && resources::supported_type(field, resources) =>
            {
                owns = true;
            }
            Type::Package { .. } if product_type(field, resources) => owns = true,
            field if field.is_copy_value() => {}
            _ => return false,
        }
    }
    owns
}

fn has_array_fields(ty: &Type) -> bool {
    matches!(ty, Type::Package { fields, .. }
        if fields.iter().any(|(_, ty)| matches!(ty, Type::Array { element, .. } if matches!(element.as_ref(), Type::Package { .. }))))
}

fn has_store_fields(ty: &Type) -> bool {
    matches!(ty, Type::Package { fields, .. }
        if fields.iter().any(|(_, ty)| matches!(ty, Type::Managed(payload) | Type::Desc(payload) if matches!(payload.as_ref(), Type::OwnedStore { .. }))))
}

fn owned_field(ty: &Type) -> bool {
    // An embedded child is complete. Its parent has one direct field flag;
    // internal partial flags belong only to the child's independent local.
    matches!(
        ty,
        Type::Managed(_) | Type::Closure(_) | Type::Package { .. }
    ) || matches!(ty, Type::Array { element, .. } if matches!(element.as_ref(), Type::Package { .. }))
}

/// Complete inline carriers have no nested partial flags. Enumerate only
/// actual owned leaves, in declaration/ascending element order, after Verify
/// has charged the full multiplied layout work.
pub(super) fn teardown_paths(
    ty: &Type,
    resources: &[ResourceDefinition],
) -> CompileResult<Vec<(Vec<u32>, String)>> {
    let mut pending = vec![(ty, Vec::new())];
    let mut result = Vec::new();
    while let Some((ty, path)) = pending.pop() {
        match ty {
            Type::Closure(id) => result.push((path, format!("zil.closure.drop.{}", id.0))),
            Type::Managed(payload) => {
                if matches!(payload.as_ref(), Type::OwnedStore { .. }) {
                    result.push((path, dynamic_store::helper(payload, resources, "drop")?));
                    continue;
                }
                let definition = resources
                    .iter()
                    .find(|r| &r.payload == payload.as_ref())
                    .ok_or_else(|| {
                        vec![Diagnostic::error(
                            Phase::Codegen,
                            "E-CODEGEN-CFG",
                            None,
                            "owned field lacks its resource definition",
                        )]
                    })?;
                let teardown = if definition.payload == Type::ByteBuffer {
                    Some("zil.bytes.drop".into())
                } else {
                    definition.teardown.map(symbol)
                };
                if let Some(teardown) = teardown {
                    result.push((path, teardown));
                }
            }
            Type::Package { fields, .. } => {
                for (index, (_, field)) in fields.iter().enumerate().rev() {
                    if owned_field(field) {
                        let mut child = path.clone();
                        child.push(index as u32);
                        pending.push((field, child));
                    }
                }
            }
            Type::Array { element, length } if matches!(element.as_ref(), Type::Package { .. }) => {
                for index in (0..*length).rev() {
                    let mut child = path.clone();
                    child.push(index);
                    pending.push((element, child));
                }
            }
            _ if ty.is_copy_value() => {}
            _ => {
                return invalid(
                    "E-CODEGEN-CFG",
                    "complete cleanup requires supported owning fields",
                    None,
                );
            }
        }
    }
    Ok(result)
}

fn gep_suffix(path: &[u32]) -> String {
    path.iter().map(|index| format!(", i32 {index}")).collect()
}

impl FunctionEmitter<'_> {
    pub(super) fn package_flags(
        &mut self,
        local: LocalId,
        allocate: bool,
        live: bool,
    ) -> CompileResult<()> {
        if !self.partial_packages.contains(&local) {
            return Ok(());
        }
        let Type::Package { fields, .. } = self.local_type(local)? else {
            return invalid(
                "E-CODEGEN-CFG",
                "field flags require a package",
                Some(self.function.span),
            );
        };
        let owned = fields
            .iter()
            .enumerate()
            .filter_map(|(index, (_, ty))| owned_field(ty).then_some(index))
            .collect::<Vec<_>>();
        for index in owned {
            if allocate {
                self.line(format!(
                    "  %package.live.{}.{index} = alloca i1, align 1",
                    local.0
                ));
            }
            self.line(format!(
                "  store i1 {live}, ptr %package.live.{}.{index}, align 1",
                local.0
            ));
        }
        Ok(())
    }

    fn package_field(&self, local: LocalId, index: u32) -> CompileResult<(String, Type)> {
        let ty = self.local_type(local)?;
        let Type::Package { fields, .. } = ty else {
            return invalid(
                "E-CODEGEN-CFG",
                "package field requires a package local",
                Some(self.function.span),
            );
        };
        let Some((_, field)) = fields.get(index as usize) else {
            return invalid(
                "E-CODEGEN-CFG",
                "package field index exceeds its shape",
                Some(self.function.span),
            );
        };
        Ok((llvm_type(ty)?, field.clone()))
    }

    pub(super) fn borrow_package_field(
        &mut self,
        instruction: &Instruction,
        local: LocalId,
        index: u32,
    ) -> CompileResult<()> {
        let (package, field) = self.package_field(local, index)?;
        if !field.is_copy_value() && !matches!(field, Type::Closure(_)) {
            return invalid(
                "E-CODEGEN-CFG",
                "field address requires a finite Copy field or concrete environment",
                Some(instruction.span),
            );
        }
        self.line(format!(
            "  {} = getelementptr {package}, ptr %local.{}, i32 0, i32 {index}",
            value(result(instruction)?),
            local.0
        ));
        Ok(())
    }

    pub(super) fn read_package_field(
        &mut self,
        instruction: &Instruction,
        local: LocalId,
        index: u32,
    ) -> CompileResult<()> {
        let (package, field) = self.package_field(local, index)?;
        if !field.is_copy_value() {
            return invalid(
                "E-CODEGEN-CFG",
                "only Copy package fields can be read",
                Some(instruction.span),
            );
        }
        if field == Type::Unit {
            return Ok(());
        }
        let pointer = self.temp();
        self.line(format!(
            "  {pointer} = getelementptr {package}, ptr %local.{}, i32 0, i32 {index}",
            local.0
        ));
        self.line(format!(
            "  {} = load {}, ptr {pointer}, align {}",
            value(result(instruction)?),
            llvm_type(&field)?,
            alignment(&field, self.closures)?
        ));
        Ok(())
    }

    pub(super) fn describe_package_field(
        &mut self,
        instruction: &Instruction,
        local: LocalId,
        index: u32,
    ) -> CompileResult<()> {
        let (package, field) = self.package_field(local, index)?;
        if !matches!(field, Type::Managed(_)) {
            return invalid(
                "E-CODEGEN-CFG",
                "only Managed package fields have resource descriptions",
                Some(instruction.span),
            );
        }
        self.line(format!(
            "  {} = getelementptr {package}, ptr %local.{}, i32 0, i32 {index}",
            value(result(instruction)?),
            local.0
        ));
        Ok(())
    }

    pub(super) fn take_package_field(
        &mut self,
        instruction: &Instruction,
        local: LocalId,
        index: u32,
    ) -> CompileResult<()> {
        let (package, field) = self.package_field(local, index)?;
        if !owned_field(&field) {
            return invalid(
                "E-CODEGEN-CFG",
                "field take requires an owning field",
                Some(instruction.span),
            );
        }
        let pointer = self.temp();
        self.line(format!(
            "  {pointer} = getelementptr {package}, ptr %local.{}, i32 0, i32 {index}",
            local.0
        ));
        self.line(format!(
            "  {} = load {}, ptr {pointer}, align {}",
            value(result(instruction)?),
            llvm_type(&field)?,
            alignment(&field, self.closures)?
        ));
        self.line(format!(
            "  store i1 false, ptr %package.live.{}.{index}, align 1",
            local.0
        ));
        Ok(())
    }

    pub(super) fn store_package_field(
        &mut self,
        instruction: &Instruction,
        local: LocalId,
        index: u32,
        input: ValueId,
    ) -> CompileResult<()> {
        let (package, field) = self.package_field(local, index)?;
        if &field != self.value_type(input)? {
            return invalid(
                "E-CODEGEN-CFG",
                "package field store type differs",
                Some(instruction.span),
            );
        }
        if field != Type::Unit {
            let pointer = self.temp();
            self.line(format!(
                "  {pointer} = getelementptr {package}, ptr %local.{}, i32 0, i32 {index}",
                local.0
            ));
            self.line(format!(
                "  store {} {}, ptr {pointer}, align {}",
                llvm_type(&field)?,
                value(input),
                alignment(&field, self.closures)?
            ));
        }
        if owned_field(&field) {
            self.line(format!(
                "  store i1 true, ptr %package.live.{}.{index}, align 1",
                local.0
            ));
        }
        Ok(())
    }

    pub(super) fn drop_package_field(
        &mut self,
        instruction: &Instruction,
        local: LocalId,
        index: u32,
    ) -> CompileResult<()> {
        let (package, field) = self.package_field(local, index)?;
        if !owned_field(&field) {
            return invalid(
                "E-CODEGEN-CFG",
                "field cleanup requires an owning field",
                Some(instruction.span),
            );
        }
        let teardowns = teardown_paths(&field, self.resources)?;
        self.emit_package_field_drop(local, index, &package, &teardowns);
        Ok(())
    }

    fn emit_package_field_drop(
        &mut self,
        local: LocalId,
        index: u32,
        package: &str,
        teardowns: &[(Vec<u32>, String)],
    ) {
        let flag = self.temp();
        let label = self.continuation;
        self.continuation += 1;
        self.line(format!(
            "  {flag} = load i1, ptr %package.live.{}.{index}, align 1",
            local.0
        ));
        self.line(format!(
            "  br i1 {flag}, label %package.field.drop.{label}, label %package.field.done.{label}"
        ));
        self.line(format!("package.field.drop.{label}:"));
        // Retire this obligation before user teardown. The field address can
        // never silently become a live description of the replacement value.
        self.line(format!(
            "  store i1 false, ptr %package.live.{}.{index}, align 1",
            local.0
        ));
        for (path, teardown) in teardowns {
            let pointer = self.temp();
            let suffix = gep_suffix(path);
            self.line(format!(
                "  {pointer} = getelementptr {package}, ptr %local.{}, i32 0, i32 {index}{suffix}",
                local.0
            ));
            self.line(format!("  call void @{teardown}(ptr {pointer})"));
        }
        self.line(format!("  br label %package.field.done.{label}"));
        self.line(format!("package.field.done.{label}:"));
    }

    pub(super) fn drop_package(
        &mut self,
        instruction: &Instruction,
        local: LocalId,
    ) -> CompileResult<()> {
        let ty = self.local_type(local)?;
        let Type::Package { fields, .. } = ty else {
            return invalid(
                "E-CODEGEN-CFG",
                "package cleanup requires a package local",
                Some(instruction.span),
            );
        };
        let package = llvm_type(ty)?;
        // Resolve before emission so malformed LIR cannot create a partial body.
        let mut teardowns = Vec::new();
        for (index, (_, ty)) in fields.iter().enumerate() {
            if owned_field(ty) {
                teardowns.push((index, teardown_paths(ty, self.resources)?));
            }
        }
        let flag = self.temp();
        let label = self.continuation;
        self.continuation += 1;
        self.line(format!(
            "  {flag} = load i1, ptr %resource.live.{}, align 1",
            local.0
        ));
        self.line(format!(
            "  br i1 {flag}, label %package.drop.{label}, label %package.done.{label}"
        ));
        self.line(format!("package.drop.{label}:"));
        // Surrender the carrier obligation before invoking any user teardown.
        // No package or payload write occurs after a field can release its data.
        self.line(format!(
            "  store i1 false, ptr %resource.live.{}, align 1",
            local.0
        ));
        for (index, paths) in teardowns {
            if self.partial_packages.contains(&local) {
                self.emit_package_field_drop(local, index as u32, &package, &paths);
            } else {
                for (path, teardown) in paths {
                    let pointer = self.temp();
                    let suffix = gep_suffix(&path);
                    self.line(format!(
                        "  {pointer} = getelementptr {package}, ptr %local.{}, i32 0, i32 {index}{suffix}", local.0
                    ));
                    self.line(format!("  call void @{teardown}(ptr {pointer})"));
                }
            }
        }
        self.line(format!("  br label %package.done.{label}"));
        self.line(format!("package.done.{label}:"));
        Ok(())
    }
}
