//! Evaluate original constants only after their complete carrier is known.
//! Arithmetic uses the independent V scalar interpreter, never CT or Analyze.
use super::*;

impl CheckedContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn evaluate_constant(
        &self,
        module: &TypedModule,
        recipe: &SemanticConstRecipe,
        carrier: &CheckedType,
        environment: &EvalEnv,
        external: &mut impl ExternalProofs,
        active: &mut ActiveQueries,
        b: &mut Budget,
    ) -> Check<Constant> {
        dynamic_metadata::recipe(recipe, b)?;
        scalar_carrier(carrier, b)?;
        let key = DependencyKey { kind: QueryKind::Const, token: active.fresh(b)? };
        active.with_query(key, b, |active, b| {
            let closed = |pattern: DynConstPattern, b: &mut Budget| {
                scalar::closed(&pattern, &carrier.ty, module.target.pointer_width, b)?
                    .ok_or_else(|| bad("closed semantic scalar expression remains unresolved"))
            };
            match recipe {
                SemanticConstRecipe::Value(value) => closed(DynConstPattern::Value(value.clone()), b),
                SemanticConstRecipe::IntegerLiteral(text) => closed(DynConstPattern::IntegerLiteral(text.clone()), b),
                SemanticConstRecipe::FloatLiteral(text) => closed(DynConstPattern::FloatLiteral(text.clone()), b),
                SemanticConstRecipe::Formal(reference) => {
                    self.constant_actual(self.formal(environment, reference, b)?, carrier, environment, module, b)
                }
                SemanticConstRecipe::Environment { scope, ordinal } => {
                    self.constant_actual(self.environment_actual(environment, *scope, *ordinal, b)?, carrier, environment, module, b)
                }
                SemanticConstRecipe::Unary { op, operand_type, value } => {
                    let operand = self.evaluate_type(module, operand_type, environment, external, active, b)?;
                    scalar_carrier(&operand, b)?;
                    // -MIN is parsed in its actual signed carrier. Parsing its
                    // positive magnitude first would reject a legal minimum.
                    let value = if *op == UnaryOp::Negate && operand.ty.is_integer() {
                        if let SemanticConstRecipe::IntegerLiteral(text) = value.as_ref() {
                            DynConstPattern::IntegerLiteral(text.clone())
                        } else {
                            DynConstPattern::Value(self.evaluate_constant(module, value, &operand, environment, external, active, b)?)
                        }
                    } else {
                        DynConstPattern::Value(self.evaluate_constant(module, value, &operand, environment, external, active, b)?)
                    };
                    closed(DynConstPattern::Unary { op: *op, value: Box::new(value), ty: Box::new(DynTypePattern::Concrete(operand.ty)) }, b)
                }
                SemanticConstRecipe::Binary { op, operand_type, left, right } => {
                    let operand = self.evaluate_type(module, operand_type, environment, external, active, b)?;
                    scalar_carrier(&operand, b)?;
                    let left = self.evaluate_constant(module, left, &operand, environment, external, active, b)?;
                    if matches!((op, &left), (BinaryOp::And, Constant::Bool(false)) | (BinaryOp::Or, Constant::Bool(true))) {
                        return closed(DynConstPattern::Value(left), b);
                    }
                    let right = self.evaluate_constant(module, right, &operand, environment, external, active, b)?;
                    closed(DynConstPattern::Binary { op: *op, left: Box::new(DynConstPattern::Value(left)), right: Box::new(DynConstPattern::Value(right)), ty: Box::new(DynTypePattern::Concrete(operand.ty)) }, b)
                }
                SemanticConstRecipe::Convert { source_type, target_type, value } => {
                    let source = self.evaluate_type(module, source_type, environment, external, active, b)?;
                    let target = self.evaluate_type(module, target_type, environment, external, active, b)?;
                    scalar_carrier(&source, b)?;
                    scalar_carrier(&target, b)?;
                    if !self.same_type(environment, carrier, &target, b)? { return Err(bad("constant conversion target differs from its actual carrier")); }
                    let value = self.evaluate_constant(module, value, &source, environment, external, active, b)?;
                    closed(DynConstPattern::Convert { value: Box::new(DynConstPattern::Value(value)), ty: Box::new(DynTypePattern::Concrete(target.ty)) }, b)
                }
                SemanticConstRecipe::AssociatedConst { receiver, application, member } => {
                    let receiver = self.evaluate_type(module, receiver, environment, external, active, b)?;
                    let application = self.application(module, application, environment, external, active, b)?;
                    let value = external.query(module, self, environment, &ExternalRequest::AssociatedConst { receiver, application, member: *member }, RequirementMode::ActualObligation, active, b)?;
                    let ExternalValue::Const { carrier: actual, value } = value else { return Err(bad("associated Const proof returned another kind")); };
                    if !self.same_type(environment, carrier, &actual, b)? { return Err(bad("associated Const has a different complete carrier")); }
                    closed(DynConstPattern::Value(value), b)
                }
            }
        })
    }

    fn constant_actual(&self, actual: &CheckedActual, expected: &CheckedType, environment: &EvalEnv, module: &TypedModule, b: &mut Budget) -> Check<Constant> {
        let CheckedActual::Const { carrier, value } = actual else { return Err(bad("Const recipe selects a different full formal/environment kind")); };
        if !self.same_type(environment, carrier, expected, b)? { return Err(bad("Const actual has a different complete carrier")); }
        scalar::closed(&DynConstPattern::Value(value.clone()), &expected.ty, module.target.pointer_width, b)?
            .ok_or_else(|| bad("typed Const actual is unresolved"))
    }
}

fn scalar_carrier(value: &CheckedType, b: &mut Budget) -> Check<()> {
    dynamic_metadata::ty(&value.ty, b)?;
    if !value.classes.is_empty() || !value.origins.is_empty() || !value.statics.is_empty()
        || value.life.binder_count != 0 || !value.life.slots.is_empty()
        || !value.life.retained.is_empty() || !value.life.nominal_arguments.is_empty()
        || !(value.ty.is_integer() || value.ty.is_float() || matches!(value.ty, Type::Unit | Type::Bool)) {
        return Err(bad("constant carrier is not a complete scalar Known0 type"));
    }
    Ok(())
}
