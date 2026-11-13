//! Capture-avoiding substitution of one declared scheme into a caller Gamma.
//! Frames are indexed by real lexical depth; method and parent ordinals never
//! share a solution vector. Replacement operands are copied atomically.
use crate::*;
use std::collections::BTreeMap;

pub(super) struct Substitution<'a> {
    pub frames: &'a [(u32, &'a [SemanticActualRecipe])],
    pub selves: &'a [(u32, &'a SemanticTypeRecipe)],
    pub environments: &'a [(u32, &'a [SemanticActualRecipe])],
    pub implicit: &'a BTreeMap<u32, SemanticOuterLifeRecipe>,
    pub invocation: &'a BTreeMap<u32, SemanticOuterLifeRecipe>,
    pub shift: u32,
}
struct Apply<'a, 'b> {
    substitution: &'a Substitution<'a>,
    limits: &'b Limits,
    left: &'b mut usize,
}
fn bad(message: &str) -> Diagnostic {
    error("E-SEMANTIC-ORIGINAL", Span::default(), message)
}
impl<'a> Apply<'a, '_> {
    fn pay(&mut self, depth: usize) -> R<()> {
        *self.left = self.left.checked_sub(1).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "original scheme substitution work exceeded",
                None,
            )
        })?;
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "original scheme substitution depth exceeded",
                None,
            ));
        }
        Ok(())
    }
    fn scope(&mut self, scope: &mut SemanticScopeRef) -> R<()> {
        let SemanticScopeRef::Ancestor(depth) = scope;
        *depth = depth
            .checked_add(self.substitution.shift)
            .ok_or_else(|| bad("original scope relocation overflows"))?;
        Ok(())
    }
    fn selected(&mut self, reference: &SemanticFormalRef) -> R<Option<&'a SemanticActualRecipe>> {
        let SemanticScopeRef::Ancestor(depth) = reference.scope;
        for (frame, arguments) in self.substitution.frames {
            if *frame == depth {
                return arguments
                    .get(reference.ordinal as usize)
                    .map(Some)
                    .ok_or_else(|| bad("original scheme omits a complete mixed formal"));
            }
        }
        Ok(None)
    }
    fn environment(
        &mut self,
        scope: SemanticScopeRef,
        ordinal: u32,
    ) -> R<Option<&'a SemanticActualRecipe>> {
        let SemanticScopeRef::Ancestor(depth) = scope;
        for (frame, arguments) in self.substitution.environments {
            if *frame == depth {
                return arguments
                    .get(ordinal as usize)
                    .map(Some)
                    .ok_or_else(|| bad("original scheme omits a sealed environment slot"));
            }
        }
        Ok(None)
    }
    fn operand(&mut self, operand: &mut SemanticRecipeOperand) -> R<()> {
        match operand {
            SemanticRecipeOperand::TypeFormal(reference) => self.scope(&mut reference.scope)?,
            SemanticRecipeOperand::Environment { scope, .. } => self.scope(scope)?,
            SemanticRecipeOperand::Parameter(_) | SemanticRecipeOperand::Receiver => {}
        }
        Ok(())
    }
    fn role(&self, life: &SemanticLifeRecipe) -> Option<&'a SemanticOuterLifeRecipe> {
        match life {
            SemanticLifeRecipe::Implicit {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            } => self.substitution.implicit.get(ordinal),
            SemanticLifeRecipe::Invocation(ordinal) => self.substitution.invocation.get(ordinal),
            _ => None,
        }
    }
    fn outer(&mut self, outer: &mut SemanticOuterLifeRecipe, depth: usize) -> R<()> {
        self.pay(depth)?;
        if let SemanticOuterLifeRecipe::Class(life) = outer {
            if let Some(replacement) = self.role(life) {
                crate::dyn_metadata::semantic_operand_snapshot(
                    replacement,
                    self.limits,
                    self.left,
                    depth + 1,
                )?;
                *outer = replacement.clone();
            } else {
                self.life(life, depth + 1)?;
            }
        }
        Ok(())
    }
    fn life(&mut self, life: &mut SemanticLifeRecipe, depth: usize) -> R<()> {
        self.pay(depth)?;
        if let Some(replacement) = self.role(life) {
            let SemanticOuterLifeRecipe::Class(replacement) = replacement else {
                return Err(bad(
                    "static outer adaptation cannot become a persistent class equality",
                ));
            };
            crate::dyn_metadata::semantic_operand_snapshot(
                replacement,
                self.limits,
                self.left,
                depth + 1,
            )?;
            *life = replacement.clone();
            return Ok(());
        }
        match life {
            SemanticLifeRecipe::Formal(reference) => {
                if let Some(actual) = self.selected(reference)? {
                    let SemanticActualRecipe::Life(replacement) = actual else {
                        return Err(bad("original scheme changes a Life formal kind"));
                    };
                    crate::dyn_metadata::semantic_operand_snapshot(
                        replacement,
                        self.limits,
                        self.left,
                        depth + 1,
                    )?;
                    *life = replacement.clone();
                } else {
                    self.scope(&mut reference.scope)?;
                }
            }
            SemanticLifeRecipe::Environment { scope, ordinal } => {
                if let Some(actual) = self.environment(*scope, *ordinal)? {
                    let SemanticActualRecipe::Life(replacement) = actual else {
                        return Err(bad("original sealed Life slot changes kind"));
                    };
                    crate::dyn_metadata::semantic_operand_snapshot(
                        replacement,
                        self.limits,
                        self.left,
                        depth + 1,
                    )?;
                    *life = replacement.clone();
                } else {
                    self.scope(scope)?;
                }
            }
            SemanticLifeRecipe::Implicit { scope, .. } => self.scope(scope)?,
            SemanticLifeRecipe::OuterOccurrence { operand, .. }
            | SemanticLifeRecipe::IntrinsicClass { operand, .. } => self.operand(operand)?,
            SemanticLifeRecipe::Bound { .. } | SemanticLifeRecipe::Invocation(_) => {}
        }
        Ok(())
    }
    fn actual(&mut self, actual: &mut SemanticActualRecipe, depth: usize) -> R<()> {
        self.pay(depth)?;
        match actual {
            SemanticActualRecipe::Type(ty) => self.ty(ty, depth + 1)?,
            SemanticActualRecipe::Life(life) => self.life(life, depth + 1)?,
            SemanticActualRecipe::Const { carrier, value } => {
                self.ty(carrier, depth + 1)?;
                self.constant(value, depth + 1)?;
            }
            SemanticActualRecipe::Infer => {
                return Err(bad(
                    "original scheme substitution retains an inference hole",
                ))
            }
        }
        Ok(())
    }
    fn application(&mut self, application: &mut SemanticTraitRecipe, depth: usize) -> R<()> {
        self.pay(depth)?;
        for actual in &mut application.arguments {
            self.actual(actual, depth + 1)?;
        }
        Ok(())
    }
    fn constant(&mut self, value: &mut SemanticConstRecipe, depth: usize) -> R<()> {
        self.pay(depth)?;
        match value {
            SemanticConstRecipe::Formal(reference) => {
                if let Some(actual) = self.selected(reference)? {
                    let SemanticActualRecipe::Const {
                        value: replacement, ..
                    } = actual
                    else {
                        return Err(bad("original scheme changes a Const formal kind"));
                    };
                    crate::dyn_metadata::semantic_operand_snapshot(
                        replacement,
                        self.limits,
                        self.left,
                        depth + 1,
                    )?;
                    *value = replacement.clone();
                } else {
                    self.scope(&mut reference.scope)?;
                }
            }
            SemanticConstRecipe::Environment { scope, ordinal } => {
                if let Some(actual) = self.environment(*scope, *ordinal)? {
                    let SemanticActualRecipe::Const {
                        value: replacement, ..
                    } = actual
                    else {
                        return Err(bad("original sealed Const slot changes kind"));
                    };
                    crate::dyn_metadata::semantic_operand_snapshot(
                        replacement,
                        self.limits,
                        self.left,
                        depth + 1,
                    )?;
                    *value = replacement.clone();
                } else {
                    self.scope(scope)?;
                }
            }
            SemanticConstRecipe::Unary {
                operand_type,
                value,
                ..
            } => {
                self.ty(operand_type, depth + 1)?;
                self.constant(value, depth + 1)?;
            }
            SemanticConstRecipe::Binary {
                operand_type,
                left,
                right,
                ..
            } => {
                self.ty(operand_type, depth + 1)?;
                self.constant(left, depth + 1)?;
                self.constant(right, depth + 1)?;
            }
            SemanticConstRecipe::Convert {
                source_type,
                target_type,
                value,
            } => {
                self.ty(source_type, depth + 1)?;
                self.ty(target_type, depth + 1)?;
                self.constant(value, depth + 1)?;
            }
            SemanticConstRecipe::AssociatedConst {
                receiver,
                application,
                ..
            } => {
                self.ty(receiver, depth + 1)?;
                self.application(application, depth + 1)?;
            }
            SemanticConstRecipe::Value(_)
            | SemanticConstRecipe::IntegerLiteral(_)
            | SemanticConstRecipe::FloatLiteral(_) => {}
        }
        Ok(())
    }
    fn sequence(&mut self, sequence: &mut SemanticClassSequenceRecipe, depth: usize) -> R<()> {
        self.pay(depth)?;
        for part in &mut sequence.parts {
            match part {
                SemanticClassSequencePart::One(life) => self.life(life, depth + 1)?,
                SemanticClassSequencePart::AllClasses { ty, .. } => self.ty(ty, depth + 1)?,
            }
        }
        Ok(())
    }
    fn retained(&mut self, parts: &mut [SemanticRetainedPart], depth: usize) -> R<()> {
        self.pay(depth)?;
        for part in parts {
            match part {
                SemanticRetainedPart::One { parent, .. } => self.life(parent, depth + 1)?,
                SemanticRetainedPart::All { parents, .. } => self.sequence(parents, depth + 1)?,
            }
        }
        Ok(())
    }
    fn nominal(&mut self, parts: &mut [SemanticNominalPart], depth: usize) -> R<()> {
        self.pay(depth)?;
        for part in parts {
            match part {
                SemanticNominalPart::One { class, .. } => self.life(class, depth + 1)?,
                SemanticNominalPart::FromType { ty, classes, .. } => {
                    self.ty(ty, depth + 1)?;
                    self.sequence(classes, depth + 1)?;
                }
            }
        }
        Ok(())
    }
    fn storage(&mut self, storage: &mut SemanticStorageShapeRecipe, depth: usize) -> R<()> {
        self.pay(depth)?;
        self.sequence(&mut storage.domain, depth + 1)?;
        for slot in &mut storage.slots {
            self.life(&mut slot.class, depth + 1)?;
        }
        self.retained(&mut storage.retained, depth + 1)?;
        self.nominal(&mut storage.nominal, depth + 1)
    }
    fn retained_shape(&mut self, shape: &mut SemanticRetainedShapeRecipe, depth: usize) -> R<()> {
        self.pay(depth)?;
        self.sequence(&mut shape.domain, depth + 1)?;
        for part in &mut shape.ports {
            match part {
                SemanticPortPart::One { leaf, class, .. } => {
                    self.ty(leaf, depth + 1)?;
                    self.life(class, depth + 1)?;
                }
                SemanticPortPart::FromType { ty, classes } => {
                    self.ty(ty, depth + 1)?;
                    self.sequence(classes, depth + 1)?;
                }
            }
        }
        self.retained(&mut shape.retained, depth + 1)?;
        self.nominal(&mut shape.nominal, depth + 1)
    }
    fn signature(&mut self, signature: &mut SemanticSignatureRecipe, depth: usize) -> R<()> {
        self.pay(depth)?;
        for parameter in &mut signature.parameters {
            self.ty(&mut parameter.ty, depth + 1)?;
        }
        self.ty(&mut signature.result, depth + 1)
    }
    fn requirement(&mut self, requirement: &mut SemanticRequirementRecipe, depth: usize) -> R<()> {
        self.pay(depth)?;
        match requirement {
            SemanticRequirementRecipe::Structural { ty, .. }
            | SemanticRequirementRecipe::Builtin { ty, .. } => self.ty(ty, depth + 1)?,
            SemanticRequirementRecipe::Trait { ty, application } => {
                self.ty(ty, depth + 1)?;
                self.application(application, depth + 1)?;
            }
            SemanticRequirementRecipe::Callable { ty, signature, .. } => {
                self.ty(ty, depth + 1)?;
                self.signature(signature, depth + 1)?;
            }
            SemanticRequirementRecipe::TypeEqual { left, right } => {
                self.ty(left, depth + 1)?;
                self.ty(right, depth + 1)?;
            }
            SemanticRequirementRecipe::ConstEqual {
                carrier,
                left,
                right,
            } => {
                self.ty(carrier, depth + 1)?;
                self.constant(left, depth + 1)?;
                self.constant(right, depth + 1)?;
            }
            SemanticRequirementRecipe::LifeEqual { left, right }
            | SemanticRequirementRecipe::Outlives {
                longer: left,
                shorter: right,
            } => {
                self.life(left, depth + 1)?;
                self.life(right, depth + 1)?;
            }
            SemanticRequirementRecipe::ForAll { requirements, .. } => {
                for requirement in requirements {
                    self.requirement(requirement, depth + 1)?;
                }
            }
        }
        Ok(())
    }
    fn ty(&mut self, ty: &mut SemanticTypeRecipe, depth: usize) -> R<()> {
        self.pay(depth)?;
        match ty {
            SemanticTypeRecipe::Formal(reference) => {
                if let Some(actual) = self.selected(reference)? {
                    let SemanticActualRecipe::Type(replacement) = actual else {
                        return Err(bad("original scheme changes a Type formal kind"));
                    };
                    crate::dyn_metadata::semantic_operand_snapshot(
                        replacement,
                        self.limits,
                        self.left,
                        depth + 1,
                    )?;
                    *ty = replacement.clone();
                } else {
                    self.scope(&mut reference.scope)?;
                }
            }
            SemanticTypeRecipe::SelfType {
                scope: SemanticScopeRef::Ancestor(level),
            } => {
                if let Some((_, replacement)) = self
                    .substitution
                    .selves
                    .iter()
                    .find(|(frame, _)| frame == level)
                {
                    crate::dyn_metadata::semantic_operand_snapshot(
                        *replacement,
                        self.limits,
                        self.left,
                        depth + 1,
                    )?;
                    *ty = (*replacement).clone();
                } else {
                    *level = level
                        .checked_add(self.substitution.shift)
                        .ok_or_else(|| bad("original Self scope overflows"))?;
                }
            }
            SemanticTypeRecipe::Environment { scope, ordinal } => {
                if let Some(actual) = self.environment(*scope, *ordinal)? {
                    let SemanticActualRecipe::Type(replacement) = actual else {
                        return Err(bad("original sealed Type slot changes kind"));
                    };
                    crate::dyn_metadata::semantic_operand_snapshot(
                        replacement,
                        self.limits,
                        self.left,
                        depth + 1,
                    )?;
                    *ty = replacement.clone();
                } else {
                    self.scope(scope)?;
                }
            }
            SemanticTypeRecipe::OriginalType { scope, .. } => self.scope(scope)?,
            SemanticTypeRecipe::Exact(exact) => {
                for class in &mut exact.classes {
                    self.life(class, depth + 1)?;
                }
            }
            SemanticTypeRecipe::Nominal { arguments, .. } => {
                for actual in arguments {
                    self.actual(actual, depth + 1)?;
                }
            }
            SemanticTypeRecipe::AssociatedType {
                receiver,
                application,
                ..
            } => {
                self.ty(receiver, depth + 1)?;
                self.application(application, depth + 1)?;
            }
            SemanticTypeRecipe::Tuple(fields) | SemanticTypeRecipe::AnonymousSum(fields) => {
                for field in fields {
                    self.ty(field, depth + 1)?;
                }
            }
            SemanticTypeRecipe::Array { element, length } => {
                self.ty(element, depth + 1)?;
                self.constant(length, depth + 1)?;
            }
            SemanticTypeRecipe::Address {
                lifetime, inner, ..
            }
            | SemanticTypeRecipe::Desc { lifetime, inner } => {
                self.outer(lifetime, depth + 1)?;
                self.ty(inner, depth + 1)?;
            }
            SemanticTypeRecipe::Slice(inner)
            | SemanticTypeRecipe::Managed(inner)
            | SemanticTypeRecipe::OwnedStore(inner)
            | SemanticTypeRecipe::Mutex(inner)
            | SemanticTypeRecipe::Registration(inner)
            | SemanticTypeRecipe::AsyncRegistration(inner) => self.ty(inner, depth + 1)?,
            SemanticTypeRecipe::MutexGuard { payload, retained } => {
                self.ty(payload, depth + 1)?;
                self.retained_shape(retained, depth + 1)?;
            }
            SemanticTypeRecipe::Task { result, retained } => {
                self.ty(result, depth + 1)?;
                self.retained_shape(retained, depth + 1)?;
            }
            SemanticTypeRecipe::ErrorUnion { error, result } => {
                self.ty(error, depth + 1)?;
                self.ty(result, depth + 1)?;
            }
            SemanticTypeRecipe::Qualified { storage, life } => {
                self.ty(storage, depth + 1)?;
                self.storage(life, depth + 1)?;
            }
            SemanticTypeRecipe::Code { signature, .. } => self.signature(signature, depth + 1)?,
            SemanticTypeRecipe::Closure {
                arguments,
                environment,
                ..
            } => {
                for actual in arguments.iter_mut().chain(environment) {
                    self.actual(actual, depth + 1)?;
                }
            }
            SemanticTypeRecipe::Opaque {
                arguments, hidden, ..
            } => {
                for actual in arguments {
                    self.actual(actual, depth + 1)?;
                }
                self.ty(hidden, depth + 1)?;
            }
            SemanticTypeRecipe::Dyn { interface, .. } => {
                self.application(&mut interface.application, depth + 1)?;
                match &mut interface.context {
                    SemanticDynContextRecipe::Closed => {}
                    SemanticDynContextRecipe::Type(ty) => self.ty(ty, depth + 1)?,
                    SemanticDynContextRecipe::Shape(shape) => {
                        self.retained_shape(shape, depth + 1)?
                    }
                }
                for binding in &mut interface.associated {
                    self.application(&mut binding.application, depth + 1)?;
                    match &mut binding.value {
                        SemanticAssociatedRecipe::Type(ty) => self.ty(ty, depth + 1)?,
                        SemanticAssociatedRecipe::Const { carrier, value } => {
                            self.ty(carrier, depth + 1)?;
                            self.constant(value, depth + 1)?;
                        }
                    }
                }
            }
            SemanticTypeRecipe::Atom(_)
            | SemanticTypeRecipe::UnresolvedNamed(_)
            | SemanticTypeRecipe::Unsupported(_) => {}
        }
        Ok(())
    }
}
pub(super) fn ty(
    value: &SemanticTypeRecipe,
    substitution: &Substitution<'_>,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticTypeRecipe> {
    crate::dyn_metadata::semantic_operand_snapshot(value, limits, left, 1)?;
    let mut value = value.clone();
    Apply {
        substitution,
        limits,
        left,
    }
    .ty(&mut value, 1)?;
    Ok(value)
}
pub(super) fn signature(
    value: &SemanticSignatureRecipe,
    substitution: &Substitution<'_>,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticSignatureRecipe> {
    crate::dyn_metadata::semantic_operand_snapshot(value, limits, left, 1)?;
    let mut value = value.clone();
    Apply {
        substitution,
        limits,
        left,
    }
    .signature(&mut value, 1)?;
    Ok(value)
}

pub(super) fn requirement(
    value: &SemanticRequirementRecipe,
    substitution: &Substitution<'_>,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticRequirementRecipe> {
    crate::dyn_metadata::semantic_operand_snapshot(value, limits, left, 1)?;
    let mut value = value.clone();
    Apply {
        substitution,
        limits,
        left,
    }
    .requirement(&mut value, 1)?;
    Ok(value)
}
