//! Concrete nominal and finite resource receiver inference in closure bodies.
//! No new dispatch or ownership rule: final Builder::method_call and independent
//! Verify remain authoritative. Capture writes use the existing address path.
use super::*;

impl InferBuilder<'_, '_> {
    pub(super) fn nominal_method_call(
        &mut self,
        base: LocalType,
        object: &CoreExpr,
        member: &str,
        arguments: &[CoreExpr],
        depth: usize,
        span: Span,
    ) -> R<Option<LocalType>> {
        let variable = match base {
            LocalType::Scalar(v) | LocalType::Address(v, _) | LocalType::Resource(v, _) => v,
            LocalType::SliceLength | LocalType::Closure(..) => return Ok(None),
        };
        self.graph.solve()?;
        let Some(actual) = self.graph.ty(variable)? else {
            return Ok(None);
        };
        let nominal = match &actual {
            Type::Address { inner, .. } => inner.as_ref(),
            other => other,
        };
        if !matches!(
            nominal,
            Type::Record { .. } | Type::Package { .. } | Type::ByteBuffer
        ) {
            return Ok(None);
        }
        self.graph.reserve_type(nominal, 0, span)?;
        let Some(signature) =
            aggregates::resolve_type_method(nominal, member, self.signatures, self.imports)
        else {
            return Ok(None);
        };
        self.reserve_signature(signature, span)?;
        let signature = signature.clone();
        if signature.parameters.len() != arguments.len() + 1 {
            return Err(error(
                "E-CALL-ARITY",
                span,
                "closure method argument count mismatch",
            ));
        }
        let receiver = &signature.parameters[0];
        let receiver_value = match (receiver, base, &actual) {
            (Type::Address { mutable, .. }, LocalType::Scalar(v), ty)
                if !matches!(ty, Type::Address { .. }) =>
            {
                LocalType::Address(v, *mutable)
            }
            _ => base,
        };
        if matches!(receiver, Type::Address { mutable: true, .. }) {
            self.mark_address_write(object, depth + 1)?;
        }
        if matches!(receiver, Type::Desc(_))
            && self
                .resource_writes
                .get(&signature.id)
                .is_some_and(|writes| writes.contains(&0))
        {
            self.mark_resource_write(receiver_value, object.span)?;
        }
        receiver_value.bind(&mut self.graph, receiver, object.span)?;
        for (index, (argument, parameter)) in
            arguments.iter().zip(&signature.parameters[1..]).enumerate()
        {
            let value = self.expr_expected(argument, Some(parameter), depth + 1)?;
            if matches!(parameter, Type::Address { mutable: true, .. }) {
                self.mark_address_write(argument, depth + 1)?;
            }
            if matches!(parameter, Type::Desc(_))
                && self
                    .resource_writes
                    .get(&signature.id)
                    .is_some_and(|writes| writes.contains(&(index + 1)))
            {
                self.mark_resource_write(value, argument.span)?;
            }
            value.bind(&mut self.graph, parameter, argument.span)?;
        }
        self.known(&signature.result, span).map(Some)
    }
}
