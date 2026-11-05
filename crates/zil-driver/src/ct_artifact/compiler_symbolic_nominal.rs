//! Producer-only abstract nominal identities for declaration checking. These
//! are neither Type graphs nor numeric substitutes and never leave scratch.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ScalarArgument {
    Parameter {
        definition: CtSymbolId,
        ordinal: u32,
        domain: CtScalarTy,
    },
    Constant(Constant),
}
impl ScalarArgument {
    fn domain(&self) -> CtTy {
        match self {
            Self::Parameter { domain, .. } => CtTy::Scalar(*domain),
            Self::Constant(value) => expr::scalar_constant(value),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Argument {
    Type(CtTy),
    Scalar(ScalarArgument),
    Lifetime(CtSymbolId, u32),
}
pub(super) struct Nominal {
    definition: CtSymbolId,
    arguments: Vec<Argument>,
    schema: CtRecordSchemaId,
}

impl<'a> Compiler<'a> {
    pub(super) fn prepare_abstract_scope(&mut self, declaration: usize) -> R<()> {
        self.abstract_scalars.clear();
        self.abstract_lives.clear();
        let d = self.declarations[declaration];
        if !Self::mixed_parameters(&d.function.generics) {
            return Ok(());
        }
        let definition = self.item_symbol(d.module, d.item)?;
        self.source_life_scope = self.source_life_scope(d.module, &definition, d.function.span)?;
        for (ordinal, parameter) in d.function.generics.iter().enumerate() {
            self.budget.node(parameter.span)?;
            self.budget.text(&parameter.name, parameter.span)?;
            self.budget.work(32, parameter.span)?;
            let ordinal = u32::try_from(ordinal)
                .map_err(|_| error(parameter.span, "CT abstract ordinal overflow"))?;
            match &parameter.kind {
                s::GenericParamKind::Const(ty) => {
                    let domain = self.scalar_argument_type(ty, parameter.span)?;
                    Budget::take(
                        &mut self.budget.bytes,
                        std::mem::size_of::<ScalarArgument>() + 24,
                        parameter.span,
                        "CT abstract scalar bindings exhausted",
                    )?;
                    self.abstract_scalars.push((
                        parameter.name.clone(),
                        ScalarArgument::Parameter {
                            definition: definition.clone(),
                            ordinal,
                            domain,
                        },
                    ));
                }
                s::GenericParamKind::Lifetime => {
                    Budget::take(
                        &mut self.budget.bytes,
                        64,
                        parameter.span,
                        "CT abstract life bindings exhausted",
                    )?;
                    self.abstract_lives
                        .push((parameter.name.clone(), definition.clone(), ordinal));
                }
                s::GenericParamKind::Type(_) => {}
            }
        }
        Ok(())
    }

    fn abstract_scalar(
        &mut self,
        parameter: &s::GenericParam,
        actual: &s::GenericArg,
        span: Span,
    ) -> R<ScalarArgument> {
        let s::GenericParamKind::Const(ty) = &parameter.kind else {
            unreachable!()
        };
        let domain = self.scalar_argument_type(ty, span)?;
        let name = match actual {
            s::GenericArg::Type(s::TypeRef {
                kind: s::TypeKind::Named { path, args },
                ..
            }) if path.len() == 1 && args.is_empty() => Some(path[0].as_str()),
            s::GenericArg::Const(s::Expr {
                kind: s::ExprKind::Name(name),
                ..
            }) => Some(name.as_str()),
            _ => None,
        };
        if let Some(name) = name {
            for (bound, value) in &self.abstract_scalars {
                self.budget.work(name.len().min(bound.len()) + 32, span)?;
                if name == bound {
                    if value.domain() != CtTy::Scalar(domain) {
                        return Err(error(span, "CT abstract scalar argument domain differs"));
                    }
                    return Ok(value.clone());
                }
            }
        }
        self.source_scalar_argument(parameter, actual, span)
            .map(ScalarArgument::Constant)
    }

    fn abstract_arguments_cost(
        budget: &mut Budget<'_>,
        arguments: &[Argument],
        span: Span,
    ) -> R<()> {
        budget.work(arguments.len(), span)?;
        Budget::take(
            &mut budget.bytes,
            arguments
                .len()
                .saturating_mul(std::mem::size_of::<Argument>()),
            span,
            "CT abstract nominal arguments exhausted",
        )?;
        for argument in arguments {
            match argument {
                Argument::Type(ty) => budget.copy_ty(ty, 0, span)?,
                Argument::Scalar(_) | Argument::Lifetime(..) => budget.work(32, span)?,
            }
        }
        Ok(())
    }

    pub(super) fn abstract_nominal(
        &mut self,
        module: usize,
        owner: usize,
        item: &'a s::Item,
        args: &[s::GenericArg],
        depth: usize,
        span: Span,
    ) -> R<CtTy> {
        self.budget.depth(depth, span)?;
        let s::ItemKind::Aggregate(declaration) = &item.kind else {
            unreachable!()
        };
        if declaration.kind != s::AggregateKind::Struct {
            return Err(forbidden(
                span,
                "CT abstract data requires an ordinary pure struct",
            ));
        }
        if args.len() != declaration.generics.len() {
            return Err(error(span, "CT abstract nominal argument arity differs"));
        }
        let definition = self.item_symbol(owner, item)?;
        let mut arguments = Vec::new();
        let mut data = Vec::new();
        let mut scalars = Vec::new();
        let mut lives = Vec::new();
        for (ordinal, (parameter, actual)) in declaration.generics.iter().zip(args).enumerate() {
            self.budget.node(span)?;
            self.budget.text(&parameter.name, span)?;
            for prior in &declaration.generics[..ordinal] {
                self.budget
                    .work(parameter.name.len().min(prior.name.len()) + 1, span)?;
                if prior.name == parameter.name {
                    return Err(error(span, "duplicate CT abstract nominal parameter"));
                }
            }
            let value = match (&parameter.kind, actual) {
                (s::GenericParamKind::Type(_), s::GenericArg::Type(ty)) => {
                    self.generic_names(std::slice::from_ref(parameter), &[], span)?;
                    let ty = self.ty(module, ty, depth + 1)?;
                    self.ordinary_copy(&ty, depth + 1, span)?;
                    self.budget.copy_ty(&ty, 0, span)?;
                    data.push((parameter.name.clone(), ty.clone()));
                    Argument::Type(ty)
                }
                (s::GenericParamKind::Const(_), actual) => {
                    let value = self.abstract_scalar(parameter, actual, span)?;
                    self.budget.work(32, span)?;
                    scalars.push((parameter.name.clone(), value.clone()));
                    Argument::Scalar(value)
                }
                (s::GenericParamKind::Lifetime, s::GenericArg::Lifetime(name)) => {
                    let mut found = None;
                    for (bound, definition, ordinal) in &self.abstract_lives {
                        self.budget.work(name.len().min(bound.len()) + 32, span)?;
                        if bound == name {
                            found = Some((definition.clone(), *ordinal));
                        }
                    }
                    let (definition, ordinal) = found.ok_or_else(|| {
                        error(span, "CT abstract life is not this declaration's binder")
                    })?;
                    self.budget.work(32, span)?;
                    lives.push((parameter.name.clone(), definition.clone(), ordinal));
                    Argument::Lifetime(definition, ordinal)
                }
                _ => return Err(error(span, "CT abstract nominal actual kind differs")),
            };
            Budget::take(
                &mut self.budget.bytes,
                std::mem::size_of::<Argument>(),
                span,
                "CT abstract nominal identity bytes exhausted",
            )?;
            arguments.push(value);
        }
        for old in &self.abstract_nominals {
            Self::abstract_arguments_cost(&mut self.budget, &old.arguments, span)?;
            Self::abstract_arguments_cost(&mut self.budget, &arguments, span)?;
            if old.definition == definition && old.arguments == arguments {
                self.budget
                    .work(self.active_abstract_nominals.len(), span)?;
                if self.active_abstract_nominals.contains(&old.schema) {
                    return Err(vec![Diagnostic::error(
                        Phase::Ct,
                        "E-CT-PHASE-CYCLE",
                        Some(span),
                        "CT abstract nominal depends on its unfinished field shape",
                    )]);
                }
                return Ok(CtTy::Record(old.schema));
            }
        }
        self.budget.node(span)?;
        Self::abstract_arguments_cost(&mut self.budget, &arguments, span)?;
        let schema = CtRecordSchemaId(
            u32::try_from(self.records.len())
                .map_err(|_| error(span, "CT abstract schema ID overflow"))?,
        );
        self.records.push(CtRecordSchema {
            name: definition.clone(),
            arguments: Vec::new(),
            fields: Vec::new(),
            life_context: None,
            origin: CtSchemaOrigin::UserPure,
        });
        self.record_sources.push((owner, declaration));
        self.record_arguments.push(Vec::new());
        self.abstract_nominals.push(Nominal {
            definition,
            arguments,
            schema,
        });
        self.active_abstract_nominals.push(schema);
        let old_data = std::mem::replace(&mut self.bindings, data);
        let old_meta = std::mem::take(&mut self.metadata_bindings);
        let old_scalars = std::mem::replace(&mut self.abstract_scalars, scalars);
        let old_lives = std::mem::replace(&mut self.abstract_lives, lives);
        let result = (|| {
            let mut fields = Vec::new();
            for field in &declaration.fields {
                self.budget.node(field.span)?;
                if field.manages {
                    return Err(forbidden(
                        field.span,
                        "CT abstract data cannot manage a runtime resource",
                    ));
                }
                fields.push(self.ty(owner, &field.ty, depth + 1)?);
            }
            self.records[schema.0 as usize].fields = fields;
            Ok(CtTy::Record(schema))
        })();
        self.bindings = old_data;
        self.metadata_bindings = old_meta;
        self.abstract_scalars = old_scalars;
        self.abstract_lives = old_lives;
        self.active_abstract_nominals.pop();
        result
    }
}
