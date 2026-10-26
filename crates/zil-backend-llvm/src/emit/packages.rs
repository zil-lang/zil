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
                || matches!(ty, Type::Managed(_) if resources::supported_type(ty, resources))
        })
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
            .filter_map(|(index, (_, ty))| matches!(ty, Type::Managed(_)).then_some(index))
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
        if !matches!(field, Type::Managed(_)) {
            return invalid(
                "E-CODEGEN-CFG",
                "field take requires a Managed field",
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
        if matches!(field, Type::Managed(_)) {
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
        let Type::Managed(payload) = &field else {
            return invalid(
                "E-CODEGEN-CFG",
                "field cleanup requires a Managed field",
                Some(instruction.span),
            );
        };
        let Some(definition) = self
            .resources
            .iter()
            .find(|r| &r.payload == payload.as_ref())
        else {
            return invalid(
                "E-CODEGEN-CFG",
                "package field lacks its resource definition",
                Some(instruction.span),
            );
        };
        let teardown = if definition.payload == Type::ByteBuffer {
            Some("zil.bytes.drop".into())
        } else {
            definition.teardown.map(symbol)
        };
        self.emit_package_field_drop(local, index, &package, teardown.as_deref());
        Ok(())
    }

    fn emit_package_field_drop(
        &mut self,
        local: LocalId,
        index: u32,
        package: &str,
        teardown: Option<&str>,
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
        if let Some(teardown) = teardown {
            let pointer = self.temp();
            self.line(format!(
                "  {pointer} = getelementptr {package}, ptr %local.{}, i32 0, i32 {index}",
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
            if let Type::Managed(payload) = ty {
                let Some(definition) = self
                    .resources
                    .iter()
                    .find(|r| &r.payload == payload.as_ref())
                else {
                    return invalid(
                        "E-CODEGEN-CFG",
                        "package field lacks its resource definition",
                        Some(instruction.span),
                    );
                };
                let teardown = if definition.payload == Type::ByteBuffer {
                    Some("zil.bytes.drop".into())
                } else {
                    definition.teardown.map(symbol)
                };
                teardowns.push((index, teardown));
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
        for (index, teardown) in teardowns {
            if self.partial_packages.contains(&local) {
                self.emit_package_field_drop(local, index as u32, &package, teardown.as_deref());
            } else if let Some(teardown) = teardown {
                let pointer = self.temp();
                self.line(format!(
                    "  {pointer} = getelementptr {package}, ptr %local.{}, i32 0, i32 {index}",
                    local.0
                ));
                self.line(format!("  call void @{teardown}(ptr {pointer})"));
            }
        }
        self.line(format!("  br label %package.done.{label}"));
        self.line(format!("package.done.{label}:"));
        Ok(())
    }
}
