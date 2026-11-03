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
        let (value, ty) = self.expr(initializer, expected)?;
        if let Some(expected) = expected {
            self.same(&ty, expected, initializer.span)?;
        }
        // This checks the complete value, including discarded fields, exact arity
        // at every tuple and duplicate names, using the existing paid pattern core.
        if !self.conditional_pattern(pattern, &ty)? {
            return Err(unsupported(
                pattern.span,
                "ordinary binding must be irrefutable",
            ));
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
}
