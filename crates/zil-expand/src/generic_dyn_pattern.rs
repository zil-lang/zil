//! Original nominal impl headers retain formal occurrences, not class indices.
use super::*;
use zil_ir::{DynConstPattern, DynPatternActual, DynSelfPattern, DynTypePattern};
type Nominals<'a> = BTreeMap<String, (LifeDeclarationId, &'a [GenericParam])>;
struct Recipe<'a, 'b, 'l> {
    nominal: &'a Nominals<'a>,
    formals: &'a [GenericParam],
    budget: &'b mut Budget<'l>,
}
impl Recipe<'_, '_, '_> {
    fn parameter(&mut self, name: &str, kind: u8, span: Span) -> R<u32> {
        for (ordinal, formal) in self.formals.iter().enumerate() {
            self.budget.node(span, 0)?;
            self.budget.text(&formal.name, span)?;
            self.budget.text(name, span)?;
            let same_kind = matches!(
                (&formal.kind, kind),
                (GenericParamKind::Type(_), 0)
                    | (GenericParamKind::Const(_), 1)
                    | (GenericParamKind::Lifetime, 2)
            );
            if same_kind && formal.name == name {
                return u32::try_from(ordinal)
                    .map_err(|_| error(span, "dyn impl formal ordinal exhausted"));
            }
        }
        Err(error(
            span,
            "dyn impl recipe occurrence has no formal of the required original kind",
        ))
    }
    fn constant(
        &mut self,
        value: &Expr,
        domain: &GenericParam,
        depth: usize,
    ) -> R<DynConstPattern> {
        self.budget.node(value.span, depth)?;
        if let ExprKind::Name(name) = &value.kind {
            self.budget.identity_work(self.formals.len(), value.span)?;
            if self
                .formals
                .iter()
                .any(|p| p.name == *name && matches!(p.kind, GenericParamKind::Const(_)))
            {
                return Ok(DynConstPattern::Parameter(
                    self.parameter(name, 1, value.span)?,
                ));
            }
        }
        let value = consts::carrier(value, self.budget)?;
        let value = consts::normalize(domain, &value, self.budget)?;
        Ok(DynConstPattern::Value(consts::normalized_value(
            &value,
            self.budget,
        )?))
    }
    fn nominal(&mut self, ty: &TypeRef, depth: usize) -> R<DynSelfPattern> {
        self.budget.node(ty.span, depth)?;
        let TypeKind::Named { path, args } = &grouped(ty).kind else {
            return Err(unsupported(
                ty.span,
                "dyn impl Self recipe requires an actual nominal header",
            ));
        };
        let [name] = path.as_slice() else {
            return Err(error(
                ty.span,
                "dyn impl nominal recipe is not source-resolved",
            ));
        };
        self.budget.text(name, ty.span)?;
        let (declaration, parameters) = self.nominal.get(name).copied().ok_or_else(|| {
            error(
                ty.span,
                "dyn impl Self recipe has no actual nominal declaration origin",
            )
        })?;
        if parameters.len() != args.len() {
            return Err(error(
                ty.span,
                "dyn impl recipe actual count differs from its original nominal declaration",
            ));
        }
        let mut arguments = Vec::new();
        for (formal, actual) in parameters.iter().zip(args) {
            self.budget.node(ty.span, depth + 1)?;
            arguments.push(match (&formal.kind, actual) {
                (GenericParamKind::Lifetime, GenericArg::Lifetime(name)) => {
                    DynPatternActual::LifeParameter(self.parameter(name, 2, ty.span)?)
                }
                (GenericParamKind::Type(_), GenericArg::Type(ty)) => {
                    DynPatternActual::Type(self.ty(ty, depth + 1)?)
                }
                (GenericParamKind::Const(_), GenericArg::Const(value)) => {
                    DynPatternActual::Const(self.constant(value, formal, depth + 1)?)
                }
                _ => {
                    return Err(error(
                        ty.span,
                        "dyn impl recipe actual kind differs from its original nominal declaration",
                    ));
                }
            });
        }
        Ok(DynSelfPattern {
            declaration,
            arguments,
        })
    }
    fn ty(&mut self, ty: &TypeRef, depth: usize) -> R<DynTypePattern> {
        self.budget.node(ty.span, depth)?;
        match &grouped(ty).kind {
            TypeKind::Named { path, args } if path.len() == 1 => {
                let name = &path[0];
                if args.is_empty()
                    && self
                        .formals
                        .iter()
                        .any(|p| p.name == *name && matches!(p.kind, GenericParamKind::Type(_)))
                {
                    return Ok(DynTypePattern::Parameter(self.parameter(name, 0, ty.span)?));
                }
                if self.nominal.contains_key(name) {
                    return Ok(DynTypePattern::Nominal(Box::new(
                        self.nominal(ty, depth + 1)?,
                    )));
                }
                if matches!(name.as_str(), "Managed" | "Desc" | "Store") {
                    let [GenericArg::Type(inner)] = args.as_slice() else {
                        return Err(error(
                            ty.span,
                            "dyn structural Self recipe has wrong original type arguments",
                        ));
                    };
                    let inner = Box::new(self.ty(inner, depth + 1)?);
                    return Ok(match name.as_str() {
                        "Managed" => DynTypePattern::Managed(inner),
                        "Desc" => DynTypePattern::Desc(inner),
                        _ => DynTypePattern::OwnedStore(inner),
                    });
                }
                Ok(DynTypePattern::Concrete(lower_type(ty, self.budget)?))
            }
            TypeKind::Reference {
                mutable,
                lifetime,
                inner,
            } => Ok(DynTypePattern::Address {
                mutable: *mutable,
                lifetime: lifetime
                    .as_ref()
                    .map(|name| self.parameter(name, 2, ty.span))
                    .transpose()?,
                inner: Box::new(self.ty(inner, depth + 1)?),
            }),
            TypeKind::Tuple(fields) => {
                let mut result = Vec::new();
                for field in fields {
                    result.push(self.ty(field, depth + 1)?);
                }
                Ok(DynTypePattern::Tuple(result))
            }
            TypeKind::Array {
                element,
                length: None,
            } => Ok(DynTypePattern::Slice(Box::new(
                self.ty(element, depth + 1)?,
            ))),
            TypeKind::Array {
                element,
                length: Some(value),
            } => {
                let domain = GenericParam {
                    name: "array_length".into(),
                    kind: GenericParamKind::Const(named("usize", ty.span)),
                    span: ty.span,
                };
                Ok(DynTypePattern::Array {
                    element: Box::new(self.ty(element, depth + 1)?),
                    length: self.constant(value, &domain, depth + 1)?,
                })
            }
            TypeKind::Error { error: err, result } => Ok(DynTypePattern::ErrorUnion {
                error: Box::new(self.ty(err, depth + 1)?),
                result: Box::new(self.ty(result, depth + 1)?),
            }),
            _ => Err(unsupported(
                ty.span,
                "dyn impl structural recipe needs its complete semantic constructor",
            )),
        }
    }
}
pub(super) fn collect(
    module: &Module,
    origins: &origins::Origins,
    budget: &mut Budget<'_>,
) -> R<BTreeMap<origins::DeclId, DynSelfPattern>> {
    let mut nominals = Nominals::new();
    for (ordinal, item) in module.items.iter().enumerate() {
        budget.node(item.span, 0)?;
        let (name, parameters) = match &item.kind {
            ItemKind::Aggregate(a) => (&a.name, a.generics.as_slice()),
            ItemKind::Enum(e) => (&e.name, e.generics.as_slice()),
            _ => continue,
        };
        budget.text(name, item.span)?;
        let origin = origins.get(ordinal, item.span)?.declaration;
        let declaration = declaration_id(origin, origins, budget, item.span)?;
        nominals.insert(name.clone(), (declaration, parameters));
    }
    let mut result = BTreeMap::new();
    for (ordinal, item) in module.items.iter().enumerate() {
        budget.node(item.span, 0)?;
        let ItemKind::Impl(implementation) = &item.kind else {
            continue;
        };
        if implementation.trait_type.is_none() || implementation.generics.is_empty() {
            continue;
        }
        let origin = origins.get(ordinal, item.span)?.declaration;
        let pattern = Recipe {
            nominal: &nominals,
            formals: &implementation.generics,
            budget,
        }
        .nominal(&implementation.target, 0)?;
        result.insert(origin, pattern);
    }
    Ok(result)
}

pub(super) fn pay(
    root: &DynSelfPattern,
    budget: &mut Budget<'_>,
    span: Span,
    depth: usize,
) -> R<()> {
    fn ty(value: &DynTypePattern, b: &mut Budget<'_>, s: Span, d: usize) -> R<()> {
        b.node(s, d)?;
        match value {
            DynTypePattern::Concrete(t) => crate::stored_types::pay_type(t, b, s, d + 1)?,
            DynTypePattern::Parameter(_) => {}
            DynTypePattern::Nominal(p) => pay(p, b, s, d + 1)?,
            DynTypePattern::Address { inner, .. }
            | DynTypePattern::Slice(inner)
            | DynTypePattern::Managed(inner)
            | DynTypePattern::Desc(inner)
            | DynTypePattern::OwnedStore(inner) => ty(inner, b, s, d + 1)?,
            DynTypePattern::Tuple(ts) => {
                for t in ts {
                    ty(t, b, s, d + 1)?;
                }
            }
            DynTypePattern::Array { element, length } => {
                constant(length, b, s, d + 1)?;
                ty(element, b, s, d + 1)?;
            }
            DynTypePattern::ErrorUnion { error, result } => {
                ty(error, b, s, d + 1)?;
                ty(result, b, s, d + 1)?;
            }
        }
        Ok(())
    }
    fn constant(value: &DynConstPattern, b: &mut Budget<'_>, s: Span, d: usize) -> R<()> {
        b.node(s, d)?;
        if let DynConstPattern::Value(zil_ir::Constant::WideString(units)) = value {
            b.identity_work(units.len(), s)?;
        }
        Ok(())
    }
    budget.node(span, depth)?;
    for actual in &root.arguments {
        budget.node(span, depth + 1)?;
        match actual {
            DynPatternActual::Type(t) => ty(t, budget, span, depth + 2)?,
            DynPatternActual::Const(v) => constant(v, budget, span, depth + 2)?,
            DynPatternActual::LifeParameter(_) => {}
        }
    }
    Ok(())
}
