//! Dyn applications retain every declaration actual in original formal order.
//! Ordinary trait solving never receives a lifetime disguised as a Type actual.
use super::*;

#[derive(Clone)]
pub(crate) enum Application {
    Ordinary(trait_instances::Reference),
    Context {
        name: String,
        span: Span,
        actuals: Vec<zil_ir::DynGenericActual>,
        /// Trait-declared names, scoped by this application's real declaration.
        lives: BTreeMap<String, zil_ir::StorageBinder>,
        types: BTreeMap<String, TypeRef>,
    },
}
impl Application {
    pub(crate) fn name(&self) -> &str {
        match self {
            Self::Ordinary(r) => &r.key.name,
            Self::Context { name, .. } => name,
        }
    }
    pub(crate) fn span(&self) -> Span {
        match self {
            Self::Ordinary(r) => r.key.declaration,
            Self::Context { span, .. } => *span,
        }
    }
    pub(crate) fn ordinary(&self, span: Span) -> R<&trait_instances::Reference> {
        match self {
            Self::Ordinary(r) => Ok(r),
            _ => Err(unsupported(
                span,
                "contextual dyn application requires its complete lifetime consumer",
            )),
        }
    }
    pub(crate) fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Ordinary(a), Self::Ordinary(b)) => a.key == b.key,
            (
                Self::Context {
                    name: a,
                    actuals: aa,
                    ..
                },
                Self::Context {
                    name: b,
                    actuals: ba,
                    ..
                },
            ) => a == b && aa == ba,
            _ => false,
        }
    }
    pub(crate) fn pay(&self, budget: &mut Budget<'_>, span: Span) -> R<()> {
        match self {
            Self::Ordinary(r) => {
                pay_key(&r.key, budget, span)?;
                for t in &r.arguments {
                    budget.ty(t)?;
                }
            }
            Self::Context {
                name,
                actuals,
                lives,
                types,
                ..
            } => {
                budget.text(name, span)?;
                for actual in actuals {
                    budget.node(span, 0)?;
                    if let zil_ir::DynGenericActual::Type(t) = actual {
                        crate::stored_types::pay_type(t, budget, span, 0)?;
                    }
                }
                for name in lives.keys() {
                    budget.text(name, span)?;
                }
                for (name, ty) in types {
                    budget.text(name, span)?;
                    budget.ty(ty)?;
                }
            }
        }
        Ok(())
    }
    pub(crate) fn clone_paid(&self, budget: &mut Budget<'_>, span: Span) -> R<Self> {
        self.pay(budget, span)?;
        Ok(self.clone())
    }
    pub(crate) fn binder(&self, name: &str) -> Option<zil_ir::StorageBinder> {
        match self {
            Self::Context { lives, .. } => lives.get(name).copied(),
            _ => None,
        }
    }
}
impl Index<'_> {
    pub(in crate::generics) fn dyn_declaration(
        &self,
        ty: &TypeRef,
        budget: &mut Budget<'_>,
    ) -> R<(&Item, &TraitDecl)> {
        budget.ty(ty)?;
        let TypeKind::Dyn {
            path,
            args,
            bindings,
        } = &grouped(ty).kind
        else {
            return Err(error(ty.span, "dyn application syntax absent"));
        };
        if !bindings.is_empty() {
            return Err(unsupported(
                ty.span,
                "dyn associated bindings require their complete consumer",
            ));
        }
        let [name] = path.as_slice() else {
            return Err(error(
                ty.span,
                "dyn trait has no resolved declaration identity",
            ));
        };
        let application = TypeRef {
            span: ty.span,
            kind: TypeKind::Named {
                path: path.clone(),
                args: args.clone(),
            },
        };
        self.declaration(&application, budget)?;
        let (item, declaration) = self
            .declarations
            .get(name.as_str())
            .copied()
            .ok_or_else(|| error(ty.span, "dyn trait declaration absent"))?;
        if args.len() != declaration.generics.len() {
            return Err(error(
                ty.span,
                "dyn trait actual arity differs from declaration",
            ));
        }
        for (formal, actual) in declaration.generics.iter().zip(args) {
            budget.node(formal.span, 0)?;
            if !matches!(
                (&formal.kind, actual),
                (GenericParamKind::Lifetime, GenericArg::Lifetime(_))
                    | (GenericParamKind::Type(_), GenericArg::Type(_))
                    | (GenericParamKind::Const(_), GenericArg::Const(_))
            ) {
                return Err(error(
                    ty.span,
                    "dyn trait actual kind differs from declaration",
                ));
            }
        }
        Ok((item, declaration))
    }
    pub(in crate::generics) fn deferred_application(
        &self,
        ty: &TypeRef,
        budget: &mut Budget<'_>,
    ) -> R<bool> {
        let (_, declaration) = self.dyn_declaration(ty, budget)?;
        Ok(declaration
            .generics
            .iter()
            .any(|p| matches!(p.kind, GenericParamKind::Lifetime)))
    }
    pub(super) fn context_interface(
        &self,
        reference: Application,
        budget: &mut Budget<'_>,
        span: Span,
    ) -> R<Interface> {
        let (item, declaration) = self
            .declarations
            .get(reference.name())
            .copied()
            .ok_or_else(|| error(span, "dyn trait origin absent"))?;
        let origin = self
            .origin(item, budget)?
            .ok_or_else(|| error(span, "dyn trait origin absent"))?;
        if !declaration.bounds.is_empty() {
            return Err(unsupported(
                span,
                "contextual inherited dyn applications require complete inherited witnesses",
            ));
        }
        let Application::Context { types, .. } = &reference else {
            return Err(error(span, "contextual dyn application absent"));
        };
        let mut slots = Vec::new();
        for (ordinal, member) in declaration.members.iter().enumerate() {
            budget.node(member.span, 0)?;
            let ItemKind::Function(method) = &member.kind else {
                continue;
            };
            if !method.generics.is_empty() {
                continue;
            }
            budget.item(member)?;
            let mut item = member.clone();
            budget.substituted_item(&item, types)?;
            substitute_item(&mut item, types)?;
            let ItemKind::Function(mut signature) = item.kind else {
                unreachable!()
            };
            signature.body = FunctionBody::Declaration;
            let Some(receiver) = signature.params.first().filter(|p| p.name == "self") else {
                continue;
            };
            if !super::super::super::super::static_traits::receiver_type(&receiver.ty) {
                return Err(error(receiver.span, "dyn receiver is not a Self receiver"));
            }
            for ty in signature
                .params
                .iter()
                .skip(1)
                .map(|p| &p.ty)
                .chain(signature.return_type.iter())
            {
                budget.ty(ty)?;
                if callable::refers(ty, "Self") {
                    return Err(unsupported(
                        ty.span,
                        "dyn slot needs explicit fixed Self bindings",
                    ));
                }
            }
            slots.push(Slot {
                declaration: origin.member(ordinal, member.span)?.id(member.span)?,
                reference: reference.clone_paid(budget, span)?,
                contract_written: !signature.contract.is_empty(),
                signature,
            });
        }
        Ok(Interface {
            context: None,
            reference,
            declaration: origin.declaration,
            inherited: Vec::new(),
            slots,
        })
    }
}

/// Validate the full declaration application before checking even an unused impl.
/// Substitution preserves lifetime scope; the ordinary header checker then compares
/// both signatures in the same actual impl domain.
pub(in crate::generics) fn contextual_signature(
    declaration: &TraitDecl,
    member: &Item,
    application: &TypeRef,
    budget: &mut Budget<'_>,
) -> R<Function> {
    let TypeKind::Named { args, .. } = &grouped(application).kind else {
        return Err(error(application.span, "impl trait application absent"));
    };
    if args.len() != declaration.generics.len() {
        return Err(error(
            application.span,
            "impl trait actual arity differs from declaration",
        ));
    }
    let mut types = BTreeMap::new();
    let mut lives = BTreeMap::new();
    for (formal, actual) in declaration.generics.iter().zip(args) {
        budget.node(formal.span, 0)?;
        budget.text(&formal.name, formal.span)?;
        match (&formal.kind, actual) {
            (GenericParamKind::Lifetime, GenericArg::Lifetime(name)) => {
                budget.text(name, application.span)?;
                lives.insert(formal.name.clone(), name.clone());
            }
            (GenericParamKind::Type(_), GenericArg::Type(ty)) => {
                budget.ty(ty)?;
                types.insert(formal.name.clone(), ty.clone());
            }
            (GenericParamKind::Const(_), GenericArg::Const(_)) => {
                return Err(unsupported(
                    application.span,
                    "contextual trait const premise requires its complete solver",
                ));
            }
            _ => {
                return Err(error(
                    application.span,
                    "impl trait actual kind differs from declaration",
                ));
            }
        }
    }
    budget.item(member)?;
    let mut member = member.clone();
    budget.substituted_item(&member, &types)?;
    substitute_item(&mut member, &types)?;
    let ItemKind::Function(mut signature) = member.kind else {
        return Err(error(
            member.span,
            "contextual trait member is not callable",
        ));
    };
    crate::generics::source_life_alpha::function(&mut signature, &lives, budget)?;
    Ok(signature)
}
