//! Structural validation of the complete original ghost program. Successful
//! lookup never establishes a class equality, a target, or a borrowing right.
use crate::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Type,
    Const,
    Life,
}
fn kind(value: &SemanticFormal) -> Kind {
    match value {
        SemanticFormal::Type { .. } => Kind::Type,
        SemanticFormal::Const { .. } => Kind::Const,
        SemanticFormal::Lifetime => Kind::Life,
    }
}
fn fail(message: &str) -> Diagnostic {
    error("E-SEMANTIC-SCOPE", Span::default(), message)
}
struct Check<'a, 'b> {
    definitions: BTreeMap<SemanticDefinitionId, &'a SemanticDefinition>,
    instances: BTreeMap<SemanticInstanceId, &'a SemanticInstance>,
    modules: &'a [&'a CoreModule],
    limits: &'b Limits,
    left: &'b mut usize,
}
impl<'a> Check<'a, '_> {
    fn fee(&mut self, count: usize) -> R<()> {
        *self.left = self.left.checked_sub(count).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "semantic structural work budget exceeded",
                None,
            )
        })?;
        Ok(())
    }
    fn node(&mut self, depth: usize) -> R<()> {
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "semantic structural depth exceeded",
                None,
            ));
        }
        self.fee(1)
    }
    fn definition(&mut self, id: SemanticDefinitionId) -> R<&'a SemanticDefinition> {
        self.fee((self.definitions.len().saturating_add(1)).ilog2() as usize + 1)?;
        self.definitions
            .get(&id)
            .copied()
            .ok_or_else(|| fail("semantic definition is absent from the complete program"))
    }
    fn instance(&mut self, id: SemanticInstanceId) -> R<&'a SemanticInstance> {
        self.fee((self.instances.len().saturating_add(1)).ilog2() as usize + 1)?;
        self.instances
            .get(&id)
            .copied()
            .ok_or_else(|| fail("semantic instance is absent from the complete program"))
    }
    fn scope(
        &mut self,
        mut owner: SemanticDefinitionId,
        scope: SemanticScopeRef,
    ) -> R<&'a SemanticDefinition> {
        let SemanticScopeRef::Ancestor(hops) = scope;
        for depth in 0..hops as usize {
            self.node(depth)?;
            owner = self
                .definition(owner)?
                .parent
                .ok_or_else(|| fail("semantic scope selector crosses the lexical root"))?;
        }
        self.definition(owner)
    }
    fn formal(
        &mut self,
        owner: SemanticDefinitionId,
        reference: &SemanticFormalRef,
        expected: Kind,
    ) -> R<()> {
        let definition = self.scope(owner, reference.scope)?;
        self.fee(1)?;
        let formal = definition
            .parameters
            .get(reference.ordinal as usize)
            .ok_or_else(|| {
                fail("semantic formal ordinal is outside the full original mixed list")
            })?;
        if kind(formal) != expected {
            return Err(fail("semantic formal selector has the wrong original kind"));
        }
        Ok(())
    }
    fn environment(
        &mut self,
        owner: SemanticDefinitionId,
        scope: SemanticScopeRef,
        ordinal: u32,
        expected: Kind,
    ) -> R<()> {
        let definition = self.scope(owner, scope)?;
        self.fee(1)?;
        let formal = definition
            .environment
            .get(ordinal as usize)
            .ok_or_else(|| fail("semantic environment ordinal is absent"))?;
        if kind(&formal.kind) != expected {
            return Err(fail("semantic environment selector has the wrong kind"));
        }
        Ok(())
    }
    fn implicit(
        &mut self,
        owner: SemanticDefinitionId,
        scope: SemanticScopeRef,
        ordinal: u32,
    ) -> R<()> {
        let definition = self.scope(owner, scope)?;
        self.fee(definition.implicit_lifetimes.len().saturating_add(1))?;
        if definition
            .implicit_lifetimes
            .iter()
            .filter(|row| row.ordinal == ordinal)
            .count()
            != 1
        {
            return Err(fail(
                "semantic implicit lifetime does not name one original occurrence",
            ));
        }
        Ok(())
    }
    fn bound(&mut self, bound: &[u32], depth: u32, index: u32) -> R<()> {
        self.fee(2)?;
        let level = bound
            .len()
            .checked_sub(depth as usize + 1)
            .ok_or_else(|| fail("semantic bound lifetime escapes its real binder"))?;
        if index >= bound[level] {
            return Err(fail("semantic bound lifetime index is absent"));
        }
        Ok(())
    }
    fn operand(&mut self, owner: SemanticDefinitionId, operand: &SemanticRecipeOperand) -> R<()> {
        self.fee(1)?;
        match operand {
            SemanticRecipeOperand::TypeFormal(reference) => {
                self.formal(owner, reference, Kind::Type)
            }
            SemanticRecipeOperand::Environment { scope, ordinal } => {
                self.environment(owner, *scope, *ordinal, Kind::Type)
            }
            SemanticRecipeOperand::Parameter(index) => {
                let definition = self.definition(owner)?;
                let SemanticDefinitionBody::Function { signature, .. } = &definition.body else {
                    return Err(fail("semantic parameter operand is outside a function"));
                };
                if *index as usize >= signature.parameters.len() {
                    return Err(fail("semantic parameter operand is absent"));
                }
                Ok(())
            }
            SemanticRecipeOperand::Receiver => {
                if self.definition(owner)?.kind != SemanticDefinitionKind::Closure {
                    return Err(fail(
                        "semantic receiver operand is outside a closure definition",
                    ));
                }
                Ok(())
            }
        }
    }
    fn life(
        &mut self,
        owner: SemanticDefinitionId,
        value: &SemanticLifeRecipe,
        bound: &[u32],
        invocation: u32,
        depth: usize,
    ) -> R<()> {
        self.node(depth)?;
        match value {
            SemanticLifeRecipe::Formal(reference) => self.formal(owner, reference, Kind::Life),
            SemanticLifeRecipe::Implicit { scope, ordinal } => {
                self.implicit(owner, *scope, *ordinal)
            }
            SemanticLifeRecipe::Environment { scope, ordinal } => {
                self.environment(owner, *scope, *ordinal, Kind::Life)
            }
            SemanticLifeRecipe::Bound { depth, index } => self.bound(bound, *depth, *index),
            SemanticLifeRecipe::Invocation(index) => {
                if *index >= invocation {
                    return Err(fail(
                        "semantic invocation selector is outside the original signature",
                    ));
                }
                Ok(())
            }
            SemanticLifeRecipe::OuterOccurrence { operand, path, .. }
            | SemanticLifeRecipe::IntrinsicClass { operand, path, .. } => {
                self.node(depth.saturating_add(path.len()))?;
                self.fee(path.len())?;
                self.operand(owner, operand)
            }
        }
    }
    fn actual_recipe(
        &mut self,
        owner: SemanticDefinitionId,
        value: &SemanticActualRecipe,
        bound: &mut Vec<u32>,
        invocation: u32,
        depth: usize,
    ) -> R<()> {
        self.node(depth)?;
        match value {
            SemanticActualRecipe::Type(value) => {
                self.ty(owner, value, bound, invocation, depth + 1)
            }
            SemanticActualRecipe::Const { carrier, value } => {
                self.ty(owner, carrier, bound, invocation, depth + 1)?;
                self.constant(owner, value, bound, invocation, depth + 1)
            }
            SemanticActualRecipe::Life(value) => {
                self.life(owner, value, bound, invocation, depth + 1)
            }
        }
    }
    fn arguments(
        &mut self,
        owner: SemanticDefinitionId,
        target: SemanticDefinitionId,
        values: &[SemanticActualRecipe],
        bound: &mut Vec<u32>,
        invocation: u32,
        depth: usize,
    ) -> R<()> {
        let definition = self.definition(target)?;
        if values.len() != definition.parameters.len() {
            return Err(fail(
                "semantic application omits or adds original mixed actuals",
            ));
        }
        for (value, formal) in values.iter().zip(&definition.parameters) {
            self.node(depth)?;
            let actual_kind = match value {
                SemanticActualRecipe::Type(_) => Kind::Type,
                SemanticActualRecipe::Const { .. } => Kind::Const,
                SemanticActualRecipe::Life(_) => Kind::Life,
            };
            if actual_kind != kind(formal) {
                return Err(fail(
                    "semantic application actual kind differs from its original formal",
                ));
            }
            self.actual_recipe(owner, value, bound, invocation, depth + 1)?;
        }
        Ok(())
    }
    fn trait_recipe(
        &mut self,
        owner: SemanticDefinitionId,
        value: &SemanticTraitRecipe,
        bound: &mut Vec<u32>,
        invocation: u32,
        depth: usize,
    ) -> R<()> {
        self.node(depth)?;
        let mut target = None;
        self.fee(self.definitions.len().saturating_mul(2).saturating_add(1))?;
        for definition in self.definitions.values().copied().collect::<Vec<_>>() {
            self.fee(1)?;
            if matches!(definition.body,SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait{declaration}) if declaration==value.declaration)
            {
                if target.replace(definition.id).is_some() {
                    return Err(fail(
                        "semantic trait application has ambiguous original scope",
                    ));
                }
            }
        }
        self.arguments(
            owner,
            target.ok_or_else(|| {
                fail("semantic trait application has no original declaration scope")
            })?,
            &value.arguments,
            bound,
            invocation,
            depth + 1,
        )
    }
    fn sequence(
        &mut self,
        owner: SemanticDefinitionId,
        value: &SemanticClassSequenceRecipe,
        bound: &mut Vec<u32>,
        invocation: u32,
        depth: usize,
    ) -> R<()> {
        self.node(depth)?;
        for part in &value.parts {
            match part {
                SemanticClassSequencePart::One(value) => {
                    self.life(owner, value, bound, invocation, depth + 1)?
                }
                SemanticClassSequencePart::AllClasses { ty, path } => {
                    self.node(depth.saturating_add(path.len()))?;
                    self.fee(path.len())?;
                    self.ty(owner, ty, bound, invocation, depth + 1)?;
                }
            }
        }
        Ok(())
    }
    fn retained(
        &mut self,
        owner: SemanticDefinitionId,
        values: &[SemanticRetainedPart],
        bound: &mut Vec<u32>,
        invocation: u32,
        depth: usize,
    ) -> R<()> {
        for value in values {
            self.node(depth)?;
            match value {
                SemanticRetainedPart::One { path, parent, .. } => {
                    self.fee(path.len())?;
                    self.life(owner, parent, bound, invocation, depth + 1)?;
                }
                SemanticRetainedPart::All { path, parents } => {
                    self.fee(path.len())?;
                    self.sequence(owner, parents, bound, invocation, depth + 1)?;
                }
            }
        }
        Ok(())
    }
    fn nominal_parts(
        &mut self,
        owner: SemanticDefinitionId,
        values: &[SemanticNominalPart],
        bound: &mut Vec<u32>,
        invocation: u32,
        depth: usize,
    ) -> R<()> {
        for value in values {
            self.node(depth)?;
            match value {
                SemanticNominalPart::One { path, class, .. } => {
                    self.fee(path.len())?;
                    self.life(owner, class, bound, invocation, depth + 1)?;
                }
                SemanticNominalPart::FromType { path, ty, classes } => {
                    self.fee(path.len())?;
                    self.ty(owner, ty, bound, invocation, depth + 1)?;
                    self.sequence(owner, classes, bound, invocation, depth + 1)?;
                }
            }
        }
        Ok(())
    }
    fn retained_shape(
        &mut self,
        owner: SemanticDefinitionId,
        value: &SemanticRetainedShapeRecipe,
        bound: &mut Vec<u32>,
        invocation: u32,
        depth: usize,
    ) -> R<()> {
        self.sequence(owner, &value.domain, bound, invocation, depth)?;
        for port in &value.ports {
            self.node(depth)?;
            match port {
                SemanticPortPart::One { leaf, class, .. } => {
                    self.ty(owner, leaf, bound, invocation, depth + 1)?;
                    self.life(owner, class, bound, invocation, depth + 1)?;
                }
                SemanticPortPart::FromType { ty, classes } => {
                    self.ty(owner, ty, bound, invocation, depth + 1)?;
                    self.sequence(owner, classes, bound, invocation, depth + 1)?;
                }
            }
        }
        self.retained(owner, &value.retained, bound, invocation, depth + 1)?;
        self.nominal_parts(owner, &value.nominal, bound, invocation, depth + 1)
    }
    fn constant(
        &mut self,
        owner: SemanticDefinitionId,
        value: &SemanticConstRecipe,
        bound: &mut Vec<u32>,
        invocation: u32,
        depth: usize,
    ) -> R<()> {
        self.node(depth)?;
        match value {
            SemanticConstRecipe::Formal(reference) => self.formal(owner, reference, Kind::Const),
            SemanticConstRecipe::Environment { scope, ordinal } => {
                self.environment(owner, *scope, *ordinal, Kind::Const)
            }
            SemanticConstRecipe::Unary {
                operand_type,
                value,
                ..
            } => {
                self.ty(owner, operand_type, bound, invocation, depth + 1)?;
                self.constant(owner, value, bound, invocation, depth + 1)
            }
            SemanticConstRecipe::Binary {
                operand_type,
                left,
                right,
                ..
            } => {
                self.ty(owner, operand_type, bound, invocation, depth + 1)?;
                self.constant(owner, left, bound, invocation, depth + 1)?;
                self.constant(owner, right, bound, invocation, depth + 1)
            }
            SemanticConstRecipe::Convert {
                source_type,
                target_type,
                value,
            } => {
                self.ty(owner, source_type, bound, invocation, depth + 1)?;
                self.ty(owner, target_type, bound, invocation, depth + 1)?;
                self.constant(owner, value, bound, invocation, depth + 1)
            }
            SemanticConstRecipe::AssociatedConst {
                receiver,
                application,
                ..
            } => {
                self.ty(owner, receiver, bound, invocation, depth + 1)?;
                self.trait_recipe(owner, application, bound, invocation, depth + 1)
            }
            SemanticConstRecipe::Value(_)
            | SemanticConstRecipe::IntegerLiteral(_)
            | SemanticConstRecipe::FloatLiteral(_) => Ok(()),
        }
    }
    fn signature(
        &mut self,
        owner: SemanticDefinitionId,
        value: &SemanticSignatureRecipe,
        bound: &mut Vec<u32>,
        depth: usize,
    ) -> R<()> {
        self.node(depth)?;
        if value.bound_lifetimes > 0 {
            bound.push(value.bound_lifetimes);
        }
        for parameter in &value.parameters {
            self.ty(
                owner,
                &parameter.ty,
                bound,
                value.invocation_lifetimes,
                depth + 1,
            )?;
        }
        self.ty(
            owner,
            &value.result,
            bound,
            value.invocation_lifetimes,
            depth + 1,
        )?;
        if value.bound_lifetimes > 0 {
            bound.pop();
        }
        Ok(())
    }
    fn ty(
        &mut self,
        owner: SemanticDefinitionId,
        value: &SemanticTypeRecipe,
        bound: &mut Vec<u32>,
        invocation: u32,
        depth: usize,
    ) -> R<()> {
        self.node(depth)?;
        match value {
            SemanticTypeRecipe::Atom(value) => {
                if !matches!(
                    value,
                    Type::Unit
                        | Type::Bool
                        | Type::I8
                        | Type::I16
                        | Type::I32
                        | Type::I64
                        | Type::Isize
                        | Type::U8
                        | Type::U16
                        | Type::U32
                        | Type::U64
                        | Type::Usize
                        | Type::F32
                        | Type::F64
                        | Type::ForeignHandle
                        | Type::CWideString
                        | Type::ByteBuffer
                ) {
                    return Err(fail(
                        "semantic Atom erases a structural, generic or nominal type expression",
                    ));
                }
                Ok(())
            }
            SemanticTypeRecipe::Exact(value) => {
                if value.classes.len() != value.life.binder_count as usize {
                    return Err(fail(
                        "semantic Exact omits its complete intrinsic class images",
                    ));
                }
                for value in &value.classes {
                    self.life(owner, value, bound, invocation, depth + 1)?;
                }
                Ok(())
            }
            SemanticTypeRecipe::Formal(reference) => self.formal(owner, reference, Kind::Type),
            SemanticTypeRecipe::Environment { scope, ordinal } => {
                self.environment(owner, *scope, *ordinal, Kind::Type)
            }
            SemanticTypeRecipe::SelfType { scope } => {
                if !matches!(
                    self.scope(owner, *scope)?.body,
                    SemanticDefinitionBody::Scope(_)
                ) {
                    return Err(fail("semantic Self has no real trait/implementation owner"));
                }
                Ok(())
            }
            SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            } => {
                if !matches!(
                    self.definition(*definition)?.body,
                    SemanticDefinitionBody::Nominal { .. }
                ) {
                    return Err(fail(
                        "semantic nominal application selects a nonnominal definition",
                    ));
                }
                self.arguments(owner, *definition, arguments, bound, invocation, depth + 1)
            }
            SemanticTypeRecipe::AssociatedType {
                receiver,
                application,
                ..
            } => {
                self.ty(owner, receiver, bound, invocation, depth + 1)?;
                self.trait_recipe(owner, application, bound, invocation, depth + 1)
            }
            SemanticTypeRecipe::Tuple(values) | SemanticTypeRecipe::AnonymousSum(values) => {
                for value in values {
                    self.ty(owner, value, bound, invocation, depth + 1)?;
                }
                Ok(())
            }
            SemanticTypeRecipe::Array { element, length } => {
                self.ty(owner, element, bound, invocation, depth + 1)?;
                self.constant(owner, length, bound, invocation, depth + 1)
            }
            SemanticTypeRecipe::Slice(value)
            | SemanticTypeRecipe::Managed(value)
            | SemanticTypeRecipe::OwnedStore(value)
            | SemanticTypeRecipe::Mutex(value)
            | SemanticTypeRecipe::Registration(value)
            | SemanticTypeRecipe::AsyncRegistration(value) => {
                self.ty(owner, value, bound, invocation, depth + 1)
            }
            SemanticTypeRecipe::Address {
                inner, lifetime, ..
            }
            | SemanticTypeRecipe::Desc { inner, lifetime } => {
                if let SemanticOuterLifeRecipe::Class(life) = lifetime {
                    self.life(owner, life, bound, invocation, depth + 1)?;
                }
                self.ty(owner, inner, bound, invocation, depth + 1)
            }
            SemanticTypeRecipe::MutexGuard { payload, retained } => {
                self.ty(owner, payload, bound, invocation, depth + 1)?;
                self.retained_shape(owner, retained, bound, invocation, depth + 1)
            }
            SemanticTypeRecipe::Task { result, retained } => {
                self.ty(owner, result, bound, invocation, depth + 1)?;
                self.retained_shape(owner, retained, bound, invocation, depth + 1)
            }
            SemanticTypeRecipe::ErrorUnion { error, result } => {
                self.ty(owner, error, bound, invocation, depth + 1)?;
                self.ty(owner, result, bound, invocation, depth + 1)
            }
            SemanticTypeRecipe::Qualified { storage, life } => {
                self.ty(owner, storage, bound, invocation, depth + 1)?;
                self.sequence(owner, &life.domain, bound, invocation, depth + 1)?;
                for slot in &life.slots {
                    self.node(depth.saturating_add(slot.path.len()))?;
                    self.fee(slot.path.len())?;
                    self.life(owner, &slot.class, bound, invocation, depth + 1)?;
                }
                self.retained(owner, &life.retained, bound, invocation, depth + 1)?;
                self.nominal_parts(owner, &life.nominal, bound, invocation, depth + 1)
            }
            SemanticTypeRecipe::Code { signature, .. } => {
                self.signature(owner, signature, bound, depth + 1)
            }
            SemanticTypeRecipe::Closure {
                definition,
                arguments,
                environment,
            } => {
                let target = self.definition(*definition)?;
                if target.kind != SemanticDefinitionKind::Closure
                    || target.environment.len() != environment.len()
                {
                    return Err(fail(
                        "semantic closure recipe omits its original environment or selects another kind",
                    ));
                }
                self.arguments(owner, *definition, arguments, bound, invocation, depth + 1)?;
                for (actual, formal) in environment.iter().zip(&target.environment) {
                    let actual_kind = match actual {
                        SemanticActualRecipe::Type(_) => Kind::Type,
                        SemanticActualRecipe::Const { .. } => Kind::Const,
                        SemanticActualRecipe::Life(_) => Kind::Life,
                    };
                    if actual_kind != kind(&formal.kind) {
                        return Err(fail(
                            "semantic closure environment actual has the wrong original kind",
                        ));
                    }
                    self.actual_recipe(owner, actual, bound, invocation, depth + 1)?;
                }
                Ok(())
            }
            SemanticTypeRecipe::Dyn { interface, .. } => {
                self.trait_recipe(owner, &interface.application, bound, invocation, depth + 1)?;
                if let SemanticDynContextRecipe::Type(value) = &interface.context {
                    self.ty(owner, value, bound, invocation, depth + 1)?;
                }
                for binding in &interface.associated {
                    self.trait_recipe(owner, &binding.application, bound, invocation, depth + 1)?;
                    match &binding.value {
                        SemanticAssociatedRecipe::Type(value) => {
                            self.ty(owner, value, bound, invocation, depth + 1)?
                        }
                        SemanticAssociatedRecipe::Const { carrier, value } => {
                            self.ty(owner, carrier, bound, invocation, depth + 1)?;
                            self.constant(owner, value, bound, invocation, depth + 1)?;
                        }
                    }
                }
                Ok(())
            }
            SemanticTypeRecipe::Opaque {
                definition,
                arguments,
                hidden,
            } => {
                self.arguments(owner, *definition, arguments, bound, invocation, depth + 1)?;
                self.ty(owner, hidden, bound, invocation, depth + 1)
            }
            SemanticTypeRecipe::UnresolvedNamed(_) | SemanticTypeRecipe::Unsupported(_) => {
                Err(fail("unresolved semantic type recipe cannot be executable"))
            }
        }
    }
}

impl Check<'_, '_> {
    fn requirement(
        &mut self,
        owner: SemanticDefinitionId,
        value: &SemanticRequirementRecipe,
        bound: &mut Vec<u32>,
        invocation: u32,
        depth: usize,
    ) -> R<()> {
        self.node(depth)?;
        match value {
            SemanticRequirementRecipe::Structural { ty, .. } => {
                self.ty(owner, ty, bound, invocation, depth + 1)
            }
            SemanticRequirementRecipe::Trait { ty, application } => {
                self.ty(owner, ty, bound, invocation, depth + 1)?;
                self.trait_recipe(owner, application, bound, invocation, depth + 1)
            }
            SemanticRequirementRecipe::Callable { ty, signature, .. } => {
                self.ty(owner, ty, bound, invocation, depth + 1)?;
                self.signature(owner, signature, bound, depth + 1)
            }
            SemanticRequirementRecipe::TypeEqual { left, right } => {
                self.ty(owner, left, bound, invocation, depth + 1)?;
                self.ty(owner, right, bound, invocation, depth + 1)
            }
            SemanticRequirementRecipe::ConstEqual {
                carrier,
                left,
                right,
            } => {
                self.ty(owner, carrier, bound, invocation, depth + 1)?;
                self.constant(owner, left, bound, invocation, depth + 1)?;
                self.constant(owner, right, bound, invocation, depth + 1)
            }
            SemanticRequirementRecipe::LifeEqual { left, right } => {
                self.life(owner, left, bound, invocation, depth + 1)?;
                self.life(owner, right, bound, invocation, depth + 1)
            }
            SemanticRequirementRecipe::Outlives { longer, shorter } => {
                self.life(owner, longer, bound, invocation, depth + 1)?;
                self.life(owner, shorter, bound, invocation, depth + 1)
            }
            SemanticRequirementRecipe::ForAll {
                lifetimes,
                requirements,
            } => {
                self.fee(1)?;
                if *lifetimes > 0 {
                    bound.push(*lifetimes);
                }
                for value in requirements {
                    self.requirement(owner, value, bound, invocation, depth + 1)?;
                }
                if *lifetimes > 0 {
                    bound.pop();
                }
                Ok(())
            }
        }
    }
    fn formal_recipe(
        &mut self,
        owner: SemanticDefinitionId,
        value: &SemanticFormal,
        bound: &mut Vec<u32>,
        depth: usize,
    ) -> R<()> {
        self.node(depth)?;
        match value {
            SemanticFormal::Type { requirements } => {
                for value in requirements {
                    self.requirement(owner, value, bound, 0, depth + 1)?;
                }
                Ok(())
            }
            SemanticFormal::Const { carrier } => self.ty(owner, carrier, bound, 0, depth + 1),
            SemanticFormal::Lifetime => Ok(()),
        }
    }
    fn original(&mut self, definition: &SemanticDefinition) -> R<()> {
        let owner = definition.id;
        let mut ancestors = BTreeSet::new();
        let mut next = Some(owner);
        while let Some(id) = next {
            self.node(ancestors.len())?;
            self.fee(ancestors.len().saturating_add(1))?;
            if !ancestors.insert(id) {
                return Err(fail("semantic declaration lexical parents form a cycle"));
            }
            next = self.definition(id)?.parent;
        }
        let mut implicit = BTreeSet::new();
        let mut paths = BTreeSet::new();
        for occurrence in &definition.implicit_lifetimes {
            self.node(occurrence.occurrence_path.len())?;
            self.fee(
                occurrence
                    .occurrence_path
                    .len()
                    .saturating_add(implicit.len())
                    .saturating_add(paths.len())
                    .saturating_add(2),
            )?;
            if !implicit.insert(occurrence.ordinal) || !paths.insert(&occurrence.occurrence_path) {
                return Err(fail(
                    "semantic implicit source occurrences or ordinals repeat",
                ));
            }
        }
        let mut bound = Vec::new();
        for formal in &definition.parameters {
            self.formal_recipe(owner, formal, &mut bound, 0)?;
        }
        for environment in &definition.environment {
            // Both the kind (including a const carrier) and its original
            // source are written in the real creator's lexical domain.
            // Child ordinal zero cannot stand in for creator ordinal zero.
            let creator = definition
                .parent
                .ok_or_else(|| fail("semantic environment has no actual lexical creator"))?;
            self.formal_recipe(creator, &environment.kind, &mut bound, 0)?;
            let actual_kind = match environment.source {
                SemanticActualRecipe::Type(_) => Kind::Type,
                SemanticActualRecipe::Const { .. } => Kind::Const,
                SemanticActualRecipe::Life(_) => Kind::Life,
            };
            if actual_kind != kind(&environment.kind) {
                return Err(fail(
                    "semantic environment original source kind differs from its formal",
                ));
            }
            self.actual_recipe(creator, &environment.source, &mut bound, 0, 0)?;
        }
        for value in &definition.requirements {
            self.requirement(owner, value, &mut bound, 0, 0)?;
        }
        match &definition.body {
            SemanticDefinitionBody::Function { signature, sites } => {
                if !matches!(
                    definition.kind,
                    SemanticDefinitionKind::Function
                        | SemanticDefinitionKind::Method
                        | SemanticDefinitionKind::Closure
                        | SemanticDefinitionKind::TaskEntry
                ) {
                    return Err(fail(
                        "semantic executable signature belongs to a nonfunction declaration",
                    ));
                }
                self.signature(owner, signature, &mut bound, 0)?;
                if signature.bound_lifetimes > 0 {
                    bound.push(signature.bound_lifetimes);
                }
                let mut ids = BTreeSet::new();
                for site in sites {
                    self.fee(ids.len().saturating_add(1))?;
                    if !ids.insert(site.id) {
                        return Err(fail(
                            "semantic original site identity repeats inside its definition",
                        ));
                    }
                    self.node(site.occurrence_path.len())?;
                    self.fee(site.occurrence_path.len())?;
                    match &site.kind {
                        SemanticSiteRecipeKind::CallableCall { callee: result }
                        | SemanticSiteRecipeKind::Construct { result }
                        | SemanticSiteRecipeKind::TypeUse { expected: result }
                        | SemanticSiteRecipeKind::DeclareLocal { expected: result } => {
                            self.ty(owner, result, &mut bound, signature.invocation_lifetimes, 0)?
                        }
                        SemanticSiteRecipeKind::Call { target, actuals } => {
                            if !matches!(
                                self.definition(*target)?.body,
                                SemanticDefinitionBody::Function { .. }
                            ) {
                                return Err(fail(
                                    "semantic static call selects a nonfunction original definition",
                                ));
                            }
                            self.arguments(
                                owner,
                                *target,
                                actuals,
                                &mut bound,
                                signature.invocation_lifetimes,
                                0,
                            )?;
                        }
                        SemanticSiteRecipeKind::Closure {
                            definition,
                            environment,
                        } => {
                            let target = self.definition(*definition)?;
                            if target.kind != SemanticDefinitionKind::Closure
                                || target.parent != Some(owner)
                                || target.environment.len() != environment.len()
                            {
                                return Err(fail(
                                    "semantic closure site disagrees with its original lexical declaration",
                                ));
                            }
                            for (actual, formal) in environment.iter().zip(&target.environment) {
                                self.node(0)?;
                                let actual_kind = match actual {
                                    SemanticActualRecipe::Type(_) => Kind::Type,
                                    SemanticActualRecipe::Const { .. } => Kind::Const,
                                    SemanticActualRecipe::Life(_) => Kind::Life,
                                };
                                if actual_kind != kind(&formal.kind) {
                                    return Err(fail(
                                        "semantic original closure environment has wrong mixed kinds",
                                    ));
                                }
                                self.actual_recipe(
                                    owner,
                                    actual,
                                    &mut bound,
                                    signature.invocation_lifetimes,
                                    0,
                                )?;
                            }
                        }
                    }
                }
                if signature.bound_lifetimes > 0 {
                    bound.pop();
                }
            }
            SemanticDefinitionBody::Nominal {
                declaration,
                kind: nominal_kind,
                body,
            } => {
                if definition.kind != SemanticDefinitionKind::Nominal {
                    return Err(fail("semantic nominal body has the wrong declaration kind"));
                }
                let mut found = 0usize;
                for module in self.modules {
                    self.fee(module.life_declarations.len())?;
                    for actual in &module.life_declarations {
                        if actual.id != *declaration {
                            continue;
                        }
                        found += 1;
                        if actual.identity != definition.identity
                            || actual.kind != *nominal_kind
                            || actual.parameters.len() != definition.parameters.len()
                        {
                            return Err(fail(
                                "semantic nominal original declaration identity or full mixed arity differs",
                            ));
                        }
                        for (actual, formal) in actual.parameters.iter().zip(&definition.parameters)
                        {
                            self.fee(1)?;
                            let actual_kind = match actual {
                                LifeParameterKind::Type => Kind::Type,
                                LifeParameterKind::Const(_) => Kind::Const,
                                LifeParameterKind::Lifetime => Kind::Life,
                            };
                            if actual_kind != kind(formal) {
                                return Err(fail(
                                    "semantic nominal original mixed formal kinds differ",
                                ));
                            }
                        }
                    }
                }
                if found != 1 {
                    return Err(fail(
                        "semantic nominal body has no unique actual original declaration",
                    ));
                }
                match body {
                    SemanticNominalBody::Product { fields } => {
                        let mut names = BTreeSet::new();
                        for field in fields {
                            self.fee(
                                field
                                    .name
                                    .len()
                                    .saturating_add(names.len())
                                    .saturating_add(1),
                            )?;
                            if !names.insert(&field.name) {
                                return Err(fail("semantic original nominal field repeats"));
                            }
                            self.ty(owner, &field.ty, &mut bound, 0, 0)?;
                        }
                    }
                    SemanticNominalBody::Enum { variants } => {
                        let mut names = BTreeSet::new();
                        for variant in variants {
                            self.fee(
                                variant
                                    .name
                                    .len()
                                    .saturating_add(names.len())
                                    .saturating_add(1),
                            )?;
                            if !names.insert(&variant.name) {
                                return Err(fail("semantic original nominal variant repeats"));
                            }
                            for field in &variant.fields {
                                self.ty(owner, field, &mut bound, 0, 0)?;
                            }
                        }
                    }
                }
            }
            SemanticDefinitionBody::Scope(scope) => {
                if definition.kind != SemanticDefinitionKind::Scope {
                    return Err(fail(
                        "semantic lexical scope recipe has the wrong declaration kind",
                    ));
                }
                match scope {
                    SemanticScopeRecipe::Trait { .. } => {}
                    SemanticScopeRecipe::InherentImpl { self_type, .. } => {
                        self.ty(owner, self_type, &mut bound, 0, 0)?
                    }
                    SemanticScopeRecipe::TraitImpl {
                        self_type,
                        trait_application,
                        ..
                    } => {
                        self.ty(owner, self_type, &mut bound, 0, 0)?;
                        self.trait_recipe(owner, trait_application, &mut bound, 0, 0)?;
                    }
                }
            }
        }
        Ok(())
    }
}

/// Validate original definitions before interpreting or specializing them.
/// Instance, emitted-site and actual-class validation are separate mandatory
/// consumers; this pass cannot publish a complete Typed semantic program.
pub(crate) fn originals(modules: &[&CoreModule], limits: &Limits, left: &mut usize) -> R<()> {
    let mut definitions = BTreeMap::new();
    let mut instances = BTreeMap::new();
    for core in modules {
        let Some(program) = &core.semantic else {
            continue;
        };
        crate::dyn_metadata::semantic_snapshot(program, limits, left, 1)?;
        let rows = program
            .definitions
            .len()
            .saturating_add(program.instances.len());
        *left = left
            .checked_sub(rows.saturating_mul(3).saturating_add(1))
            .ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "semantic definition inventory budget exceeded",
                    None,
                )
            })?;
        for definition in &program.definitions {
            *left = left
                .checked_sub(definitions.len().saturating_add(1).ilog2() as usize + 1)
                .ok_or_else(|| {
                    Diagnostic::limit(
                        Phase::Types,
                        "semantic definition index budget exceeded",
                        None,
                    )
                })?;
            if definitions.insert(definition.id, definition).is_some() {
                return Err(fail(
                    "semantic definition identity is duplicated across the complete program",
                ));
            }
        }
        for instance in &program.instances {
            *left = left
                .checked_sub(instances.len().saturating_add(1).ilog2() as usize + 1)
                .ok_or_else(|| {
                    Diagnostic::limit(
                        Phase::Types,
                        "semantic instance index budget exceeded",
                        None,
                    )
                })?;
            if instances.insert(instance.id, instance).is_some() {
                return Err(fail(
                    "semantic instance identity is duplicated across the complete program",
                ));
            }
        }
    }
    let mut check = Check {
        definitions,
        instances,
        modules,
        limits,
        left,
    };
    for core in modules {
        let Some(program) = &core.semantic else {
            continue;
        };
        for definition in &program.definitions {
            check.original(definition)?;
        }
        for instance in &program.instances {
            check.node(0)?;
            let definition = check.definition(instance.definition)?;
            if definition.parameters.len() != instance.actuals.len() {
                return Err(fail("semantic instance omits original mixed actuals"));
            }
            let mut chain = BTreeSet::new();
            let mut next = Some(instance.id);
            while let Some(id) = next {
                check.node(chain.len())?;
                check.fee(chain.len().saturating_add(1))?;
                if !chain.insert(id) {
                    return Err(fail("semantic instance lexical parents form a cycle"));
                }
                next = check.instance(id)?.parent;
            }
            match (definition.parent, instance.parent) {
                (None, None) => {}
                (Some(expected), Some(actual))
                    if check.instance(actual)?.definition == expected => {}
                _ => {
                    return Err(fail(
                        "semantic instance lexical parent disagrees with its original declaration",
                    ));
                }
            }
            if let SemanticLifetimeInstantiation::Partition { rigid } =
                instance.lifetime_instantiation
            {
                let original = check.instance(rigid)?;
                if original.id == instance.id
                    || original.definition != instance.definition
                    || original.lifetime_instantiation != SemanticLifetimeInstantiation::Rigid
                {
                    return Err(fail(
                        "semantic partition lacks another actual rigid representative",
                    ));
                }
                if original.target == instance.target {
                    return Err(fail(
                        "semantic partition and rigid representative must have separate real targets",
                    ));
                }
            }
        }
    }
    Ok(())
}
