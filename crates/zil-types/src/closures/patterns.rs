//! Pattern constraints share the ordinary private value graph; they grant no access.
use super::*;

impl Graph {
    pub(crate) fn pattern_bind(
        &mut self,
        pattern: &CorePattern,
        value: Variable,
    ) -> R<Vec<(String, Variable, Span)>> {
        let mut bindings = Vec::new();
        self.pattern_constraints(
            pattern,
            value,
            &mut std::collections::BTreeSet::new(),
            &mut bindings,
            0,
        )?;
        self.solve()?;
        Ok(bindings)
    }
    fn pattern_variable(&mut self, span: Span) -> R<Variable> {
        self.spend(span)?;
        if self.nodes.len() >= self.node_limit {
            return Err(limit("pattern constraint variable budget exceeded", span));
        }
        Ok(self.fresh(VALUE, false))
    }
    fn pattern_constraints(
        &mut self,
        pattern: &CorePattern,
        value: Variable,
        names: &mut std::collections::BTreeSet<String>,
        bindings: &mut Vec<(String, Variable, Span)>,
        depth: usize,
    ) -> R<()> {
        self.spend(pattern.span)?;
        if depth > self.depth_limit {
            return Err(limit(
                "pattern constraint depth budget exceeded",
                pattern.span,
            ));
        }
        match &pattern.kind {
            CorePatternKind::Constructor { .. }
            | CorePatternKind::RecordConstructor { .. }
            | CorePatternKind::Float(_)
            | CorePatternKind::String(_)
            | CorePatternKind::Character(_) => {
                return Err(error(
                    "E-CORE-ELABORATION",
                    pattern.span,
                    "unelaborated source pattern reached executable Core",
                ));
            }

            CorePatternKind::Name(name) => {
                self.spend_count(name.len(), pattern.span)?;
                if !names.insert(name.clone()) {
                    return Err(error(
                        "E-MATCH-BINDING",
                        pattern.span,
                        "pattern binds a name more than once",
                    ));
                }
                bindings.push((name.clone(), value, pattern.span));
            }
            CorePatternKind::Wildcard => {}
            CorePatternKind::Reference {
                mutable,
                pattern: inner,
            } => {
                let address = self.pattern_variable(pattern.span)?;
                self.address(address, value, Some(*mutable), pattern.span)?;
                self.pattern_constraints(inner, address, names, bindings, depth + 1)?;
            }
            CorePatternKind::ErrorArm {
                is_error,
                pattern: inner,
            } => {
                let expected = self.ty(value)?.ok_or_else(|| {
                    error(
                        "E-MATCH-TYPE",
                        pattern.span,
                        "error arm needs a known ErrorUnion type",
                    )
                })?;
                self.reserve_type(&expected, 0, pattern.span)?;
                let Type::ErrorUnion {
                    error: error_ty,
                    result,
                } = &expected
                else {
                    return Err(error(
                        "E-MATCH-TYPE",
                        pattern.span,
                        "error arm requires an ErrorUnion",
                    ));
                };
                let variable = self.pattern_variable(inner.span)?;
                self.bind(
                    variable,
                    if *is_error { error_ty } else { result },
                    inner.span,
                )?;
                self.pattern_constraints(inner, variable, names, bindings, depth + 1)?;
            }
            CorePatternKind::Bool(_) => self.bind(value, &Type::Bool, pattern.span)?,
            CorePatternKind::Integer(text) => {
                self.spend_count(text.len(), pattern.span)?;
                self.constrain(value, INTEGER, pattern.span)?;
            }
            CorePatternKind::Tuple(fields) if fields.is_empty() => {
                self.bind(value, &Type::Unit, pattern.span)?
            }
            CorePatternKind::Tuple(fields) => {
                self.spend_count(fields.len(), pattern.span)?;
                let mut variables = Vec::with_capacity(fields.len());
                for _ in fields {
                    variables.push(self.pattern_variable(pattern.span)?);
                }
                let tuple = self.pattern_variable(pattern.span)?;
                self.tuple(tuple, variables.clone(), pattern.span)?;
                self.unify(value, tuple, pattern.span)?;
                for (field, variable) in fields.iter().zip(variables) {
                    self.pattern_constraints(field, variable, names, bindings, depth + 1)?;
                }
            }
            CorePatternKind::Record { ty, fields } => {
                self.bind(value, ty, pattern.span)?;
                self.spend_count(
                    fields
                        .len()
                        .saturating_mul(fields.len())
                        .saturating_add(fields.iter().map(|(name, _)| name.len()).sum::<usize>()),
                    pattern.span,
                )?;
                let ordered = enums::record_pattern_fields(ty, fields, ty, pattern.span)?;
                for (field, field_ty) in ordered {
                    let variable = self.pattern_variable(field.span)?;
                    self.bind(variable, field_ty, field.span)?;
                    self.pattern_constraints(field, variable, names, bindings, depth + 1)?;
                }
            }
            CorePatternKind::Variant {
                ty,
                variant,
                fields,
            } => {
                if variant == enums::TYPE_MEMBER {
                    let expected = self.ty(value)?.ok_or_else(|| {
                        error(
                            "E-ANONYMOUS-SUM",
                            pattern.span,
                            "member pattern requires an already known anonymous sum",
                        )
                    })?;
                    self.reserve_type(&expected, 0, pattern.span)?;
                    self.reserve_type(&expected, 0, pattern.span)?;
                    let (_, types) = enums::pattern_variant(ty, variant, &expected, pattern.span)?;
                    if fields.len() != types.len() {
                        return Err(error(
                            "E-MATCH-ARITY",
                            pattern.span,
                            "member pattern requires one payload",
                        ));
                    }
                    for (field, ty) in fields.iter().zip(types) {
                        let variable = self.pattern_variable(field.span)?;
                        self.bind(variable, ty, field.span)?;
                        self.pattern_constraints(field, variable, names, bindings, depth + 1)?;
                    }
                    return Ok(());
                }
                self.bind(value, ty, pattern.span)?;
                // Keep the complete stored type in the value constraint;
                // only variant structure uses its already-resolved layout.
                let Type::Enum { variants, .. } = stored::physical(ty) else {
                    return Err(error(
                        "E-MATCH-TYPE",
                        pattern.span,
                        "variant pattern requires an enum",
                    ));
                };
                let (_, types) = variants
                    .iter()
                    .find(|(name, _)| name == variant)
                    .ok_or_else(|| error("E-ENUM-VARIANT", pattern.span, "unknown enum variant"))?;
                if types.len() != fields.len() {
                    return Err(error(
                        "E-MATCH-ARITY",
                        pattern.span,
                        "variant pattern arity mismatch",
                    ));
                }
                for (field, ty) in fields.iter().zip(types) {
                    let variable = self.pattern_variable(field.span)?;
                    self.bind(variable, ty, field.span)?;
                    self.pattern_constraints(field, variable, names, bindings, depth + 1)?;
                }
            }
        }
        Ok(())
    }
}
