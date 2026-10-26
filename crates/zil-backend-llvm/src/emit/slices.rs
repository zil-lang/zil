//! Native borrowed windows. Permission is established independently by Verify;
//! this module preserves the checked window and guards every offset operation.
use super::*;

impl FunctionEmitter<'_> {
    /// The array object's address is already the address of its first element.
    /// Avoid even a zero-offset GEP here: range guards precede address math.
    fn slice_base(&mut self, address: ValueId) -> CompileResult<(Type, String, String)> {
        let Type::Address { inner, .. } = self.value_type(address)?.clone() else {
            return invalid(
                "E-CODEGEN-CFG",
                "slice projection requires a checked address",
                Some(self.function.span),
            );
        };
        match *inner {
            Type::Array { element, length } => Ok((*element, value(address), length.to_string())),
            Type::Slice { element } => {
                let data = self.temp();
                let length = self.temp();
                self.line(format!(
                    "  {data} = extractvalue {{ ptr, i64 }} {}, 0",
                    value(address)
                ));
                self.line(format!(
                    "  {length} = extractvalue {{ ptr, i64 }} {}, 1",
                    value(address)
                ));
                Ok((*element, data, length))
            }
            _ => invalid(
                "E-CODEGEN-CFG",
                "slice projection lost its array or slice pointee",
                Some(self.function.span),
            ),
        }
    }

    pub(super) fn slice_range(
        &mut self,
        instruction: &Instruction,
        address: ValueId,
        start: Option<ValueId>,
        end: Option<ValueId>,
    ) -> CompileResult<()> {
        let (element, data, length) = self.slice_base(address)?;
        let start = match start {
            Some(value) => self.widened_index(value)?,
            None => "0".into(),
        };
        let end = match end {
            Some(value) => self.widened_index(value)?,
            None => length.clone(),
        };
        // Every admitted backing length is nonnegative. Unsigned comparisons
        // reject sign-extended negative endpoints as well as reversed ranges.
        let reversed = self.temp();
        let outside = self.temp();
        let invalid = self.temp();
        self.line(format!("  {reversed} = icmp ugt i64 {start}, {end}"));
        self.line(format!("  {outside} = icmp ugt i64 {end}, {length}"));
        self.line(format!("  {invalid} = or i1 {reversed}, {outside}"));
        self.trap_if(&invalid);

        let offset = self.temp();
        let count = self.temp();
        let pair = self.temp();
        self.line(format!(
            "  {offset} = getelementptr {}, ptr {data}, i64 {start}",
            aggregate_field_type(&element)?
        ));
        self.line(format!("  {count} = sub i64 {end}, {start}"));
        self.line(format!(
            "  {pair} = insertvalue {{ ptr, i64 }} zeroinitializer, ptr {offset}, 0"
        ));
        self.line(format!(
            "  %v.{} = insertvalue {{ ptr, i64 }} {pair}, i64 {count}, 1",
            result(instruction)?.0
        ));
        Ok(())
    }

    pub(super) fn slice_index(
        &mut self,
        instruction: &Instruction,
        address: ValueId,
        index: ValueId,
    ) -> CompileResult<()> {
        let (element, data, length) = self.slice_base(address)?;
        let index = self.widened_index(index)?;
        let outside = self.temp();
        self.line(format!("  {outside} = icmp uge i64 {index}, {length}"));
        self.trap_if(&outside);
        self.line(format!(
            "  %v.{} = getelementptr {}, ptr {data}, i64 {index}",
            result(instruction)?.0,
            aggregate_field_type(&element)?
        ));
        Ok(())
    }

    pub(super) fn reborrow_slice(
        &mut self,
        instruction: &Instruction,
        address: ValueId,
    ) -> CompileResult<()> {
        let data = self.temp();
        self.line(format!(
            "  {data} = extractvalue {{ ptr, i64 }} {}, 0",
            value(address)
        ));
        // Preserve both fields. No heap box, address erasure or invented length.
        self.line(format!(
            "  %v.{} = insertvalue {{ ptr, i64 }} {}, ptr {data}, 0",
            result(instruction)?.0,
            value(address)
        ));
        Ok(())
    }
}
