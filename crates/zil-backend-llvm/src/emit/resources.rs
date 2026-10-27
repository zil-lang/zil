//! Finite inline headers and plain access descriptions. No heap box, reference
//! count, address-derived identity, or optimizer alias promise is introduced.
use super::*;

pub(super) fn supported_type(ty: &Type, resources: &[ResourceDefinition]) -> bool {
    if dynamic_store::is_resource(ty, resources) || byte_buffer::is_resource(ty, resources) {
        return true;
    }
    match ty {
        Type::Managed(payload) | Type::Desc(payload) => {
            matches!(payload.as_ref(), Type::Record { name, .. }
                if payload.is_copy_value() && resources.iter().any(|r| &r.name == name && &r.payload == payload.as_ref()))
        }
        _ => false,
    }
}

impl FunctionEmitter<'_> {
    fn desc_field(&self, desc: ValueId, index: u32) -> CompileResult<(String, Type)> {
        let Type::Desc(payload) = self.value_type(desc)? else {
            return invalid(
                "E-CODEGEN-CFG",
                "resource access requires a description",
                Some(self.function.span),
            );
        };
        let Type::Record { fields, .. } = payload.as_ref() else {
            return invalid(
                "E-CODEGEN-CFG",
                "resource payload is not a finite record",
                Some(self.function.span),
            );
        };
        let Some((_, field)) = fields.get(index as usize) else {
            return invalid(
                "E-CODEGEN-CFG",
                "resource field is outside its payload",
                Some(self.function.span),
            );
        };
        Ok((llvm_type(payload)?, field.clone()))
    }

    pub(super) fn read_desc(
        &mut self,
        instruction: &Instruction,
        desc: ValueId,
        index: u32,
    ) -> CompileResult<()> {
        let (payload, field) = self.desc_field(desc, index)?;
        if field == Type::Unit {
            return Ok(());
        }
        let pointer = self.temp();
        self.line(format!(
            "  {pointer} = getelementptr {payload}, ptr {}, i32 0, i32 {index}",
            value(desc)
        ));
        self.line(format!(
            "  {} = load {}, ptr {pointer}, align {}",
            value(result(instruction)?),
            llvm_type(&field)?,
            alignment(&field, self.closures)?
        ));
        Ok(())
    }

    pub(super) fn write_desc(
        &mut self,
        _instruction: &Instruction,
        desc: ValueId,
        index: u32,
        input: ValueId,
    ) -> CompileResult<()> {
        let (payload, field) = self.desc_field(desc, index)?;
        if field == Type::Unit {
            return Ok(());
        }
        let pointer = self.temp();
        self.line(format!(
            "  {pointer} = getelementptr {payload}, ptr {}, i32 0, i32 {index}",
            value(desc)
        ));
        self.line(format!(
            "  store {} {}, ptr {pointer}, align {}",
            llvm_type(&field)?,
            value(input),
            alignment(&field, self.closures)?
        ));
        Ok(())
    }

    pub(super) fn drop_managed(
        &mut self,
        instruction: &Instruction,
        local: LocalId,
    ) -> CompileResult<()> {
        let Type::Managed(payload) = self.local_type(local)? else {
            return invalid(
                "E-CODEGEN-CFG",
                "cleanup requires an actual managed local",
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
                "cleanup has no verified resource definition",
                Some(instruction.span),
            );
        };
        let teardown = definition.teardown;
        let byte_buffer = definition.payload == Type::ByteBuffer;
        let store_drop = if matches!(definition.payload, Type::OwnedStore { .. }) { Some(dynamic_store::helper(&definition.payload, self.resources, "drop")?) } else { None };
        let flag = self.temp();
        let label = self.continuation;
        self.continuation += 1;
        self.line(format!(
            "  {flag} = load i1, ptr %resource.live.{}, align 1",
            local.0
        ));
        self.line(format!(
            "  br i1 {flag}, label %resource.drop.{label}, label %resource.done.{label}"
        ));
        self.line(format!("resource.drop.{label}:"));
        // The activation owns the obligation before invoking user code. There
        // is no write into a payload after its teardown could have released it.
        self.line(format!(
            "  store i1 false, ptr %resource.live.{}, align 1",
            local.0
        ));
        if let Some(store_drop) = store_drop {
            self.line(format!("  call void @{store_drop}(ptr %local.{})", local.0));
        } else if byte_buffer {
            self.line(format!(
                "  call void @zil.bytes.drop(ptr %local.{})",
                local.0
            ));
        } else if let Some(teardown) = teardown {
            self.line(format!(
                "  call void @{}(ptr %local.{})",
                symbol(teardown),
                local.0
            ));
        }
        self.line(format!("  br label %resource.done.{label}"));
        self.line(format!("resource.done.{label}:"));
        Ok(())
    }
}
