//! Substitute scalar values in existing type/annotation expression slots.
//! Named type paths remain in the type namespace; this does not evaluate Type
//! or Syntax producers or relax the ordinary type and attribute checks.
use super::*;

impl materialize::Materializer<'_, '_> {
    pub(super) fn generation_type(
        &mut self,
        ty: &mut s::TypeRef,
        depth: usize,
    ) -> CompileResult<()> {
        self.budget.node(ty.span, depth)?;
        match &mut ty.kind {
            s::TypeKind::Named { args, .. } | s::TypeKind::Dyn { args, .. } => {
                for argument in args {
                    self.generation_argument(argument, depth + 1)?;
                }
            }
            s::TypeKind::Reference { inner, .. }
            | s::TypeKind::Group(inner)
            | s::TypeKind::Optional(inner)
            | s::TypeKind::ForLifetime { inner, .. } => self.generation_type(inner, depth + 1)?,
            s::TypeKind::Tuple(types) | s::TypeKind::Union(types) | s::TypeKind::Impl(types) => {
                for ty in types {
                    self.generation_type(ty, depth + 1)?;
                }
            }
            s::TypeKind::Array { element, length } => {
                self.generation_type(element, depth + 1)?;
                if let Some(length) = length {
                    let was_name = matches!(length.kind, s::ExprKind::Name(_));
                    self.expression(length, depth + 1)?;
                    // Array length syntax stores a bounded decimal cardinality,
                    // unlike ordinary value expressions' typed VM literals.
                    // Only a Name actually replaced by a verified iterator can
                    // take this representation conversion; source literals and
                    // runtime-shadowed names keep the ordinary type boundary.
                    if was_name && let Some((value, _)) = generation::integer(length) {
                        self.budget.spend(0, 32, length.span)?;
                        length.kind = s::ExprKind::Literal(s::Literal::Integer(value.to_string()));
                    }
                }
            }
            s::TypeKind::Function { params, result }
            | s::TypeKind::ForeignFunction { params, result, .. } => {
                for parameter in params {
                    self.generation_type(parameter, depth + 1)?;
                }
                self.generation_type(result, depth + 1)?;
            }
            s::TypeKind::Error { error, result } => {
                self.generation_type(error, depth + 1)?;
                self.generation_type(result, depth + 1)?;
            }
            s::TypeKind::MetaEval(value) => self.expression(value, depth + 1)?,
            // The ordinary Type/Syntax boundary remains responsible for this
            // generator; a scalar iteration cannot grant its execution.
            s::TypeKind::MetaBlock(_) => {}
        }
        Ok(())
    }

    fn generation_argument(
        &mut self,
        argument: &mut s::GenericArg,
        depth: usize,
    ) -> CompileResult<()> {
        match argument {
            s::GenericArg::Type(ty) => self.generation_type(ty, depth),
            s::GenericArg::Const(value) => self.expression(value, depth),
        }
    }

    pub(super) fn generation_attributes(
        &mut self,
        attributes: &mut [s::Attribute],
        depth: usize,
    ) -> CompileResult<()> {
        for attribute in attributes {
            self.budget.node(attribute.span, depth)?;
            for value in &mut attribute.arguments {
                self.expression(value, depth + 1)?;
            }
        }
        Ok(())
    }

    fn generation_contract(
        &mut self,
        term: &mut s::ContractTerm,
        depth: usize,
    ) -> CompileResult<()> {
        self.budget.node(term.span, depth)?;
        for argument in &mut term.type_args {
            self.generation_argument(argument, depth + 1)?;
        }
        for argument in &mut term.arguments {
            self.generation_contract(argument, depth + 1)?;
        }
        if let Some(index) = &mut term.index {
            self.generation_contract(index, depth + 1)?;
        }
        Ok(())
    }

    pub(super) fn generation_signature(
        &mut self,
        function: &mut s::Function,
        depth: usize,
    ) -> CompileResult<()> {
        for parameter in &mut function.generics {
            self.budget.node(parameter.span, depth)?;
            match &mut parameter.kind {
                s::GenericParamKind::Type(bounds) => {
                    for ty in bounds {
                        self.generation_type(ty, depth + 1)?;
                    }
                }
                s::GenericParamKind::Const(ty) => self.generation_type(ty, depth + 1)?,
                s::GenericParamKind::Lifetime => {}
            }
        }
        for parameter in &mut function.params {
            self.generation_attributes(&mut parameter.attributes, depth)?;
            self.generation_type(&mut parameter.ty, depth)?;
        }
        if let Some(ty) = &mut function.return_type {
            self.generation_type(ty, depth)?;
        }
        for bound in &mut function.where_clause {
            self.budget.node(bound.span, depth)?;
            if let s::BoundSubject::Type(ty) = &mut bound.subject {
                self.generation_type(ty, depth + 1)?;
            }
            for ty in &mut bound.bounds {
                self.generation_type(ty, depth + 1)?;
            }
        }
        for clause in &mut function.contract {
            self.budget.node(clause.span, depth)?;
            for term in &mut clause.terms {
                self.generation_contract(term, depth + 1)?;
            }
        }
        Ok(())
    }
}
