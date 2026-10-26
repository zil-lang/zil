//! Finite synchronous errors use an inline tag and two fixed payload slots.
//! The inactive payload is zero initialized; no allocation or unwind is hidden
//! here. This deliberately simple representation is not a compact-union ABI.
use super::*;

impl FunctionEmitter<'_> {
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
