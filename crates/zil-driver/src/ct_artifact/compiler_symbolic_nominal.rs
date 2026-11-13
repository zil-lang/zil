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
    pub(super) fn domain(&self) -> CtTy {
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
    schema: CtTy,
}

fn push_scratch<T>(budget: &mut Budget<'_>, rows: &mut Vec<T>, row: T, span: Span) -> R<()> {
    if rows.len() == rows.capacity() {
        let capacity = rows
            .len()
            .checked_mul(2)
            .and_then(|n| n.checked_add(1))
            .map(|n| n.max(4))
            .ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Ct,
                    "CT abstract nominal scratch capacity overflow",
                    Some(span),
                )]
            })?;
        let bytes = capacity
            .checked_mul(std::mem::size_of::<T>())
            .ok_or_else(|| {
                vec![Diagnostic::limit(
                    Phase::Ct,
                    "CT abstract nominal scratch storage overflow",
                    Some(span),
                )]
            })?;
        Budget::take(
            &mut budget.bytes,
            bytes,
            span,
            "CT abstract nominal scratch backing exhausted",
        )?;
        budget.work(rows.len(), span)?;
        rows.reserve_exact(capacity - rows.len());
    }
    rows.push(row);
    Ok(())
}

impl<'a> Compiler<'a> {
    pub(super) fn runtime_abstract_nominal_type(
        &mut self,
        schema: CtRecordSchemaId,
        span: Span,
        depth: usize,
    ) -> R<Option<s::TypeRef>> {
        self.runtime_abstract_nominal(&CtTy::Record(schema), span, depth)
    }
    pub(super) fn runtime_abstract_enum_type(
        &mut self,
        schema: CtEnumSchemaId,
        span: Span,
        depth: usize,
    ) -> R<Option<s::TypeRef>> {
        self.runtime_abstract_nominal(&CtTy::Enum(schema), span, depth)
    }
    fn runtime_abstract_nominal(
        &mut self,
        schema: &CtTy,
        span: Span,
        depth: usize,
    ) -> R<Option<s::TypeRef>> {
        let mut selected = None;
        for (index, row) in self.abstract_nominals.iter().enumerate() {
            self.budget.work(1, span)?;
            if &row.schema == schema {
                selected = Some(index);
                break;
            }
        }
        let Some(index) = selected else {
            return Ok(None);
        };
        let row = &self.abstract_nominals[index];
        Self::abstract_arguments_cost(&mut self.budget, &row.arguments, span)?;
        self.budget.work(32, span)?;
        let definition = row.definition.clone();
        let arguments = row.arguments.clone();
        let (_, item) = self.metadata_declaration(&definition, span)?;
        let name = match (&item.kind, schema) {
            (s::ItemKind::Aggregate(declaration), CtTy::Record(_)) => &declaration.name,
            (s::ItemKind::Enum(declaration), CtTy::Enum(_)) => &declaration.name,
            _ => {
                return Err(error(
                    span,
                    "computed abstract nominal lost its actual declaration",
                ));
            }
        };
        self.budget.text(name, span)?;
        Budget::take(
            &mut self.budget.bytes,
            arguments
                .len()
                .saturating_mul(std::mem::size_of::<s::GenericArg>()),
            span,
            "runtime CT nominal argument AST exhausted",
        )?;
        let mut args = Vec::with_capacity(arguments.len());
        for argument in arguments {
            self.budget.node(span)?;
            args.push(match argument {
                Argument::Type(ty) => {
                    s::GenericArg::Type(self.runtime_computed_type(&ty, span, depth + 1)?)
                }
                Argument::Lifetime(definition, ordinal) => s::GenericArg::Lifetime(
                    self.runtime_original_binder(&definition, ordinal, true, span)?,
                ),
                Argument::Scalar(ScalarArgument::Parameter {
                    definition,
                    ordinal,
                    ..
                }) => {
                    let name = self.runtime_original_binder(&definition, ordinal, false, span)?;
                    s::GenericArg::Const(s::Expr {
                        semantic: Default::default(),
                        span,
                        kind: s::ExprKind::Name(name),
                    })
                }
                Argument::Scalar(ScalarArgument::Constant(value)) => {
                    Budget::take(
                        &mut self.budget.bytes,
                        40,
                        span,
                        "runtime CT scalar argument text exhausted",
                    )?;
                    let kind = match value {
                        Constant::Bool(value) => s::ExprKind::Literal(s::Literal::Bool(value)),
                        Constant::U8(value) => {
                            s::ExprKind::Literal(s::Literal::Integer(format!("{value}u8")))
                        }
                        Constant::U16(value) => {
                            s::ExprKind::Literal(s::Literal::Integer(format!("{value}u16")))
                        }
                        Constant::U32(value) => {
                            s::ExprKind::Literal(s::Literal::Integer(format!("{value}u32")))
                        }
                        Constant::I32(value) => {
                            s::ExprKind::Literal(s::Literal::Integer(format!("{value}i32")))
                        }
                        Constant::I8(value) => {
                            s::ExprKind::Literal(s::Literal::Integer(format!("{value}i8")))
                        }
                        Constant::I16(value) => {
                            s::ExprKind::Literal(s::Literal::Integer(format!("{value}i16")))
                        }
                        Constant::Isize(value) => {
                            s::ExprKind::Literal(s::Literal::Integer(format!("{value}isize")))
                        }
                        Constant::I64(value) => {
                            s::ExprKind::Literal(s::Literal::Integer(format!("{value}i64")))
                        }
                        Constant::U64(value) => {
                            s::ExprKind::Literal(s::Literal::Integer(format!("{value}u64")))
                        }
                        Constant::Usize(value) => {
                            s::ExprKind::Literal(s::Literal::Integer(format!("{value}usize")))
                        }
                        _ => {
                            return Err(error(
                                span,
                                "computed CT nominal has an unsupported scalar source argument",
                            ));
                        }
                    };
                    s::GenericArg::Const(s::Expr {
                        semantic: Default::default(),
                        span,
                        kind,
                    })
                }
            });
        }
        Ok(Some(s::TypeRef {
            span,
            kind: s::TypeKind::Named {
                path: vec![name.clone()],
                args,
            },
        }))
    }
    fn runtime_original_binder(
        &mut self,
        definition: &CtSymbolId,
        ordinal: u32,
        lifetime: bool,
        span: Span,
    ) -> R<String> {
        let (_, item) = self.metadata_declaration(definition, span)?;
        let parameters = match &item.kind {
            s::ItemKind::Function(function) => &function.generics,
            s::ItemKind::Aggregate(host) => &host.generics,
            _ => {
                return Err(error(
                    span,
                    "computed CT argument owner has no original binders",
                ));
            }
        };
        let parameter = parameters
            .get(ordinal as usize)
            .ok_or_else(|| error(span, "computed CT argument original ordinal absent"))?;
        if lifetime != matches!(parameter.kind, s::GenericParamKind::Lifetime)
            || (!lifetime && !matches!(parameter.kind, s::GenericParamKind::Const(_)))
        {
            return Err(error(
                span,
                "computed CT argument original binder kind differs",
            ));
        }
        self.budget.text(&parameter.name, span)?;
        Ok(parameter.name.clone())
    }
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

    pub(super) fn abstract_scalar(
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

    pub(super) fn abstract_life(&mut self, name: &str, span: Span) -> R<(CtSymbolId, u32)> {
        for (bound, definition, ordinal) in &self.abstract_lives {
            self.budget.work(name.len().min(bound.len()) + 32, span)?;
            if bound == name {
                return Ok((definition.clone(), *ordinal));
            }
        }
        Err(error(
            span,
            "CT abstract life is not this declaration's binder",
        ))
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

    /// A Type expression needs checked symbolic actual kinds and capabilities,
    /// without allocating an executable value schema for the named declaration.
    pub(super) fn check_abstract_type_application(
        &mut self,
        module: usize,
        item: &s::Item,
        args: &[s::GenericArg],
        depth: usize,
        span: Span,
    ) -> R<()> {
        self.budget.depth(depth, span)?;
        let parameters = match &item.kind {
            s::ItemKind::Aggregate(a) => &a.generics,
            s::ItemKind::Enum(e) => &e.generics,
            _ => {
                return Err(error(
                    span,
                    "CT Type application requires a nominal declaration",
                ));
            }
        };
        if args.len() != parameters.len() {
            return Err(error(span, "CT abstract Type argument arity differs"));
        }
        for (parameter, argument) in parameters.iter().zip(args) {
            self.budget.node(span)?;
            match (&parameter.kind, argument) {
                (s::GenericParamKind::Type(_), s::GenericArg::Type(ty)) => {
                    self.generic_names(std::slice::from_ref(parameter), &[], span)?;
                    let actual = self.ty(module, ty, depth + 1)?;
                    self.ordinary_copy(&actual, depth + 1, span)?;
                }
                (s::GenericParamKind::Const(_), argument) => {
                    self.abstract_scalar(parameter, argument, span)?;
                }
                (s::GenericParamKind::Lifetime, s::GenericArg::Lifetime(name)) => {
                    let mut found = false;
                    for (bound, _, _) in &self.abstract_lives {
                        self.budget
                            .work(name.len().min(bound.len()).saturating_add(1), span)?;
                        found |= bound == name;
                    }
                    if !found {
                        return Err(error(
                            span,
                            "CT abstract Type life actual is not an authentic binder",
                        ));
                    }
                }
                _ => return Err(error(span, "CT abstract Type actual kind differs")),
            }
        }
        Ok(())
    }

    pub(super) fn abstract_enum_header(
        &mut self,
        schema: CtEnumSchemaId,
        definition: &CtSymbolId,
        arity: usize,
        span: Span,
    ) -> R<bool> {
        if !self.checking_generics {
            return Ok(false);
        }
        for row in &self.abstract_nominals {
            self.budget.work(32, span)?;
            if row.schema == CtTy::Enum(schema) {
                Self::abstract_arguments_cost(&mut self.budget, &row.arguments, span)?;
                return Ok(row.definition == *definition && row.arguments.len() == arity);
            }
        }
        Ok(false)
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
        let parameters = match &item.kind {
            s::ItemKind::Aggregate(declaration) if declaration.kind == s::AggregateKind::Struct => {
                &declaration.generics
            }
            s::ItemKind::Enum(declaration) => &declaration.generics,
            _ => {
                return Err(forbidden(
                    span,
                    "CT abstract nominal requires an ordinary pure struct or enum",
                ));
            }
        };
        if args.len() != parameters.len() {
            return Err(error(span, "CT abstract nominal argument arity differs"));
        }
        let definition = self.item_symbol(owner, item)?;
        let mut arguments = Vec::new();
        let mut data = Vec::new();
        let mut scalars = Vec::new();
        let mut lives = Vec::new();
        for (ordinal, (parameter, actual)) in parameters.iter().zip(args).enumerate() {
            self.budget.node(span)?;
            self.budget.text(&parameter.name, span)?;
            for prior in &parameters[..ordinal] {
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
                    push_scratch(
                        &mut self.budget,
                        &mut data,
                        (parameter.name.clone(), ty.clone()),
                        span,
                    )?;
                    Argument::Type(ty)
                }
                (s::GenericParamKind::Const(_), actual) => {
                    let value = self.abstract_scalar(parameter, actual, span)?;
                    self.budget.work(32, span)?;
                    push_scratch(
                        &mut self.budget,
                        &mut scalars,
                        (parameter.name.clone(), value.clone()),
                        span,
                    )?;
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
                    push_scratch(
                        &mut self.budget,
                        &mut lives,
                        (parameter.name.clone(), definition.clone(), ordinal),
                        span,
                    )?;
                    Argument::Lifetime(definition, ordinal)
                }
                _ => return Err(error(span, "CT abstract nominal actual kind differs")),
            };
            push_scratch(&mut self.budget, &mut arguments, value, span)?;
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
                return Ok(old.schema.clone());
            }
        }
        self.budget.node(span)?;
        Self::abstract_arguments_cost(&mut self.budget, &arguments, span)?;
        let schema = match &item.kind {
            s::ItemKind::Aggregate(declaration) => {
                let id = CtRecordSchemaId(
                    u32::try_from(self.records.len())
                        .map_err(|_| error(span, "CT abstract record schema ID overflow"))?,
                );
                push_scratch(
                    &mut self.budget,
                    &mut self.records,
                    CtRecordSchema {
                        name: definition.clone(),
                        arguments: Vec::new(),
                        fields: Vec::new(),
                        life_context: None,
                        origin: CtSchemaOrigin::UserPure,
                    },
                    span,
                )?;
                push_scratch(
                    &mut self.budget,
                    &mut self.record_sources,
                    (owner, declaration),
                    span,
                )?;
                push_scratch(
                    &mut self.budget,
                    &mut self.record_arguments,
                    Vec::new(),
                    span,
                )?;
                CtTy::Record(id)
            }
            s::ItemKind::Enum(declaration) => {
                let id = CtEnumSchemaId(
                    u32::try_from(self.enums.len())
                        .map_err(|_| error(span, "CT abstract enum schema ID overflow"))?,
                );
                push_scratch(
                    &mut self.budget,
                    &mut self.enums,
                    CtEnumSchema {
                        name: definition.clone(),
                        arguments: Vec::new(),
                        variants: Vec::new(),
                        life_context: None,
                        origin: CtSchemaOrigin::UserPure,
                    },
                    span,
                )?;
                push_scratch(
                    &mut self.budget,
                    &mut self.enum_sources,
                    (owner, declaration),
                    span,
                )?;
                CtTy::Enum(id)
            }
            _ => unreachable!(),
        };
        push_scratch(
            &mut self.budget,
            &mut self.abstract_nominals,
            Nominal {
                definition,
                arguments,
                schema: schema.clone(),
            },
            span,
        )?;
        push_scratch(
            &mut self.budget,
            &mut self.active_abstract_nominals,
            schema.clone(),
            span,
        )?;
        let old_data = std::mem::replace(&mut self.bindings, data);
        let old_meta = std::mem::take(&mut self.metadata_bindings);
        let old_scalars = std::mem::replace(&mut self.abstract_scalars, scalars);
        let old_lives = std::mem::replace(&mut self.abstract_lives, lives);
        let result = (|| {
            match (&item.kind, &schema) {
                (s::ItemKind::Aggregate(declaration), CtTy::Record(id)) => {
                    let mut fields = Vec::new();
                    for field in &declaration.fields {
                        self.budget.node(field.span)?;
                        if field.manages {
                            return Err(forbidden(
                                field.span,
                                "CT abstract data cannot manage a runtime resource",
                            ));
                        }
                        let field_type = self.ty(owner, &field.ty, depth + 1)?;
                        push_scratch(&mut self.budget, &mut fields, field_type, field.span)?;
                    }
                    self.records[id.0 as usize].fields = fields;
                }
                (s::ItemKind::Enum(declaration), CtTy::Enum(id)) => {
                    let mut variants = Vec::new();
                    for variant in &declaration.variants {
                        self.budget.node(variant.span)?;
                        let mut fields = Vec::new();
                        match &variant.fields {
                            s::VariantFields::Unit => {}
                            s::VariantFields::Tuple(fs) => {
                                for field in fs {
                                    self.budget.node(field.span)?;
                                    if field.manages {
                                        return Err(forbidden(
                                            field.span,
                                            "CT abstract enum cannot manage a runtime resource",
                                        ));
                                    }
                                    let field_type = self.ty(owner, &field.ty, depth + 1)?;
                                    push_scratch(
                                        &mut self.budget,
                                        &mut fields,
                                        field_type,
                                        field.span,
                                    )?;
                                }
                            }
                            s::VariantFields::Record(fs) => {
                                for field in fs {
                                    self.budget.node(field.span)?;
                                    if field.manages {
                                        return Err(forbidden(
                                            field.span,
                                            "CT abstract enum cannot manage a runtime resource",
                                        ));
                                    }
                                    let field_type = self.ty(owner, &field.ty, depth + 1)?;
                                    push_scratch(
                                        &mut self.budget,
                                        &mut fields,
                                        field_type,
                                        field.span,
                                    )?;
                                }
                            }
                        }
                        push_scratch(&mut self.budget, &mut variants, fields, variant.span)?;
                    }
                    self.enums[id.0 as usize].variants = variants;
                }
                _ => unreachable!(),
            }
            Ok(schema)
        })();
        self.bindings = old_data;
        self.metadata_bindings = old_meta;
        self.abstract_scalars = old_scalars;
        self.abstract_lives = old_lives;
        self.active_abstract_nominals.pop();
        result
    }
}
