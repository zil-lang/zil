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
            if let Ty::Infer(id) = ty && !Self::numeric_suffix(text) {
                b.spend(text.len().saturating_add(1))?;
                self.integer_variables.insert(id);
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

    // The requirement can select a width for an original literal. Every other
    // type variable, const value, nominal identity and signature role is rigid
    // here. The complete comparison runs again after these scalar equations.
    pub(super) fn constrain_callable_integers(
        &mut self,
        left: &Ty,
        right: &Ty,
        b: &mut Budget,
    ) -> Check<()> {
        fn arguments<'a>(
            left: &'a [Arg],
            right: &'a [Arg],
            pending: &mut Vec<(&'a Ty, &'a Ty)>,
            b: &mut Budget,
        ) -> Check<()> {
            b.spend(left.len().saturating_add(right.len()))?;
            if left.len() == right.len() {
                for (left, right) in left.iter().zip(right) {
                    match (left, right) {
                        (Arg::Type(left), Arg::Type(right)) => pending.push((left, right)),
                        (Arg::Const(left), Arg::Const(right)) if left.value == right.value => {
                            pending.push((&left.carrier, &right.carrier))
                        }
                        _ => {}
                    }
                }
            }
            Ok(())
        }
        let mut pending = vec![(left, right)];
        while let Some((left, right)) = pending.pop() {
            b.spend(1)?;
            if (self.integer_variable(left) && self.integer_carrier(right))
                || (self.integer_variable(right) && self.integer_carrier(left))
            {
                self.unify(left, right, b)?;
                continue;
            }
            match (left, right) {
                (Ty::Tuple(a), Ty::Tuple(z)) if a.len() == z.len() => {
                    b.spend(a.len())?;
                    pending.extend(a.iter().zip(z));
                }
                (
                    Ty::Address {
                        mutable: a,
                        inner: x,
                        ..
                    },
                    Ty::Address {
                        mutable: z,
                        inner: y,
                        ..
                    },
                ) if a == z => pending.push((x, y)),
                (Ty::Desc { inner: x, .. }, Ty::Desc { inner: y, .. })
                | (Ty::Managed(x), Ty::Managed(y))
                | (Ty::Store(x), Ty::Store(y))
                | (Ty::Slice(x), Ty::Slice(y)) => pending.push((x, y)),
                (Ty::Array(x, n), Ty::Array(y, m)) if n.value == m.value => {
                    pending.push((x, y));
                    pending.push((&n.carrier, &m.carrier));
                }
                (Ty::ErrorUnion(e, x), Ty::ErrorUnion(f, y)) => {
                    pending.push((e, f));
                    pending.push((x, y));
                }
                (Ty::Nominal(a, x), Ty::Nominal(z, y)) | (Ty::Opaque(a, x), Ty::Opaque(z, y))
                    if a == z =>
                {
                    arguments(x, y, &mut pending, b)?
                }
                (Ty::Closure(a, x, xe), Ty::Closure(z, y, ye)) if a == z => {
                    arguments(x, y, &mut pending, b)?;
                    arguments(xe, ye, &mut pending, b)?;
                }
                (
                    Ty::Projection {
                        receiver: ar,
                        declaration: a,
                        arguments: x,
                        member: i,
                    },
                    Ty::Projection {
                        receiver: zr,
                        declaration: z,
                        arguments: y,
                        member: j,
                    },
                ) if a == z && i == j => {
                    pending.push((ar, zr));
                    arguments(x, y, &mut pending, b)?;
                }
                (
                    Ty::Code {
                        kind: a,
                        parameters: x,
                        result: ar,
                        modes: am,
                        result_mode: arm,
                        call_kind: ac,
                        bound_lifetimes: ab,
                        invocation_lifetimes: ai,
                    },
                    Ty::Code {
                        kind: z,
                        parameters: y,
                        result: zr,
                        modes: zm,
                        result_mode: zrm,
                        call_kind: zc,
                        bound_lifetimes: zb,
                        invocation_lifetimes: zi,
                    },
                ) if a == z
                    && ac == zc
                    && am == zm
                    && arm == zrm
                    && ab == zb
                    && ai == zi
                    && x.len() == y.len() =>
                {
                    b.spend(x.len())?;
                    pending.extend(x.iter().zip(y));
                    pending.push((ar, zr));
                }
                (
                    Ty::Carrier {
                        kind: a, child: x, ..
                    },
                    Ty::Carrier {
                        kind: z, child: y, ..
                    },
                ) if a == z => pending.push((x, y)),
                _ => {}
            }
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
