use super::*;

impl CheckedOriginal {
    pub(super) fn fresh_integer(&mut self, b: &mut Budget) -> Check<Ty> {
        let variable = self.fresh(b)?;
        let Ty::Infer(id) = variable else { unreachable!() };
        self.integer_variables.insert(id);
        Ok(variable)
    }

    pub(super) fn integer_variable(&self, ty: &Ty) -> bool {
        matches!(ty, Ty::Infer(id) if self.integer_variables.contains(id))
    }

    pub(super) fn integer_carrier(&self, ty: &Ty) -> bool {
        self.integer_variable(ty) || matches!(ty, Ty::Atom(atom) if atom.is_integer())
    }

    pub(super) fn expression_literal(
        &mut self,
        module: &TypedModule,
        text: &str,
        floating: bool,
        negate: bool,
        expected: Option<&Ty>,
        b: &mut Budget,
    ) -> Check<Ty> {
        if !floating && let Some(expected) = expected {
            let ty = self.resolved(expected, b)?;
            if self.integer_variable(&ty) && !Self::numeric_suffix(text) {
                b.spend(text.len().saturating_add(1))?;
                self.integer_literals.push((ty.clone(), text.into(), negate));
                return Ok(ty);
            }
        }
        self.literal_original_sign(module, text, floating, negate, expected, b)
    }

    // Captures, aliases and callers all constrain the same lexical binding.
    // Only after traversing every original body can an unsolved width default.
    pub(super) fn finish_integer_literals(&mut self, module: &TypedModule, b: &mut Budget) -> Check<()> {
        for (variable, text, negate) in std::mem::take(&mut self.integer_literals) {
            let mut ty = self.resolved(&variable, b)?;
            if self.integer_variable(&ty) {
                self.unify(&ty, &Ty::Atom(Type::I32), b)?;
                ty = Ty::Atom(Type::I32);
            }
            self.literal_original_sign(module, &text, false, negate, Some(&ty), b)?;
        }
        Ok(())
    }

    pub(super) fn numeric_suffix(text: &str) -> bool {
        ["isize", "usize", "i64", "i32", "i16", "i8", "u64", "u32", "u16", "u8", "f64", "f32"]
            .iter().any(|suffix| text.ends_with(suffix))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_integer_joins_survive_capture_and_check_the_selected_width() {
        let limits = Limits::default();
        let (module, _) = zil_ir::wire::decode_module_candidate(
            include_bytes!("../fixtures/ordinary-identity-complete-head.wire"),
            &limits,
        ).unwrap();
        let mut b = Budget::new(&limits);
        let mut original = CheckedOriginal::default();
        let integer = original.fresh_integer(&mut b).unwrap();
        let capture = original.fresh(&mut b).unwrap();
        let alias = original.fresh(&mut b).unwrap();
        original.expression_literal(&module, "9223372036854775808", false, true, Some(&integer), &mut b).unwrap();
        original.unify(&integer, &capture, &mut b).unwrap();
        original.unify(&alias, &capture, &mut b).unwrap();
        assert!(original.unify(&alias, &Ty::Atom(Type::F64), &mut b).is_err());
        original.unify(&alias, &Ty::Atom(Type::I64), &mut b).unwrap();
        assert!(original.unify(&capture, &Ty::Atom(Type::I32), &mut b).is_err());
        original.finish_integer_literals(&module, &mut b).unwrap();

        let narrow = original.fresh_integer(&mut b).unwrap();
        original.expression_literal(&module, "256", false, false, Some(&narrow), &mut b).unwrap();
        original.unify(&narrow, &Ty::Atom(Type::U8), &mut b).unwrap();
        assert!(original.finish_integer_literals(&module, &mut b).is_err());

        let defaulted = original.fresh_integer(&mut b).unwrap();
        original.expression_literal(&module, "19", false, false, Some(&defaulted), &mut b).unwrap();
        original.finish_integer_literals(&module, &mut b).unwrap();
        assert_eq!(original.resolved(&defaulted, &mut b).unwrap(), Ty::Atom(Type::I32));
    }
}
