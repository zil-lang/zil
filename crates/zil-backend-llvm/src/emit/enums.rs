//! Tagged unions use one aligned max-payload area. All payload bytes start at
//! zero; recursive scalar stores preserve padding and bool's spare bits.
use super::*;

#[derive(Clone, Copy)]
struct Layout {
    size: u64,
    align: u32,
}
#[derive(Clone, Copy)]
pub(super) struct UnionLayout {
    pub words: u64,
    pub word_align: u32,
    pub align: u32,
}
fn overflow<T>() -> CompileResult<T> {
    Err(vec![Diagnostic::limit(
        Phase::Codegen,
        "enum physical layout exceeds target address range",
        None,
    )])
}
fn rounded(size: u64, align: u32) -> CompileResult<u64> {
    size.checked_add(u64::from(align) - 1)
        .map(|n| n / u64::from(align) * u64::from(align))
        .filter(|n| *n <= i64::MAX as u64)
        .ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Codegen,
                "enum layout size overflow",
                None,
            )]
        })
}
fn fields_layout<'a>(fields: impl IntoIterator<Item = &'a Type>) -> CompileResult<Layout> {
    let mut result = Layout { size: 0, align: 1 };
    for field in fields {
        let layout = physical_layout(field)?;
        result.align = result.align.max(layout.align);
        result.size = rounded(result.size, layout.align)?
            .checked_add(layout.size)
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Codegen,
                    "enum payload size overflow",
                    None,
                )]
            })?;
    }
    result.size = rounded(result.size, result.align)?;
    Ok(result)
}
fn physical_layout(ty: &Type) -> CompileResult<Layout> {
    Ok(match ty {
        Type::Unit => Layout { size: 0, align: 1 },
        Type::Bool | Type::U8 => Layout { size: 1, align: 1 },
        Type::U16 => Layout { size: 2, align: 2 },
        Type::I32 => Layout { size: 4, align: 4 },
        Type::I64
        | Type::FunctionPointer { .. }
        | Type::ForeignHandle
        | Type::CWideString
        | Type::ForeignCallback { .. } => Layout { size: 8, align: 8 },
        Type::Tuple(fields) => fields_layout(fields)?,
        Type::Record { fields, .. } => fields_layout(fields.iter().map(|(_, ty)| ty))?,
        Type::ErrorUnion { error, result } => {
            fields_layout([&Type::Bool, error.as_ref(), result.as_ref()])?
        }
        Type::Array { element, length } => {
            let element = physical_layout(element)?;
            let Some(size) = element
                .size
                .checked_mul(u64::from(*length))
                .filter(|n| *n <= i64::MAX as u64)
            else {
                return overflow();
            };
            Layout {
                size,
                align: element.align,
            }
        }
        Type::Enum { .. } => {
            let layout = union_layout(ty)?;
            let payload = layout
                .words
                .checked_mul(u64::from(layout.word_align))
                .ok_or_else(|| {
                    vec![Diagnostic::limit(
                        Phase::Codegen,
                        "enum payload size overflow",
                        None,
                    )]
                })?;
            let Some(size) = rounded(4, layout.word_align)?.checked_add(payload) else {
                return overflow();
            };
            Layout {
                size: rounded(size, layout.align)?,
                align: layout.align,
            }
        }
        _ => return unsupported("non-Copy enum payload physical layout", None),
    })
}
pub(super) fn union_layout(ty: &Type) -> CompileResult<UnionLayout> {
    let Type::Enum { variants, .. } = ty else {
        return unsupported("union layout needs enum type", None);
    };
    let mut bytes = 0;
    let mut align = 1;
    for (_, fields) in variants {
        let layout = fields_layout(fields)?;
        bytes = bytes.max(layout.size);
        align = align.max(layout.align);
    }
    Ok(UnionLayout {
        words: rounded(bytes, align)? / u64::from(align),
        word_align: align,
        align: align.max(4),
    })
}
pub(super) fn llvm_enum_type(ty: &Type) -> CompileResult<String> {
    let layout = union_layout(ty)?;
    Ok(format!(
        "{{ i32, [{} x i{}] }}",
        layout.words,
        layout.word_align * 8
    ))
}

impl FunctionEmitter<'_> {
    pub(super) fn enum_slot(&mut self, instruction: &Instruction) -> CompileResult<()> {
        let (ty, result) = match &instruction.kind {
            InstructionKind::MakeVariant { .. } => {
                let result = result(instruction)?;
                (&self.function.values[result.0 as usize], result)
            }
            InstructionKind::ReadVariant { value, .. } => (
                &self.function.values[value.0 as usize],
                result(instruction)?,
            ),
            _ => return Ok(()),
        };
        self.line(format!(
            "  %enum.slot.{} = alloca {}, align {}",
            result.0,
            llvm_type(ty)?,
            union_layout(ty)?.align
        ));
        Ok(())
    }
    pub(super) fn make_variant(
        &mut self,
        instruction: &Instruction,
        variant: u32,
        fields: &[ValueId],
    ) -> CompileResult<()> {
        let result = result(instruction)?;
        let ty = self.function.values[result.0 as usize].clone();
        let Type::Enum { variants, .. } = &ty else {
            return unsupported("verified variant lost enum type", Some(instruction.span));
        };
        let payload = Type::Tuple(variants[variant as usize].1.clone());
        let llvm = llvm_type(&ty)?;
        let layout = union_layout(&ty)?;
        let slot = format!("%enum.slot.{}", result.0);
        self.line(format!(
            "  store {llvm} zeroinitializer, ptr {slot}, align {}",
            layout.align
        ));
        let tag = self.temp();
        self.line(format!(
            "  {tag} = getelementptr {llvm}, ptr {slot}, i32 0, i32 0"
        ));
        self.line(format!("  store i32 {variant}, ptr {tag}, align 4"));
        let base = self.temp();
        self.line(format!(
            "  {base} = getelementptr {llvm}, ptr {slot}, i32 0, i32 1"
        ));
        for (index, field) in fields.iter().enumerate() {
            let field_ty = &variants[variant as usize].1[index];
            if *field_ty == Type::Unit {
                continue;
            }
            let address = self.temp();
            self.line(format!(
                "  {address} = getelementptr {}, ptr {base}, i32 0, i32 {index}",
                llvm_type(&payload)?
            ));
            self.store_payload(field_ty, &value(*field), &address)?;
        }
        self.line(format!(
            "  {} = load {llvm}, ptr {slot}, align {}",
            value(result),
            layout.align
        ));
        Ok(())
    }
    pub(super) fn variant_tag(
        &mut self,
        instruction: &Instruction,
        source: ValueId,
    ) -> CompileResult<()> {
        self.line(format!(
            "  {} = extractvalue {} {}, 0",
            value(result(instruction)?),
            llvm_type(&self.function.values[source.0 as usize])?,
            value(source)
        ));
        Ok(())
    }
    pub(super) fn read_variant(
        &mut self,
        instruction: &Instruction,
        source: ValueId,
        variant: u32,
        index: u32,
    ) -> CompileResult<()> {
        let result = result(instruction)?;
        let ty = self.function.values[source.0 as usize].clone();
        let Type::Enum { variants, .. } = &ty else {
            return unsupported("verified projection lost enum type", Some(instruction.span));
        };
        let fields = &variants[variant as usize].1;
        let llvm = llvm_type(&ty)?;
        let tag = self.temp();
        self.line(format!(
            "  {tag} = extractvalue {llvm} {}, 0",
            value(source)
        ));
        let mismatch = self.temp();
        self.line(format!("  {mismatch} = icmp ne i32 {tag}, {variant}"));
        self.trap_if(&mismatch);
        if fields[index as usize] == Type::Unit {
            return Ok(());
        }
        let slot = format!("%enum.slot.{}", result.0);
        self.line(format!(
            "  store {llvm} {}, ptr {slot}, align {}",
            value(source),
            union_layout(&ty)?.align
        ));
        let base = self.temp();
        self.line(format!(
            "  {base} = getelementptr {llvm}, ptr {slot}, i32 0, i32 1"
        ));
        let address = self.temp();
        self.line(format!(
            "  {address} = getelementptr {}, ptr {base}, i32 0, i32 {index}",
            llvm_type(&Type::Tuple(fields.clone()))?
        ));
        self.line(format!(
            "  {} = load {}, ptr {address}, align {}",
            value(result),
            llvm_type(&fields[index as usize])?,
            physical_layout(&fields[index as usize])?.align
        ));
        Ok(())
    }
    fn store_payload(&mut self, ty: &Type, source: &str, address: &str) -> CompileResult<()> {
        match ty {
            Type::Unit => {}
            Type::Bool => {
                let widened = self.temp();
                self.line(format!("  {widened} = zext i1 {source} to i8"));
                self.line(format!("  store i8 {widened}, ptr {address}, align 1"));
            }
            Type::U8
            | Type::U16
            | Type::I32
            | Type::I64
            | Type::FunctionPointer { .. }
            | Type::ForeignHandle
            | Type::CWideString
            | Type::ForeignCallback { .. } => self.line(format!(
                "  store {} {source}, ptr {address}, align {}",
                llvm_type(ty)?,
                physical_layout(ty)?.align
            )),
            Type::Enum { .. } => {
                let llvm = llvm_type(ty)?;
                let layout = union_layout(ty)?;
                let tag = self.temp();
                let tag_address = self.temp();
                self.line(format!("  {tag} = extractvalue {llvm} {source}, 0"));
                self.line(format!(
                    "  {tag_address} = getelementptr {llvm}, ptr {address}, i32 0, i32 0"
                ));
                self.line(format!("  store i32 {tag}, ptr {tag_address}, align 4"));
                if layout.words != 0 {
                    let payload = self.temp();
                    let payload_address = self.temp();
                    self.line(format!("  {payload} = extractvalue {llvm} {source}, 1"));
                    self.line(format!(
                        "  {payload_address} = getelementptr {llvm}, ptr {address}, i32 0, i32 1"
                    ));
                    self.line(format!(
                        "  store [{} x i{}] {payload}, ptr {payload_address}, align {}",
                        layout.words,
                        layout.word_align * 8,
                        layout.word_align
                    ));
                }
            }
            Type::ErrorUnion { error, result } => {
                let llvm = llvm_type(ty)?;
                for (index, field) in [&Type::Bool, error.as_ref(), result.as_ref()]
                    .into_iter()
                    .enumerate()
                {
                    if *field == Type::Unit {
                        continue;
                    }
                    let projected = self.temp();
                    let field_address = self.temp();
                    self.line(format!(
                        "  {projected} = extractvalue {llvm} {source}, {index}"
                    ));
                    self.line(format!("  {field_address} = getelementptr {llvm}, ptr {address}, i32 0, i32 {index}"));
                    self.store_payload(field, &projected, &field_address)?;
                }
            }
            _ => {
                let fields = ty.aggregate_fields().ok_or_else(|| {
                    vec![Diagnostic::unsupported(
                        Phase::Codegen,
                        "non-Copy enum field store",
                        None,
                    )]
                })?;
                let llvm = llvm_type(ty)?;
                for (index, field) in fields.iter().enumerate() {
                    if **field == Type::Unit {
                        continue;
                    }
                    let projected = self.temp();
                    let field_address = self.temp();
                    self.line(format!(
                        "  {projected} = extractvalue {llvm} {source}, {index}"
                    ));
                    self.line(format!("  {field_address} = getelementptr {llvm}, ptr {address}, i32 0, i32 {index}"));
                    self.store_payload(field, &projected, &field_address)?;
                }
            }
        }
        Ok(())
    }
}
