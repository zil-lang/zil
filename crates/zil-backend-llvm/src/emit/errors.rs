//! Finite synchronous errors use an inline tag and two fixed payload slots.
//! The inactive payload is zero initialized; no allocation or unwind is hidden
//! here. This deliberately simple representation is not a compact-union ABI.
use super::*;

pub(super) fn owned(ty: &Type) -> bool {
    matches!(ty, Type::ErrorUnion { error, result }
        if matches!(error.as_ref(), Type::Package { .. })
            || matches!(result.as_ref(), Type::Package { .. }))
}

pub(super) fn supported_type(ty: &Type, resources: &[ResourceDefinition]) -> bool {
    matches!(ty, Type::ErrorUnion { error, result }
        if (error.is_copy_value() || packages::supported_type(error, resources))
            && (result.is_copy_value() || packages::supported_type(result, resources)))
}

fn teardowns(
    payload: &Type,
    resources: &[ResourceDefinition],
) -> CompileResult<Vec<(usize, String)>> {
    let Type::Package { fields, .. } = payload else {
        return Ok(Vec::new());
    };
    let mut result = Vec::new();
    for (index, (_, ty)) in fields.iter().enumerate() {
        if let Type::Managed(inner) = ty {
            let definition = resources
                .iter()
                .find(|r| &r.payload == inner.as_ref())
                .ok_or_else(|| {
                    vec![Diagnostic::error(
                        Phase::Codegen,
                        "E-CODEGEN-CFG",
                        None,
                        "error payload field lacks its resource definition",
                    )]
                })?;
            if definition.payload == Type::ByteBuffer {
                result.push((index, "zil.bytes.drop".into()));
            } else if let Some(drop) = definition.teardown {
                result.push((index, symbol(drop)));
            }
        }
    }
    Ok(result)
}

/// The native boundary consumes the returned error Value. Source main's own
/// cleanup has already run; only this selected returned payload remains owed.
pub(super) fn entry_cleanup(ty: &Type, resources: &[ResourceDefinition]) -> CompileResult<String> {
    let Type::ErrorUnion { error, .. } = ty else {
        return Ok(String::new());
    };
    if !matches!(error.as_ref(), Type::Package { .. }) {
        return Ok(String::new());
    }
    let union = llvm_type(ty)?;
    let payload = llvm_type(error)?;
    let mut text = format!(
        "  %entry.error.value = extractvalue {union} %entry.result, 1\n  %entry.error.storage = alloca {payload}\n  store {payload} %entry.error.value, ptr %entry.error.storage\n"
    );
    for (index, teardown) in teardowns(error, resources)? {
        text.push_str(&format!(
            "  %entry.error.field.{index} = getelementptr {payload}, ptr %entry.error.storage, i32 0, i32 {index}\n  call void @{teardown}(ptr %entry.error.field.{index})\n"
        ));
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
        self.line(format!(
            "  {flag} = load i1, ptr %resource.live.{}, align 1",
            local.0
        ));
        self.line(format!(
            "  br i1 {flag}, label %error.drop.{label}, label %error.done.{label}"
        ));
        self.line(format!("error.drop.{label}:"));
        // Cleanup owns the selected carrier before it can enter user teardown.
        // The inactive payload is never read or given a cleanup obligation.
        self.line(format!(
            "  store i1 false, ptr %resource.live.{}, align 1",
            local.0
        ));
        let tag_pointer = self.temp();
        let tag = self.temp();
        self.line(format!(
            "  {tag_pointer} = getelementptr {union}, ptr %local.{}, i32 0, i32 0",
            local.0
        ));
        self.line(format!("  {tag} = load i1, ptr {tag_pointer}, align 1"));
        self.line(format!(
            "  br i1 {tag}, label %error.payload.{label}, label %success.payload.{label}"
        ));
        for (name, arm, payload, drops) in [
            ("error", 1, error.as_ref(), error_drops),
            ("success", 2, result.as_ref(), success_drops),
        ] {
            self.line(format!("{name}.payload.{label}:"));
            if !drops.is_empty() {
                let pointer = self.temp();
                let payload_llvm = llvm_type(payload)?;
                self.line(format!(
                    "  {pointer} = getelementptr {union}, ptr %local.{}, i32 0, i32 {arm}",
                    local.0
                ));
                for (index, teardown) in drops {
                    let field = self.temp();
                    self.line(format!("  {field} = getelementptr {payload_llvm}, ptr {pointer}, i32 0, i32 {index}"));
                    self.line(format!("  call void @{teardown}(ptr {field})"));
                }
            }
            self.line(format!("  br label %error.done.{label}"));
        }
        self.line(format!("error.done.{label}:"));
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
        self.line(format!(
            "  {tagged} = insertvalue {llvm} zeroinitializer, i1 {is_error}, 0"
        ));
        self.line(format!(
            "  {} = insertvalue {llvm} {tagged}, {payload_llvm} {source}, {}",
            value(result),
            if is_error { 1 } else { 2 }
        ));
        Ok(())
    }

    pub(super) fn error_tag(
        &mut self,
        instruction: &Instruction,
        source: ValueId,
    ) -> CompileResult<()> {
        self.line(format!(
            "  {} = extractvalue {} {}, 0",
            value(result(instruction)?),
            llvm_type(self.value_type(source)?)?,
            value(source)
        ));
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
        self.line(format!(
            "  {tag} = extractvalue {llvm} {}, 0",
            value(source)
        ));
        self.line(format!("  {mismatch} = icmp ne i1 {tag}, {is_error}"));
        // A raw-but-verified projection has defined failure semantics even if
        // its caller did not establish the branch tag statically.
        self.trap_if(&mismatch);
        if self.function.values[result.0 as usize] != Type::Unit {
            self.line(format!(
                "  {} = extractvalue {llvm} {}, {}",
                value(result),
                value(source),
                if is_error { 1 } else { 2 }
            ));
        }
        Ok(())
    }
}
