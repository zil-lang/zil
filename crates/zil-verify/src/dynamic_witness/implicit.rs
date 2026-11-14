//! Reconstruct the complete original recipe-token occurrence directory. Numeric
//! paths select public grammar fields and never select a runtime memory place.
use super::*;

struct Walk<'a> {
    found: Vec<Vec<DynImplicitLifeOccurrence>>,
    root: DynImplicitLifeRoot,
    path: Vec<u32>,
    b: &'a mut Budget,
}
impl Walk<'_> {
    fn node(&mut self) -> Check<()> {
        self.b.spend(1)?;
        if self.path.len() > self.b.limits.max_call_depth.min(128) {
            return Err(limit("implicit lifetime recipe path depth exceeded"));
        }
        Ok(())
    }
    fn at(&mut self, field: usize, f: impl FnOnce(&mut Self) -> Check<()>) -> Check<()> {
        self.b.spend(1)?;
        self.path.push(
            u32::try_from(field)
                .map_err(|_| limit("implicit lifetime grammar selector overflow"))?,
        );
        self.node()?;
        let result = f(self);
        self.path.pop();
        result
    }
    fn life(&mut self, value: &DynLifePattern, kind: DynImplicitLifeKind) -> Check<()> {
        self.node()?;
        if let DynLifePattern::ImplicitDeclaration(index) = value {
            self.b.spend(self.path.len().saturating_add(1))?;
            let out = self.found.get_mut(*index as usize).ok_or_else(|| {
                bad("implicit lifetime recipe selects an absent original declaration")
            })?;
            out.push(DynImplicitLifeOccurrence {
                root: self.root,
                path: self.path.clone(),
                kind,
            });
        }
        Ok(())
    }
    fn app(&mut self, app: &DynSelfPattern) -> Check<()> {
        self.node()?;
        self.at(1, |w| {
            for (index, actual) in app.arguments.iter().enumerate() {
                w.at(index, |w| {
                    w.at(0, |w| match actual {
                        DynPatternActual::Type(ty) => w.ty(ty),
                        DynPatternActual::Const(value) => w.constant(value),
                        DynPatternActual::Life(life) => {
                            w.life(life, DynImplicitLifeKind::LifetimeArgument)
                        }
                    })
                })?;
            }
            Ok(())
        })
    }
    fn ty(&mut self, ty: &DynTypePattern) -> Check<()> {
        self.node()?;
        match ty {
            DynTypePattern::Semantic { ty, .. } => {
                // Semantic implicit ordinals belong to their original lexical
                // scope, not this Dyn anonymous-token namespace. The scoped
                // grammar pass checks each referenced original occurrence;
                // the original/actual interpreters still check its full WF.
                // Never insert an invented Dyn binder or infer a zero domain.
                dynamic_metadata::recipe(ty.as_ref(), self.b)?;
            }
            DynTypePattern::AssociatedProjection {
                receiver,
                application,
                ..
            } => {
                self.at(0, |w| w.ty(receiver))?;
                self.at(1, |w| w.app(application))?;
            }
            DynTypePattern::Nominal(app) => self.at(0, |w| w.app(app))?,
            DynTypePattern::Address {
                lifetime, inner, ..
            } => {
                if let Some(life) = lifetime {
                    self.at(1, |w| w.life(life, DynImplicitLifeKind::Address))?;
                }
                self.at(2, |w| w.ty(inner))?;
            }
            DynTypePattern::Desc { lifetime, inner } => {
                if let Some(life) = lifetime {
                    self.at(0, |w| w.life(life, DynImplicitLifeKind::Description))?;
                }
                self.at(1, |w| w.ty(inner))?;
            }
            DynTypePattern::Tuple(fields) => self.at(0, |w| {
                for (i, ty) in fields.iter().enumerate() {
                    w.at(i, |w| w.ty(ty))?;
                }
                Ok(())
            })?,
            DynTypePattern::Array { element, length } => {
                self.at(0, |w| w.ty(element))?;
                self.at(1, |w| w.constant(length))?;
            }
            DynTypePattern::Slice(inner)
            | DynTypePattern::Managed(inner)
            | DynTypePattern::OwnedStore(inner) => self.at(0, |w| w.ty(inner))?,
            DynTypePattern::ErrorUnion { error, result } => {
                self.at(0, |w| w.ty(error))?;
                self.at(1, |w| w.ty(result))?;
            }
            DynTypePattern::SelfType
            | DynTypePattern::MethodParameter(_)
            | DynTypePattern::Concrete(_)
            | DynTypePattern::Parameter(_) => {}
        }
        Ok(())
    }
    fn constant(&mut self, value: &DynConstPattern) -> Check<()> {
        self.node()?;
        match value {
            DynConstPattern::Unary { value, ty, .. } => {
                self.at(1, |w| w.constant(value))?;
                self.at(2, |w| w.ty(ty))?;
            }
            DynConstPattern::Binary {
                left, right, ty, ..
            } => {
                self.at(1, |w| w.constant(left))?;
                self.at(2, |w| w.constant(right))?;
                self.at(3, |w| w.ty(ty))?;
            }
            DynConstPattern::Convert { value, ty } => {
                self.at(0, |w| w.constant(value))?;
                self.at(1, |w| w.ty(ty))?;
            }
            DynConstPattern::AssociatedProjection {
                receiver,
                application,
                ..
            } => {
                self.at(0, |w| w.ty(receiver))?;
                self.at(1, |w| w.app(application))?;
            }
            DynConstPattern::IntegerLiteral(_)
            | DynConstPattern::FloatLiteral(_)
            | DynConstPattern::MethodParameter(_)
            | DynConstPattern::Value(_)
            | DynConstPattern::Parameter(_) => {}
        }
        Ok(())
    }
    fn obligation(&mut self, value: &DynObligation) -> Check<()> {
        self.node()?;
        match value {
            DynObligation::Structural { receiver, .. } => self.at(0, |w| w.ty(receiver))?,
            DynObligation::Trait {
                receiver,
                application,
            } => {
                self.at(0, |w| w.ty(receiver))?;
                self.at(1, |w| w.app(application))?;
            }
            DynObligation::Callable {
                receiver,
                signature,
                ..
            } => {
                self.at(0, |w| w.ty(receiver))?;
                self.at(3, |w| w.signature(signature))?;
            }
            DynObligation::TypeEqual { left, right } => {
                self.at(0, |w| w.ty(left))?;
                self.at(1, |w| w.ty(right))?;
            }
            DynObligation::ConstEqual { ty, left, right } => {
                self.at(0, |w| w.ty(ty))?;
                self.at(1, |w| w.constant(left))?;
                self.at(2, |w| w.constant(right))?;
            }
        }
        Ok(())
    }
    fn effects(&mut self, effects: &[DynEffectRecipe]) -> Check<()> {
        for (i, effect) in effects.iter().enumerate() {
            self.at(i, |w| {
                if let DynEffectRecipe::Cleanup(ty) = effect {
                    w.at(0, |w| w.ty(ty))?;
                }
                Ok(())
            })?;
        }
        Ok(())
    }
    fn signature(&mut self, value: &DynSignatureRecipe) -> Check<()> {
        self.node()?;
        self.at(0, |w| {
            for (i, p) in value.parameters.iter().enumerate() {
                w.at(i, |w| w.at(0, |w| w.ty(&p.ty)))?;
            }
            Ok(())
        })?;
        self.at(1, |w| w.ty(&value.result))?;
        self.at(5, |w| {
            let c = &value.contract;
            if let Some(rows) = &c.requires {
                w.at(0, |w| {
                    for (i, row) in rows.iter().enumerate() {
                        w.at(i, |w| {
                            if let DynRequirementRecipe::State { ty, .. } = row {
                                w.at(1, |w| w.ty(ty))?;
                            }
                            Ok(())
                        })?;
                    }
                    Ok(())
                })?;
            }
            if let Some(rows) = &c.guarantees {
                w.at(1, |w| {
                    for (i, row) in rows.iter().enumerate() {
                        w.at(i, |w| {
                            if let DynGuaranteeRecipe::State { ty, .. } = row {
                                w.at(1, |w| w.ty(ty))?;
                            }
                            Ok(())
                        })?;
                    }
                    Ok(())
                })?;
            }
            if let Some(cleanup) = &c.cleanup {
                w.at(5, |w| w.at(3, |w| w.effects(&cleanup.effects)))?;
            }
            if let Some(effects) = &c.effects {
                w.at(6, |w| w.effects(effects))?;
            }
            Ok(())
        })
    }
}
fn ordered<'a>(
    values: &'a [DynImplicitLifeOccurrence],
    b: &mut Budget,
) -> Check<Vec<&'a DynImplicitLifeOccurrence>> {
    let max = values.iter().map(|v| v.path.len()).max().unwrap_or(0);
    let log = values.len().checked_ilog2().map_or(1, |v| v as usize + 2);
    b.spend(
        values
            .len()
            .saturating_mul(log)
            .saturating_mul(max.saturating_add(1))
            .saturating_add(values.len()),
    )?;
    let mut out = values.iter().collect::<Vec<_>>();
    out.sort();
    Ok(out)
}
pub(super) fn check(declaration: &DynDeclaration, b: &mut Budget) -> Check<()> {
    dynamic_metadata::recipe_declaration(declaration, b)?;
    let table = declaration
        .implicit_lifetimes
        .as_ref()
        .ok_or_else(|| bad("original implicit lifetime occurrence directory is Unknown"))?;
    if table.len() > b.limits.max_nodes {
        return Err(limit("implicit lifetime directory exceeds selected bound"));
    }
    b.spend(table.len())?;
    let mut w = Walk {
        found: (0..table.len()).map(|_| vec![]).collect(),
        root: DynImplicitLifeRoot::SelfPattern,
        path: vec![],
        b,
    };
    if let Some(ty) = &declaration.self_pattern {
        w.ty(ty)?;
    }
    if let Some(app) = &declaration.implemented_trait {
        w.root = DynImplicitLifeRoot::ImplementedTrait;
        w.app(app)?;
    }
    for (i, app) in declaration.direct_supertraits.iter().flatten().enumerate() {
        w.root = DynImplicitLifeRoot::Supertrait(
            u32::try_from(i).map_err(|_| limit("super occurrence ordinal overflow"))?,
        );
        w.app(app)?;
    }
    for (i, value) in declaration.requirements.iter().flatten().enumerate() {
        w.root = DynImplicitLifeRoot::Requirement(
            u32::try_from(i).map_err(|_| limit("requirement occurrence ordinal overflow"))?,
        );
        w.obligation(value)?;
    }
    for (i, value) in declaration
        .associated_definitions
        .iter()
        .flatten()
        .enumerate()
    {
        w.root = DynImplicitLifeRoot::AssociatedDefinition(
            u32::try_from(i).map_err(|_| limit("associated occurrence ordinal overflow"))?,
        );
        if let Some(ty) = &value.carrier {
            w.at(0, |w| w.ty(ty))?;
        }
        if let Some(value) = &value.value {
            w.at(2, |w| match value {
                DynAssociatedValueRecipe::Type(ty) => w.at(0, |w| w.ty(ty)),
                DynAssociatedValueRecipe::Constant { ty, value } => {
                    w.at(0, |w| w.ty(ty))?;
                    w.at(1, |w| w.constant(value))
                }
            })?;
        }
        w.at(3, |w| {
            for (i, value) in value.requirements.iter().enumerate() {
                w.at(i, |w| w.obligation(value))?;
            }
            Ok(())
        })?;
    }
    for (i, value) in declaration.method_signatures.iter().flatten().enumerate() {
        w.root = DynImplicitLifeRoot::MethodSignature(
            u32::try_from(i).map_err(|_| limit("method occurrence ordinal overflow"))?,
        );
        w.at(0, |w| {
            for (i, value) in value.requirements.iter().enumerate() {
                w.at(i, |w| w.obligation(value))?;
            }
            Ok(())
        })?;
        w.at(2, |w| w.signature(&value.signature))?;
    }
    for (i, value) in declaration.method_definitions.iter().flatten().enumerate() {
        w.root = DynImplicitLifeRoot::MethodDefinition(
            u32::try_from(i).map_err(|_| limit("definition occurrence ordinal overflow"))?,
        );
        w.at(0, |w| {
            for (i, value) in value.requirements.iter().enumerate() {
                w.at(i, |w| w.obligation(value))?;
            }
            Ok(())
        })?;
        w.at(6, |w| w.signature(&value.signature))?;
    }
    for (original, actual) in table.iter().zip(&w.found) {
        if original.occurrences.is_empty() || original.occurrences.len() != actual.len() {
            return Err(bad(
                "implicit lifetime directory omits or invents an original recipe token",
            ));
        }
        let original = ordered(&original.occurrences, w.b)?;
        let actual = ordered(actual, w.b)?;
        if original.windows(2).any(|pair| pair[0] == pair[1]) || original != actual {
            return Err(bad(
                "implicit lifetime occurrence root, complete path, kind or grouping differs",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{ModuleId, NodeId};
    fn declaration() -> DynDeclaration {
        DynDeclaration {
            id: LifeDeclarationId {
                module: ModuleId(0),
                declaration: NodeId(1),
                compiler: false,
            },
            identity: LifeDeclarationIdentity {
                package: "implicit-test".into(),
                module_path: vec![],
                declaration_path: vec!["Impl".into()],
                generation_path: vec![],
            },
            kind: DynDeclarationKind::Impl,
            parameters: vec![],
            members: vec![],
            self_pattern: Some(DynTypePattern::Desc {
                lifetime: Some(DynLifePattern::ImplicitDeclaration(0)),
                inner: Box::new(DynTypePattern::Concrete(Type::I32)),
            }),
            implemented_trait: None,
            direct_supertraits: Some(vec![]),
            requirements: Some(vec![]),
            associated_definitions: Some(vec![]),
            method_signatures: Some(vec![]),
            method_definitions: Some(vec![]),
            implicit_lifetimes: Some(vec![DynImplicitLifeDeclaration {
                occurrences: vec![DynImplicitLifeOccurrence {
                    root: DynImplicitLifeRoot::SelfPattern,
                    path: vec![0],
                    kind: DynImplicitLifeKind::Description,
                }],
            }]),
        }
    }
    fn run(value: &DynDeclaration) -> Check<()> {
        check(value, &mut Budget::new(&Limits::default()))
    }
    #[test]
    fn implicit_description_is_bound_to_its_real_original_token_and_kind() {
        let original = declaration();
        assert!(run(&original).is_ok());
        for mode in 0..5 {
            let mut raw = original.clone();
            let occurrences = &mut raw.implicit_lifetimes.as_mut().unwrap()[0].occurrences;
            match mode {
                0 => occurrences[0].path = vec![1],
                1 => occurrences[0].kind = DynImplicitLifeKind::Address,
                2 => occurrences[0].root = DynImplicitLifeRoot::Requirement(0),
                3 => occurrences.push(occurrences[0].clone()),
                _ => occurrences.clear(),
            };
            assert!(run(&raw).is_err());
        }
    }
    #[test]
    fn an_implicit_lifetime_argument_keeps_full_actual_ordinal_and_alias_occurrences() {
        let mut original = declaration();
        original.implemented_trait = Some(DynSelfPattern {
            declaration: original.id,
            arguments: vec![
                DynPatternActual::Type(DynTypePattern::Concrete(Type::I32)),
                DynPatternActual::Life(DynLifePattern::ImplicitDeclaration(0)),
            ],
        });
        original.implicit_lifetimes.as_mut().unwrap()[0]
            .occurrences
            .push(DynImplicitLifeOccurrence {
                root: DynImplicitLifeRoot::ImplementedTrait,
                path: vec![1, 1, 0],
                kind: DynImplicitLifeKind::LifetimeArgument,
            });
        assert!(run(&original).is_ok());
        original.implicit_lifetimes.as_mut().unwrap()[0].occurrences[1].path = vec![1, 0, 0];
        assert!(run(&original).is_err());
    }
}
