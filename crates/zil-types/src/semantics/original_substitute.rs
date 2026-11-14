//! Capture-avoiding substitution of one declared scheme into a caller Gamma.
//! Frames are indexed by real lexical depth; method and parent ordinals never
//! share a solution vector. Replacement operands are copied atomically.
use crate::*;
use std::collections::BTreeMap;

pub(crate) struct Substitution<'a> {
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
    bound_depth: u32,
    bound_delta: u32,
    bound_cutoff: u32,
    invocation_depth: u32,
    fresh_roles: bool,
    template: bool,
    template_implicit: bool,
    scopes: Option<&'a BTreeMap<u32, u32>>,
    allow_root_invocation: bool,
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
        if let Some(scopes) = self.scopes {
            *self.left = self.left.checked_sub(scopes.len()).ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "original lexical correspondence work exceeded",
                    None,
                )
            })?;
            *depth = *scopes.get(depth).ok_or_else(|| {
                Diagnostic::unsupported(
                    Phase::Types,
                    "original substitution has no proved source-to-use lexical domain mapping",
                    None,
                )
            })?;
        } else {
            *depth = depth
                .checked_add(self.substitution.shift)
                .ok_or_else(|| bad("original scope relocation overflows"))?;
        }
        Ok(())
    }
    fn selected(&mut self, reference: &SemanticFormalRef) -> R<Option<&'a SemanticActualRecipe>> {
        let SemanticScopeRef::Ancestor(depth) = reference.scope;
        *self.left = self
            .left
            .checked_sub(self.substitution.frames.len())
            .ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "original formal correspondence work exceeded",
                    None,
                )
            })?;
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
        *self.left = self
            .left
            .checked_sub(self.substitution.environments.len())
            .ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "original environment correspondence work exceeded",
                    None,
                )
            })?;
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
            SemanticRecipeOperand::TypeFormal(reference) => {
                if self.scopes.is_some() {
                    if let Some(actual) = self.selected(reference)? {
                        match actual {
                            SemanticActualRecipe::Type(SemanticTypeRecipe::Formal(actual)) => { *reference = actual.clone(); },
                            SemanticActualRecipe::Type(SemanticTypeRecipe::Environment { scope, ordinal }) => {
                                *operand = SemanticRecipeOperand::Environment { scope: *scope, ordinal: *ordinal };
                            }
                            _ => return Err(Diagnostic::unsupported(Phase::Types,
                                "original whole-Type class operand needs its complete substituted occurrence mapping", None)),
                        }
                    } else {
                        self.scope(&mut reference.scope)?;
                    }
                } else {
                    self.scope(&mut reference.scope)?;
                }
            }
            SemanticRecipeOperand::Environment { scope, .. } => self.scope(scope)?,
            SemanticRecipeOperand::Parameter(_) | SemanticRecipeOperand::Receiver => {
                if self.scopes.is_some()
                    && !self.allow_root_invocation
                    && self.invocation_depth == 0
                {
                    return Err(Diagnostic::unsupported(
                        Phase::Types,
                        "original parameter endpoint has no proved source-to-use value boundary",
                        None,
                    ));
                }
            }
        }
        Ok(())
    }
    fn role(&self, life: &SemanticLifeRecipe) -> Option<&'a SemanticOuterLifeRecipe> {
        if self.fresh_roles {
            return match life {
                SemanticLifeRecipe::Implicit {
                    scope: SemanticScopeRef::Ancestor(u32::MAX),
                    ordinal,
                } => self.substitution.implicit.get(ordinal),
                SemanticLifeRecipe::Environment {
                    scope: SemanticScopeRef::Ancestor(u32::MAX),
                    ordinal,
                } => self.substitution.invocation.get(ordinal),
                _ => None,
            };
        }
        match life {
            SemanticLifeRecipe::Implicit {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            } => self.substitution.implicit.get(ordinal),
            SemanticLifeRecipe::Invocation(ordinal) if self.invocation_depth == 0 => {
                self.substitution.invocation.get(ordinal)
            }
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
                    1,
                )?;
                *outer = replacement.clone();
                if let SemanticOuterLifeRecipe::Class(life) = outer {
                    self.lift_life(life, depth + 1)?;
                }
            } else {
                self.life(life, depth + 1)?;
            }
        }
        Ok(())
    }
    fn life(&mut self, life: &mut SemanticLifeRecipe, depth: usize) -> R<()> {
        self.pay(depth)?;
        if self.template {
            match life {
                SemanticLifeRecipe::Implicit { scope, .. }
                    if self.template_implicit && *scope == SemanticScopeRef::Ancestor(0) =>
                {
                    *scope = SemanticScopeRef::Ancestor(u32::MAX);
                    return Ok(());
                }
                SemanticLifeRecipe::Invocation(ordinal) if self.invocation_depth == 0 => {
                    *life = SemanticLifeRecipe::Environment {
                        scope: SemanticScopeRef::Ancestor(u32::MAX),
                        ordinal: *ordinal,
                    };
                    return Ok(());
                }
                _ => {}
            }
        }
        if let Some(replacement) = self.role(life) {
            let SemanticOuterLifeRecipe::Class(replacement) = replacement else {
                return Err(bad(
                    "static outer adaptation cannot become a persistent class equality",
                ));
            };
            crate::dyn_metadata::semantic_operand_snapshot(replacement, self.limits, self.left, 1)?;
            *life = replacement.clone();
            self.lift_life(life, depth + 1)?;
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
                        1,
                    )?;
                    *life = replacement.clone();
                    self.lift_life(life, depth + 1)?;
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
                        1,
                    )?;
                    *life = replacement.clone();
                    self.lift_life(life, depth + 1)?;
                } else {
                    self.scope(scope)?;
                }
            }
            SemanticLifeRecipe::Implicit { scope, .. } => self.scope(scope)?,
            SemanticLifeRecipe::OuterOccurrence { operand, .. }
            | SemanticLifeRecipe::IntrinsicClass { operand, .. } => self.operand(operand)?,
            SemanticLifeRecipe::Bound { depth, .. } => {
                if *depth >= self.bound_cutoff {
                    *depth = depth
                        .checked_add(self.bound_delta)
                        .ok_or_else(|| bad("original replacement Bound relocation overflows"))?;
                }
            }
            SemanticLifeRecipe::Invocation(_) => {
                if self.scopes.is_some()
                    && !self.allow_root_invocation
                    && self.invocation_depth == 0
                {
                    return Err(Diagnostic::unsupported(
                        Phase::Types,
                        "original Invocation endpoint has no proved source-to-use call boundary",
                        None,
                    ));
                }
            }
        }
        Ok(())
    }
    fn lift_type(&mut self, value: &mut SemanticTypeRecipe, depth: usize) -> R<()> {
        if self.bound_depth == 0 {
            return Ok(());
        }
        let empty = BTreeMap::new();
        let substitution = Substitution {
            frames: &[],
            selves: &[],
            environments: &[],
            implicit: &empty,
            invocation: &empty,
            shift: 0,
        };
        Apply {
            substitution: &substitution,
            limits: self.limits,
            left: self.left,
            bound_depth: 0,
            bound_delta: self.bound_depth,
            bound_cutoff: 0,
            invocation_depth: 0,
            fresh_roles: false,
            template: false,
            template_implicit: false,
            scopes: None,
            allow_root_invocation: true,
        }
        .ty(value, depth)
    }
    fn lift_life(&mut self, value: &mut SemanticLifeRecipe, depth: usize) -> R<()> {
        if self.bound_depth == 0 {
            return Ok(());
        }
        let empty = BTreeMap::new();
        let substitution = Substitution {
            frames: &[],
            selves: &[],
            environments: &[],
            implicit: &empty,
            invocation: &empty,
            shift: 0,
        };
        Apply {
            substitution: &substitution,
            limits: self.limits,
            left: self.left,
            bound_depth: 0,
            bound_delta: self.bound_depth,
            bound_cutoff: 0,
            invocation_depth: 0,
            fresh_roles: false,
            template: false,
            template_implicit: false,
            scopes: None,
            allow_root_invocation: true,
        }
        .life(value, depth)
    }
    fn lift_const(&mut self, value: &mut SemanticConstRecipe, depth: usize) -> R<()> {
        if self.bound_depth == 0 {
            return Ok(());
        }
        let empty = BTreeMap::new();
        let substitution = Substitution {
            frames: &[],
            selves: &[],
            environments: &[],
            implicit: &empty,
            invocation: &empty,
            shift: 0,
        };
        Apply {
            substitution: &substitution,
            limits: self.limits,
            left: self.left,
            bound_depth: 0,
            bound_delta: self.bound_depth,
            bound_cutoff: 0,
            invocation_depth: 0,
            fresh_roles: false,
            template: false,
            template_implicit: false,
            scopes: None,
            allow_root_invocation: true,
        }
        .constant(value, depth)
    }
    fn nested_signature(&mut self, value: &mut SemanticSignatureRecipe, depth: usize) -> R<()> {
        self.invocation_depth = self
            .invocation_depth
            .checked_add(1)
            .ok_or_else(|| bad("original nested Invocation scope overflows"))?;
        let result = self.signature(value, depth);
        self.invocation_depth -= 1;
        result
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
                        1,
                    )?;
                    *value = replacement.clone();
                    self.lift_const(value, depth + 1)?;
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
                        1,
                    )?;
                    *value = replacement.clone();
                    self.lift_const(value, depth + 1)?;
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
        let frame = u32::from(signature.bound_lifetimes > 0);
        self.bound_depth = self
            .bound_depth
            .checked_add(frame)
            .ok_or_else(|| bad("original Bound context overflows"))?;
        self.bound_cutoff = self
            .bound_cutoff
            .checked_add(frame)
            .ok_or_else(|| bad("original Bound cutoff overflows"))?;
        let result = (|| {
            for parameter in &mut signature.parameters {
                self.ty(&mut parameter.ty, depth + 1)?;
            }
            self.ty(&mut signature.result, depth + 1)
        })();
        self.bound_depth -= frame;
        self.bound_cutoff -= frame;
        result
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
                self.nested_signature(signature, depth + 1)?;
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
            SemanticRequirementRecipe::ForAll {
                lifetimes,
                requirements,
            } => {
                let frame = u32::from(*lifetimes > 0);
                self.bound_depth = self
                    .bound_depth
                    .checked_add(frame)
                    .ok_or_else(|| bad("original ForAll context overflows"))?;
                self.bound_cutoff = self
                    .bound_cutoff
                    .checked_add(frame)
                    .ok_or_else(|| bad("original ForAll cutoff overflows"))?;
                let result = (|| {
                    for requirement in requirements {
                        self.requirement(requirement, depth + 1)?;
                    }
                    Ok::<_, Diagnostic>(())
                })();
                self.bound_depth -= frame;
                self.bound_cutoff -= frame;
                result?;
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
                        1,
                    )?;
                    *ty = replacement.clone();
                    self.lift_type(ty, depth + 1)?;
                } else {
                    self.scope(&mut reference.scope)?;
                }
            }
            SemanticTypeRecipe::SelfType { scope } => {
                let SemanticScopeRef::Ancestor(level) = *scope;
                if let Some((_, replacement)) = self
                    .substitution
                    .selves
                    .iter()
                    .find(|(frame, _)| *frame == level)
                {
                    crate::dyn_metadata::semantic_operand_snapshot(
                        *replacement,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    *ty = (*replacement).clone();
                    self.lift_type(ty, depth + 1)?;
                } else {
                    self.scope(scope)?;
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
                        1,
                    )?;
                    *ty = replacement.clone();
                    self.lift_type(ty, depth + 1)?;
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
            SemanticTypeRecipe::Code { signature, .. } => {
                self.nested_signature(signature, depth + 1)?
            }
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
                // `hidden` belongs to `definition`, not to the caller frame.
                // Its body and local result roles are sealed independently.
                let _ = hidden;
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
pub(crate) fn ty(
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
        bound_depth: 0,
        bound_delta: 0,
        bound_cutoff: 0,
        invocation_depth: 0,
        fresh_roles: false,
        template: false,
        template_implicit: false,
        scopes: None,
        allow_root_invocation: true,
    }
    .ty(&mut value, 1)?;
    Ok(value)
}
pub(crate) fn signature(
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
        bound_depth: 0,
        bound_delta: 0,
        bound_cutoff: 0,
        invocation_depth: 0,
        fresh_roles: false,
        template: false,
        template_implicit: false,
        scopes: None,
        allow_root_invocation: true,
    }
    .signature(&mut value, 1)?;
    Ok(value)
}

pub(crate) fn requirement(
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
        bound_depth: 0,
        bound_delta: 0,
        bound_cutoff: 0,
        invocation_depth: 0,
        fresh_roles: false,
        template: false,
        template_implicit: false,
        scopes: None,
        allow_root_invocation: true,
    }
    .requirement(&mut value, 1)?;
    Ok(value)
}

pub(crate) fn actual(
    value: &SemanticActualRecipe,
    substitution: &Substitution<'_>,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticActualRecipe> {
    crate::dyn_metadata::semantic_operand_snapshot(value, limits, left, 1)?;
    let mut value = value.clone();
    Apply {
        substitution,
        limits,
        left,
        bound_depth: 0,
        bound_delta: 0,
        bound_cutoff: 0,
        invocation_depth: 0,
        fresh_roles: false,
        template: false,
        template_implicit: false,
        scopes: None,
        allow_root_invocation: true,
    }
    .actual(&mut value, 1)?;
    Ok(value)
}

pub(crate) fn signature_template(
    value: &SemanticSignatureRecipe,
    substitution: &Substitution<'_>,
    mark_implicit: bool,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticSignatureRecipe> {
    crate::dyn_metadata::semantic_operand_snapshot(value, limits, left, 1)?;
    let mut value = value.clone();
    Apply {
        substitution,
        limits,
        left,
        bound_depth: 0,
        bound_delta: 0,
        bound_cutoff: 0,
        invocation_depth: 0,
        fresh_roles: false,
        template: true,
        template_implicit: mark_implicit,
        scopes: None,
        allow_root_invocation: true,
    }
    .signature(&mut value, 1)?;
    Ok(value)
}
pub(crate) fn signature_fresh(
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
        bound_depth: 0,
        bound_delta: 0,
        bound_cutoff: 0,
        invocation_depth: 0,
        fresh_roles: true,
        template: false,
        template_implicit: false,
        scopes: None,
        allow_root_invocation: true,
    }
    .signature(&mut value, 1)?;
    Ok(value)
}

/// The caller supplies only independently validated lexical correspondences.
/// Every remaining source-domain reference must occur in this finite map.
pub(crate) fn ty_in_domains(
    value: &SemanticTypeRecipe,
    substitution: &Substitution<'_>,
    scopes: &BTreeMap<u32, u32>,
    allow_root_invocation: bool,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticTypeRecipe> {
    crate::dyn_metadata::semantic_operand_snapshot(value, limits, left, 1)?;
    let mut value = value.clone();
    Apply {
        substitution,
        limits,
        left,
        bound_depth: 0,
        bound_delta: 0,
        bound_cutoff: 0,
        invocation_depth: 0,
        fresh_roles: false,
        template: false,
        template_implicit: false,
        scopes: Some(scopes),
        allow_root_invocation,
    }
    .ty(&mut value, 1)?;
    Ok(value)
}
