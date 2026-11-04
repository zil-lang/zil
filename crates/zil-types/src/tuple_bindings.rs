//! Ordinary irrefutable Copy tuple bindings. No ownership or reference patterns.
use super::*;

fn validate(pattern: &CorePattern, remaining: &mut usize, depth_limit: usize) -> R<()> {
    fn visit(pattern: &CorePattern, remaining: &mut usize, depth: usize, limit: usize) -> R<()> {
        if depth > limit {
            return Err(Diagnostic::limit(
                Phase::Types,
                "tuple binding depth exceeded",
                Some(pattern.span),
            ));
        }
        *remaining = remaining.checked_sub(1).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "tuple binding pattern budget exceeded",
                Some(pattern.span),
            )
        })?;
        match &pattern.kind {
            CorePatternKind::Name(_) | CorePatternKind::Wildcard => Ok(()),
            CorePatternKind::Reference { pattern, .. } => {
                visit(pattern, remaining, depth + 1, limit)
            }
            CorePatternKind::Tuple(fields) => {
                for field in fields {
                    visit(field, remaining, depth + 1, limit)?;
                }
                Ok(())
            }
            _ => Err(unsupported(
                pattern.span,
                "ordinary tuple binding requires an irrefutable tuple/name/discard pattern",
            )),
        }
    }
    visit(pattern, remaining, 0, depth_limit)
}

impl Builder<'_, '_> {
    pub(super) fn tuple_binding(
        &mut self,
        pattern: &CorePattern,
        mutable: bool,
        expected: Option<&Type>,
        initializer: &CoreExpr,
    ) -> R<()> {
        validate(
            pattern,
            &mut self.match_work_left,
            self.limits.max_call_depth,
        )?;
        if reference_patterns::has_reference(pattern) {
            return self.reference_binding(pattern, mutable, expected, initializer);
        }
        let (value, ty) = self.expr(initializer, expected)?;
        if let Some(expected) = expected {
            self.same(&ty, expected, initializer.span)?;
        }
        // This checks the complete value, including discarded fields, exact arity
        // at every tuple and duplicate names, using the existing paid pattern core.
        if !self.irrefutable_pattern(pattern, &ty)? {
            return Err(unsupported(
                pattern.span,
                "ordinary binding must be irrefutable",
            ));
        }
        if owned_shapes::tuple(&ty) {
            let work = inference::type_snapshot_work(&ty, initializer.span, self.limits)?;
            self.closures
                .snapshot(work.saturating_mul(3), initializer.span)?;
            let parent = self.temp(ty.clone(), initializer.span)?;
            self.store(parent, value, initializer.span)?;
            return self.owned_tuple_pattern(pattern, parent, &ty, mutable);
        }
        let mut bindings = Vec::new();
        let unused_fail = self.current.expect("tuple binding in live block");
        self.pattern_test(pattern, value, &ty, unused_fail, &mut bindings)?;
        for (name, value, ty, span) in bindings {
            let local = self.local(&name, ty, mutable, span)?;
            self.store(local, value, span)?;
        }
        Ok(())
    }
    fn owned_tuple_pattern(
        &mut self,
        p: &CorePattern,
        root: LocalId,
        ty: &Type,
        mutable: bool,
    ) -> R<()> {
        let work = inference::type_snapshot_work(ty, p.span, self.limits)?;
        self.closures.snapshot(work.saturating_mul(4), p.span)?;
        match &p.kind {
            CorePatternKind::Name(name) => {
                let kind = if ty.is_copy_value() {
                    InstructionKind::Load(root)
                } else {
                    InstructionKind::TakeLocal { local: root }
                };
                let v = self.value(kind, ty.clone(), p.span)?;
                let local = self.local(name, ty.clone(), mutable, p.span)?;
                self.store(local, v, p.span)
            }
            CorePatternKind::Wildcard => Ok(()),
            CorePatternKind::Tuple(fields) => {
                let Type::Tuple(types) = ty else {
                    unreachable!("checked tuple pattern")
                };
                if ty.is_copy_value() {
                    let v = self.value(InstructionKind::Load(root), ty.clone(), p.span)?;
                    let mut bindings = Vec::new();
                    self.pattern_test(p, v, ty, self.current.unwrap(), &mut bindings)?;
                    for (name, v, t, span) in bindings {
                        let local = self.local(&name, t, mutable, span)?;
                        self.store(local, v, span)?;
                    }
                    return Ok(());
                }
                self.closures.snapshot(types.len(), p.span)?;
                // One destructuring binding registers fields in reverse order
                // so the aggregate's declaration-order cleanup is preserved.
                let mut targets = Vec::new();
                for (field,t) in fields.iter().zip(types).rev() {
                    let (name,writable)=match &field.kind {
                        CorePatternKind::Name(name)=>(name.clone(),mutable),
                        _=>(format!("$tuple340${}",self.function.locals.len()),true),
                    };
                    targets.push(self.local(&name,t.clone(),writable,field.span)?);
                }
                targets.reverse();
                self.emit(
                    InstructionKind::UnpackTuple {
                        local: root,
                        fields: targets.clone(),
                    },
                    None,
                    p.span,
                )?;
                for ((p, t), l) in fields.iter().zip(types).zip(targets) {
                    if !matches!(p.kind,CorePatternKind::Name(_)|CorePatternKind::Wildcard){self.owned_tuple_pattern(p,l,t,mutable)?;}
                }
                Ok(())
            }
            _ => Err(unsupported(
                p.span,
                "owned tuple binding requires an irrefutable pattern",
            )),
        }
    }
}
