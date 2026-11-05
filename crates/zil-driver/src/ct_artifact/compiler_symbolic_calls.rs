//! Full symbolic substitution at a call site. Callee bodies are still checked
//! with their own universal binders; these scratch CFGs never enter artifacts.
use super::*;
use cfg::{Builder, Value};

impl Compiler<'_> {
    fn abstract_call_header(
        &mut self,
        declaration: usize,
        module: usize,
        explicit: &[s::GenericArg],
        depth: usize,
        span: Span,
    ) -> R<(Vec<CtTy>, CtTy, Vec<CtTy>)> {
        let d = self.declarations[declaration];
        let mut data = Vec::new();
        let mut scalar = Vec::new();
        let mut lives = Vec::new();
        let mut types = Vec::new();
        for (parameter, actual) in d.function.generics.iter().zip(explicit) {
            self.budget.node(span)?;
            self.budget.text(&parameter.name, span)?;
            match (&parameter.kind, actual) {
                (s::GenericParamKind::Type(_), s::GenericArg::Type(ty)) => {
                    let ty = self.ty(module, ty, depth + 1)?;
                    self.ordinary_copy(&ty, depth + 1, span)?;
                    self.budget.copy_ty(&ty, depth + 1, span)?;
                    self.budget.copy_ty(&ty, depth + 1, span)?;
                    Budget::take(
                        &mut self.budget.bytes,
                        std::mem::size_of::<(String, CtTy)>(),
                        span,
                        "CT symbolic call type bindings exhausted",
                    )?;
                    data.push((parameter.name.clone(), ty.clone()));
                    types.push(ty);
                }
                (s::GenericParamKind::Const(_), actual) => {
                    let value = self.abstract_scalar(parameter, actual, span)?;
                    Budget::take(
                        &mut self.budget.bytes,
                        std::mem::size_of::<(String, symbolic_nominal::ScalarArgument)>(),
                        span,
                        "CT symbolic call scalar bindings exhausted",
                    )?;
                    scalar.push((parameter.name.clone(), value));
                }
                (s::GenericParamKind::Lifetime, s::GenericArg::Lifetime(name)) => {
                    let (definition, ordinal) = self.abstract_life(name, span)?;
                    Budget::take(
                        &mut self.budget.bytes,
                        std::mem::size_of::<(String, CtSymbolId, u32)>(),
                        span,
                        "CT symbolic call life bindings exhausted",
                    )?;
                    lives.push((parameter.name.clone(), definition, ordinal));
                }
                _ => {
                    return Err(error(
                        span,
                        "CT symbolic call actual kind differs from full ordinal",
                    ));
                }
            }
        }
        let old_data = std::mem::replace(&mut self.bindings, data);
        let old_meta = std::mem::take(&mut self.metadata_bindings);
        let old_scalar = std::mem::replace(&mut self.abstract_scalars, scalar);
        let old_lives = std::mem::replace(&mut self.abstract_lives, lives);
        let result = self.header(declaration);
        self.bindings = old_data;
        self.metadata_bindings = old_meta;
        self.abstract_scalars = old_scalar;
        self.abstract_lives = old_lives;
        result.map(|(parameters, result)| (parameters, result, types))
    }
}

impl Builder<'_, '_> {
    pub(super) fn abstract_mixed_call(
        &mut self,
        declaration: usize,
        explicit: Option<&[s::GenericArg]>,
        arguments: &[s::Argument],
        depth: usize,
        span: Span,
    ) -> R<Value> {
        let d = self.compiler.declarations[declaration];
        let explicit = explicit
            .ok_or_else(|| error(span, "CT symbolic full call requires explicit actuals"))?;
        if explicit.len() != d.function.generics.len() || arguments.len() != d.function.params.len()
        {
            return Err(error(span, "CT symbolic full call actual arity differs"));
        }
        // Independent universal checking is queued before call-site substitution.
        // A concrete caller cannot grant operations to the callee's own binders.
        self.compiler.ensure_generic_definition(declaration)?;
        let (parameters, result, types) =
            self.compiler
                .abstract_call_header(declaration, self.module, explicit, depth, span)?;
        let mut values = Vec::new();
        for (argument, ty) in arguments.iter().zip(parameters) {
            self.compiler.budget.node(argument.span)?;
            values.push(self.expr(&argument.value, Some(&ty), depth + 1)?.id);
        }
        // Existing scratch instances check each selected Type substitution while
        // scalar/life body obligations retain the callee's universal identities.
        let function = self.compiler.enqueue_instance(declaration, &types)?;
        let kind = CtInstructionKind::Call {
            function,
            arguments: values,
        };
        if result == CtTy::Scalar(CtScalarTy::Unit) {
            self.op(kind, span)?;
            self.constant(CtConstant::Scalar(Constant::Unit), result, span)
        } else {
            self.emit(kind, result, span)
        }
    }
}
