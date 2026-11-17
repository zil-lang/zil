//! Untrusted ABI upper candidates over the complete original operation graph.
//! This pass does not validate effects, callable premises or actual code. In
//! particular, an explicit Direct declaration is never upgraded to repair its
//! body. The independent consumers must check that body and its actual ABI.
use super::*;

#[path = "source_semantic_cleanup_candidates.rs"]
mod cleanup_candidates;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Definition(usize),
    AppliedDefinition { owner: usize, application: usize },
    Kind(CallKind),
    // Used only by the original occurrence observer, never ABI inference.
    Prototype(usize, CallKind),
    PrototypeSequence { index: usize, dimensions: u32, kind: CallKind },
    Alternatives(usize),
    Elements { index: usize, tuple: bool },
    StoreBorrow,
    OwnedDynFactory,
    StoreReceiver(usize),
    TraitReceiver(LifeDeclarationId),
    NominalReceiver(usize),
    FormalReceiver { owner: usize, ordinal: u32 },
    FallibleTrait(LifeDeclarationId),
    FallibleNominal(usize),
    LengthReceiver,
    CodeSequence { dimensions: u32, kind: CallKind },
    Unknown,
}
// References retain the complete mixed actuals in their original scope. An
// application is not a specialized type or evidence that its bounds hold.
#[derive(Clone, Copy)]
enum Arguments<'a> {
    Semantic(&'a [SemanticActualRecipe]),
    Types(&'a [SemanticTypeRecipe]),
    Written(&'a [CoreGenericArgument<SemanticTypeRecipe>]),
}
#[derive(Clone, Copy)]
struct Application<'a> {
    declaration: usize,
    arguments: Arguments<'a>,
    owner: usize,
    context: Option<usize>,
}
#[derive(Clone, Copy)]
enum Prototype<'a> {
    Code { owner: usize, signature: &'a SemanticSignatureRecipe, context: Option<usize>, frames: Option<usize> },
    Formal { owner: usize, ordinal: u32 },
    Premises { owner: usize, ty: &'a SemanticTypeRecipe },
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct InputRole { owner: usize, ordinal: u32 }

struct Vertex {
    explicit: Option<CallKind>,
    suspend: bool,
    edges: Vec<usize>,
    consumes: Vec<InputRole>,
    forwards: Vec<(InputRole, InputRole)>,
}
struct Scan<'a, 'd, 'b, 'l> {
    definitions: &'d [&'a SemanticDefinition],
    owner: usize,
    locals: Vec<(&'a str, Target)>,
    captures: &'b mut Vec<(usize, Vec<(&'a str, Target)>)>,
    applications: &'b mut Vec<Application<'a>>,
    store_elements: &'b mut Vec<Target>,
    vertex: &'b mut Vertex,
    budget: &'b mut Budget<'l>,
    prototypes: &'b mut Vec<Prototype<'a>>,
    alternatives: &'b mut Vec<(Target, Target)>,
    quantifiers: &'b mut Vec<Vec<u32>>,
    elements: &'b mut Vec<Vec<Target>>,
    active_quantifiers: Option<usize>,
    observations: Option<&'b mut Vec<call_regions::Observation>>,
    call: Option<zil_base::NodeId>,
    collect_targets: bool,
    resolved_targets: Vec<Target>,
}
fn signature(definition: &SemanticDefinition) -> Option<&SemanticSignatureRecipe> {
    match &definition.body {
        SemanticDefinitionBody::FunctionWithBody { signature, .. }
        | SemanticDefinitionBody::Function { signature, .. } => Some(signature),
        _ => None,
    }
}
fn declaration_index(
    definitions: &[&SemanticDefinition],
    id: SemanticDefinitionId,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<Option<usize>> {
    budget.identity_work(definitions.len(), span)?;
    Ok(definitions
        .iter()
        .position(|definition| definition.id == id))
}
// Only a real by-value original input can acquire a consuming upper. This
// excludes opaque result promises, code signatures and non-consuming roles.
fn input_formal(ty: &SemanticTypeRecipe) -> Option<u32> {
    match ty {
        SemanticTypeRecipe::Formal(SemanticFormalRef { scope: SemanticScopeRef::Ancestor(0), ordinal }) => Some(*ordinal),
        _ => None,
    }
}
fn once_requirements(
    requirements: &[SemanticRequirementRecipe], ordinal: u32,
    found: &mut bool, other: &mut bool, budget: &mut Budget<'_>, depth: usize,
) -> R<()> {
    for requirement in requirements {
        budget.node(Span::default(), depth)?;
        match requirement {
            SemanticRequirementRecipe::Callable { ty, kind, .. } if input_formal(ty) == Some(ordinal) => {
                if *kind == CallableKind::FnOnce { *found = true; } else { *other = true; }
            }
            SemanticRequirementRecipe::ForAll { requirements, .. } =>
                once_requirements(requirements, ordinal, found, other, budget, depth + 1)?,
            _ => {}
        }
    }
    Ok(())
}
fn input_once(definition: &SemanticDefinition, ordinal: u32, budget: &mut Budget<'_>) -> R<bool> {
    let Some(signature) = signature(definition) else { return Ok(false); };
    budget.identity_work(signature.parameters.len(), Span::default())?;
    if !signature.parameters.iter().any(|parameter| input_formal(&parameter.ty) == Some(ordinal)) {
        return Ok(false);
    }
    let mut found = false;
    let mut other = false;
    if let Some(SemanticFormal::Type { requirements }) = definition.parameters.get(ordinal as usize) {
        once_requirements(requirements, ordinal, &mut found, &mut other, budget, 0)?;
    }
    once_requirements(&definition.requirements, ordinal, &mut found, &mut other, budget, 0)?;
    Ok(found && !other)
}
fn promote_once(
    requirements: &mut [SemanticRequirementRecipe], ordinal: u32,
    budget: &mut Budget<'_>, depth: usize,
) -> R<bool> {
    let mut changed = false;
    for requirement in requirements {
        budget.node(Span::default(), depth)?;
        match requirement {
            SemanticRequirementRecipe::Callable { ty, kind: CallableKind::FnOnce, signature }
                if input_formal(ty) == Some(ordinal) => {
                changed |= signature.call_kind != CallKind::Suspend;
                signature.call_kind = CallKind::Suspend;
            }
            SemanticRequirementRecipe::ForAll { requirements, .. } =>
                changed |= promote_once(requirements, ordinal, budget, depth + 1)?,
            _ => {}
        }
    }
    Ok(changed)
}
impl<'a> Scan<'a, '_, '_, '_> {
    fn input_role(&mut self, target: Target, span: Span) -> R<Option<InputRole>> {
        self.budget.node(span, 0)?;
        let Target::Prototype(index, _) = target else { return Ok(None); };
        let Prototype::Formal { owner, ordinal } = self.prototypes[index] else { return Ok(None); };
        Ok(input_once(self.definitions[owner], ordinal, self.budget)?
            .then_some(InputRole { owner, ordinal }))
    }
    fn forwarded_inputs(
        &mut self, target: Target, arguments: &'a [SemanticOriginalExpr], receiver: bool, span: Span, depth: usize,
    ) -> R<()> {
        if self.observations.is_some() { return Ok(()); }
        let owner = match target {
            Target::Definition(owner) | Target::AppliedDefinition { owner, .. } => owner,
            _ => return Ok(()),
        };
        let Some(signature) = signature(self.definitions[owner]) else { return Ok(()); };
        // An actual receiver occupies its original value-parameter ordinal.
        // No missing receiver/spread is guessed from an equal type or spelling.
        let start = if signature.parameters.len() == arguments.len() { 0 }
            else if receiver && signature.parameters.len() == arguments.len().saturating_add(1)
                && matches!(&self.definitions[owner].body,
                    SemanticDefinitionBody::FunctionWithBody { receiver_parameter: Some(0), .. }) { 1 }
            else { return Ok(()); };
        for (parameter, argument) in signature.parameters[start..].iter().zip(arguments) {
            self.budget.node(argument.span, depth)?;
            let Some(ordinal) = input_formal(&parameter.ty) else { continue; };
            if !input_once(self.definitions[owner], ordinal, self.budget)? { continue; }
            let actual = self.target(argument, depth + 1)?;
            let mut pending = Vec::new();
            self.budget.lineage_push(&mut pending, (actual, depth + 1), span)?;
            while let Some((actual, depth)) = pending.pop() {
                self.budget.node(span, depth)?;
                if let Target::Alternatives(index) = actual {
                    let (left, right) = self.alternatives[index];
                    self.budget.lineage_push(&mut pending, (left, depth + 1), span)?;
                    self.budget.lineage_push(&mut pending, (right, depth + 1), span)?;
                } else if let Some(source) = self.input_role(actual, argument.span)? {
                    self.budget.lineage_push(&mut self.vertex.forwards,
                        (source, InputRole { owner, ordinal }), span)?;
                }
            }
            self.budget.drop_lineage_vec(pending, span)?;
        }
        Ok(())
    }
    fn requirement_prototypes(
        &mut self, ty: &SemanticTypeRecipe, requirements: &'a [SemanticRequirementRecipe],
        frames: &mut Vec<u32>, found: &mut Vec<(&'a SemanticSignatureRecipe, Vec<u32>)>,
        depth: usize, span: Span,
    ) -> R<()> {
        for requirement in requirements {
            self.budget.node(span, depth)?;
            match requirement {
                SemanticRequirementRecipe::Callable { ty: receiver, signature, .. }
                    if receiver == ty || matches!((receiver, ty),
                        (SemanticTypeRecipe::OriginalType { scope: SemanticScopeRef::Ancestor(0), source: left },
                         SemanticTypeRecipe::OriginalType { source: right, .. }) if left == right) => {
                    self.budget.lineage_bytes(std::mem::size_of_val(frames.as_slice()), span)?;
                    self.budget.lineage_push(found, (signature.as_ref(), frames.clone()), span)?;
                }
                SemanticRequirementRecipe::ForAll { lifetimes, requirements } => {
                    self.budget.lineage_push(frames, *lifetimes, span)?;
                    self.requirement_prototypes(ty, requirements, frames, found, depth + 1, span)?;
                    frames.pop();
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn formal_prototypes(
        &mut self, owner: usize, ordinal: u32,
        found: &mut Vec<(&'a SemanticSignatureRecipe, Vec<u32>)>, depth: usize, span: Span,
    ) -> R<()> {
        let definition = self.definitions[owner];
        let ty = SemanticTypeRecipe::Formal(SemanticFormalRef { scope: SemanticScopeRef::Ancestor(0), ordinal });
        let mut frames = Vec::new();
        if let Some(SemanticFormal::Type { requirements }) = definition.parameters.get(ordinal as usize) {
            self.requirement_prototypes(&ty, requirements, &mut frames, found, depth + 1, span)?;
        }
        self.requirement_prototypes(&ty, &definition.requirements, &mut frames, found, depth + 1, span)?;
        self.budget.drop_lineage_vec(frames, span)
    }
    fn premise_prototypes(
        &mut self, owner: usize, ty: &SemanticTypeRecipe,
        found: &mut Vec<(&'a SemanticSignatureRecipe, Vec<u32>)>, depth: usize, span: Span,
    ) -> R<()> {
        let mut frames = Vec::new();
        self.requirement_prototypes(ty, &self.definitions[owner].requirements, &mut frames, found, depth + 1, span)?;
        self.budget.drop_lineage_vec(frames, span)
    }
    fn result_target(
        &mut self, owner: usize, signature: &'a SemanticSignatureRecipe, context: Option<usize>,
        outer_frames: &[u32], depth: usize, span: Span,
    ) -> R<Target> {
        let mut frames = Vec::new();
        for count in outer_frames.iter().copied().chain(std::iter::once(signature.bound_lifetimes)) {
            self.budget.lineage_push(&mut frames, count, span)?;
        }
        let previous = self.active_quantifiers;
        let index = self.quantifiers.len();
        self.budget.lineage_push(self.quantifiers, frames, span)?;
        self.active_quantifiers = Some(index);
        let result = self.bound_type(owner, &signature.result, context, depth + 1, span);
        self.active_quantifiers = previous;
        result
    }
    fn observe_target(&mut self, call: zil_base::NodeId, target: Target, span: Span) -> R<()> {
        if self.observations.is_none() { return Ok(()); }
        let mut pending = Vec::new();
        self.budget.lineage_push(&mut pending, target, span)?;
        while let Some(target) = pending.pop() {
            self.budget.node(span, 0)?;
            if let Target::Alternatives(index) = target {
                let (left, right) = self.alternatives[index];
                self.budget.lineage_push(&mut pending, left, span)?;
                self.budget.lineage_push(&mut pending, right, span)?;
            } else { self.observe_single_target(call, target, span)?; }
        }
        self.budget.drop_lineage_vec(pending, span)
    }
    fn observe_single_target(&mut self, call: zil_base::NodeId, target: Target, span: Span) -> R<()> {
        let caller = self.definitions[self.owner].id;
        match target {
            Target::Definition(owner) | Target::AppliedDefinition { owner, .. } => {
                if let Some(signature) = signature(self.definitions[owner]) {
                    call_regions::signature(self.observations.as_deref_mut().unwrap(), caller, call,
                        Some(self.definitions[owner]), signature, &[], self.budget, span)?;
                }
            }
            Target::Prototype(index, _) => match self.prototypes[index] {
                Prototype::Code { signature, frames, .. } => {
                    let frames = frames.map_or(&[][..], |index| self.quantifiers[index].as_slice());
                    call_regions::signature(self.observations.as_deref_mut().unwrap(), caller, call,
                        None, signature, frames, self.budget, span)?;
                }
                prototype @ (Prototype::Formal { .. } | Prototype::Premises { .. }) => {
                    let mut found = Vec::new();
                    match prototype {
                        Prototype::Formal { owner, ordinal } => self.formal_prototypes(owner, ordinal, &mut found, 0, span)?,
                        Prototype::Premises { owner, ty } => self.premise_prototypes(owner, ty, &mut found, 0, span)?,
                        Prototype::Code { .. } => unreachable!(),
                    }
                    for (signature, frames) in &found {
                        call_regions::signature(self.observations.as_deref_mut().unwrap(), caller, call,
                            None, signature, frames, self.budget, span)?;
                    }
                    for (_, frames) in &mut found { self.budget.drop_lineage_vec(std::mem::take(frames), span)?; }
                    self.budget.drop_lineage_vec(found, span)?;
                }
            },
            _ => {}
        }
        Ok(())
    }
    fn edge(&mut self, target: Target, span: Span) -> R<()> {
        let mut pending = Vec::new();
        self.budget.lineage_push(&mut pending, (target, 0usize), span)?;
        while let Some((target, depth)) = pending.pop() {
            self.budget.node(span, depth)?;
            if let Target::Alternatives(index) = target {
                let (left, right) = self.alternatives[index];
                self.budget.lineage_push(&mut pending, (left, depth + 1), span)?;
                self.budget.lineage_push(&mut pending, (right, depth + 1), span)?;
            } else { self.edge_one(target, span)?; }
        }
        self.budget.drop_lineage_vec(pending, span)
    }
    fn edge_one(&mut self, target: Target, span: Span) -> R<()> {
        if self.collect_targets { self.budget.lineage_push(&mut self.resolved_targets, target, span)?; }
        if let Some(call) = self.call {
            self.observe_target(call, target, span)?;
        }
        if self.observations.is_none() {
            if let Some(role) = self.input_role(target, span)? {
                self.budget.lineage_push(&mut self.vertex.consumes, role, span)?;
            }
        }
        match target {
            Target::Kind(CallKind::Direct) | Target::Prototype(_, CallKind::Direct) | Target::StoreBorrow | Target::OwnedDynFactory => {}
            Target::Kind(CallKind::Suspend)
            | Target::Prototype(_, CallKind::Suspend)
            | Target::Unknown
            | Target::TraitReceiver(_)
            | Target::NominalReceiver(_)
            | Target::FormalReceiver { .. }
            | Target::FallibleTrait(_)
            | Target::FallibleNominal(_)
            | Target::LengthReceiver
            | Target::CodeSequence { .. }
            | Target::PrototypeSequence { .. }
            | Target::Alternatives(_)
            | Target::Elements { .. } => self.vertex.suspend = true,
            Target::StoreReceiver(_) => self.vertex.suspend = true,
            Target::Definition(index) | Target::AppliedDefinition { owner: index, .. } => {
                self.budget.identity_work(self.vertex.edges.len(), span)?;
                if !self.vertex.edges.contains(&index) {
                    self.budget
                        .lineage_push(&mut self.vertex.edges, index, span)?;
                }
            }
        }
        Ok(())
    }
    fn bind(&mut self, name: &'a str, target: Target, span: Span) -> R<()> {
        self.budget
            .lineage_push(&mut self.locals, (name, target), span)
    }
    fn scope(
        &mut self,
        mut owner: usize,
        reference: &SemanticScopeRef,
        span: Span,
    ) -> R<Option<usize>> {
        let SemanticScopeRef::Ancestor(distance) = reference;
        for _ in 0..*distance {
            self.budget.node(span, 0)?;
            self.budget.identity_work(self.definitions.len(), span)?;
            let Some(parent) = self.definitions[owner].parent else {
                return Ok(None);
            };
            // A lexical walk is bounded by the complete original inventory.
            let Some(next) = self
                .definitions
                .iter()
                .position(|definition| definition.id == parent)
            else {
                return Err(error(span, "effect candidate lexical parent is absent"));
            };
            owner = next;
        }
        Ok(Some(owner))
    }
    fn callable_type(
        &mut self,
        owner: usize,
        ty: &'a SemanticTypeRecipe,
        depth: usize,
        span: Span,
    ) -> R<Target> {
        self.bound_type(owner, ty, None, depth, span)
    }
    fn application(
        &mut self,
        declaration: usize,
        owner: usize,
        arguments: Arguments<'a>,
        context: Option<usize>,
        span: Span,
    ) -> R<Option<usize>> {
        let parameters = &self.definitions[declaration].parameters;
        let count = match arguments {
            Arguments::Semantic(values) => values.len(),
            Arguments::Types(values) => values.len(),
            Arguments::Written(values) => values.len(),
        };
        self.budget
            .identity_work(count.saturating_add(parameters.len()), span)?;
        if count != parameters.len() {
            return Ok(None);
        }
        for (ordinal, formal) in parameters.iter().enumerate() {
            let correct = match (formal, arguments) {
                (SemanticFormal::Type { .. }, Arguments::Semantic(values)) => {
                    matches!(values[ordinal], SemanticActualRecipe::Type(_))
                }
                (SemanticFormal::Lifetime, Arguments::Semantic(values)) => {
                    matches!(values[ordinal], SemanticActualRecipe::Life(_))
                }
                (SemanticFormal::Const { .. }, Arguments::Semantic(values)) => {
                    matches!(values[ordinal], SemanticActualRecipe::Const { .. })
                }
                (SemanticFormal::Type { .. }, Arguments::Types(_)) => true,
                (SemanticFormal::Type { .. }, Arguments::Written(values)) => {
                    matches!(values[ordinal], CoreGenericArgument::Type(_))
                }
                (SemanticFormal::Lifetime, Arguments::Written(values)) => {
                    matches!(values[ordinal], CoreGenericArgument::Lifetime(_))
                }
                (SemanticFormal::Const { .. }, Arguments::Written(values)) => {
                    matches!(values[ordinal], CoreGenericArgument::Const(_))
                }
                _ => false,
            };
            if !correct {
                return Ok(None);
            }
        }
        let index = self.applications.len();
        self.budget.lineage_push(
            self.applications,
            Application {
                declaration,
                arguments,
                owner,
                context,
            },
            span,
        )?;
        Ok(Some(index))
    }
    fn bound_type(
        &mut self,
        owner: usize,
        ty: &'a SemanticTypeRecipe,
        context: Option<usize>,
        depth: usize,
        span: Span,
    ) -> R<Target> {
        self.budget.node(span, depth)?;
        Ok(match ty {
            SemanticTypeRecipe::Tuple(fields) => {
                let mut values = Vec::new();
                for field in fields {
                    let target = self.bound_type(owner, field, context, depth + 1, span)?;
                    self.budget.lineage_push(&mut values, target, span)?;
                }
                let index = self.elements.len();
                self.budget.lineage_push(self.elements, values, span)?;
                Target::Elements { index, tuple: true }
            }
            SemanticTypeRecipe::Code { signature, .. } => {
                if self.observations.is_some() {
                    let index = self.prototypes.len();
                    self.budget.lineage_push(self.prototypes, Prototype::Code { owner, signature, context, frames: self.active_quantifiers }, span)?;
                    Target::Prototype(index, signature.call_kind)
                } else { Target::Kind(signature.call_kind) }
            },
            SemanticTypeRecipe::Managed(inner)
            | SemanticTypeRecipe::Address { inner, .. }
            | SemanticTypeRecipe::Desc { inner, .. }
            | SemanticTypeRecipe::Qualified { storage: inner, .. }
            | SemanticTypeRecipe::Opaque { hidden: inner, .. } => {
                self.bound_type(owner, inner, context, depth + 1, span)?
            }
            SemanticTypeRecipe::OwnedStore(element) => {
                // Keep the original element's complete nominal application
                // and rigid owner. A Store read changes no caller premise.
                let element = self.bound_type(owner, element, context, depth + 1, span)?;
                let index = self.store_elements.len();
                self.budget
                    .lineage_push(self.store_elements, element, span)?;
                Target::StoreReceiver(index)
            }
            SemanticTypeRecipe::Closure { definition, .. } => {
                declaration_index(self.definitions, *definition, self.budget, span)?
                    .map_or(Target::Unknown, Target::Definition)
            }
            SemanticTypeRecipe::Formal(reference) => {
                let Some(owner) = self.scope(owner, &reference.scope, span)? else {
                    return Ok(Target::Unknown);
                };
                let mut frame = context;
                let mut distance = depth;
                while let Some(index) = frame {
                    distance = distance.saturating_add(1);
                    self.budget.node(span, distance)?;
                    let application = self.applications[index];
                    if application.declaration == owner {
                        let actual = match application.arguments {
                            Arguments::Semantic(values) => {
                                match values.get(reference.ordinal as usize) {
                                    Some(SemanticActualRecipe::Type(ty)) => Some(ty),
                                    _ => None,
                                }
                            }
                            Arguments::Types(values) => values.get(reference.ordinal as usize),
                            Arguments::Written(values) => {
                                match values.get(reference.ordinal as usize) {
                                    Some(CoreGenericArgument::Type(ty)) => Some(ty),
                                    _ => None,
                                }
                            }
                        };
                        return if let Some(actual) = actual {
                            self.bound_type(
                                application.owner,
                                actual,
                                application.context,
                                distance + 1,
                                span,
                            )
                        } else {
                            Ok(Target::Unknown)
                        };
                    }
                    frame = application.context;
                }
                let definition = self.definitions[owner];
                let local_type = SemanticTypeRecipe::Formal(SemanticFormalRef {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal: reference.ordinal,
                });
                let mut found = None;
                if let Some(SemanticFormal::Type { requirements }) =
                    definition.parameters.get(reference.ordinal as usize)
                {
                    self.callable_requirements(
                        owner,
                        &local_type,
                        requirements,
                        &mut found,
                        depth + 1,
                        span,
                    )?;
                }
                self.callable_requirements(
                    owner,
                    &local_type,
                    &definition.requirements,
                    &mut found,
                    depth + 1,
                    span,
                )?;
                if let Some(kind) = found {
                    // Preserve the original role through aliases/captures, even
                    // when no call-region observer is installed. A CallKind
                    // summary alone loses whose complete consume bound it is.
                    let index = self.prototypes.len();
                    self.budget.lineage_push(self.prototypes, Prototype::Formal { owner, ordinal: reference.ordinal }, span)?;
                    Target::Prototype(index, kind)
                } else { Target::FormalReceiver { owner, ordinal: reference.ordinal } }
            }
            SemanticTypeRecipe::Array { .. } | SemanticTypeRecipe::Slice(_) => {
                let sequence = self.code_sequence(ty, depth + 1, span)?;
                if self.observations.is_some() && let Some((dimensions, kind)) = sequence {
                    let mut leaf = ty;
                    loop {
                        self.budget.node(span, depth + 1)?;
                        match leaf {
                            SemanticTypeRecipe::Array { element, .. } | SemanticTypeRecipe::Slice(element) => leaf = element,
                            SemanticTypeRecipe::Address { inner, .. } | SemanticTypeRecipe::Qualified { storage: inner, .. } => leaf = inner,
                            SemanticTypeRecipe::Code { signature, .. } => {
                                let index = self.prototypes.len();
                                self.budget.lineage_push(self.prototypes, Prototype::Code { owner, signature, context, frames: self.active_quantifiers }, span)?;
                                break Target::PrototypeSequence { index, dimensions, kind };
                            }
                            _ => break Target::LengthReceiver,
                        }
                    }
                } else { sequence.map_or(Target::LengthReceiver, |(dimensions, kind)| Target::CodeSequence { dimensions, kind }) }
            },
            SemanticTypeRecipe::Dyn { interface, .. } => {
                Target::TraitReceiver(interface.application.declaration)
            }
            SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            } => {
                let Some(declaration) =
                    declaration_index(self.definitions, *definition, self.budget, span)?
                else {
                    return Ok(Target::Unknown);
                };
                self.application(
                    declaration,
                    owner,
                    Arguments::Semantic(arguments),
                    context,
                    span,
                )?
                .map_or(Target::Unknown, Target::NominalReceiver)
            }
            SemanticTypeRecipe::ErrorUnion { result, .. } => {
                match self.bound_type(owner, result, context, depth + 1, span)? {
                    Target::TraitReceiver(id) => Target::FallibleTrait(id),
                    Target::NominalReceiver(id) => Target::FallibleNominal(id),
                    _ => Target::Unknown,
                }
            }
            SemanticTypeRecipe::SelfType { scope } => {
                let Some(mut owner) = self.scope(owner, scope, span)? else {
                    return Ok(Target::Unknown);
                };
                loop {
                    self.budget.node(span, depth + 1)?;
                    match &self.definitions[owner].body {
                        SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait {
                            declaration,
                        }) => break Target::TraitReceiver(*declaration),
                        SemanticDefinitionBody::Scope(
                            SemanticScopeRecipe::InherentImpl { self_type, .. }
                            | SemanticScopeRecipe::TraitImpl { self_type, .. }
                            | SemanticScopeRecipe::BuiltinImpl { self_type, .. },
                        ) => break self.bound_type(owner, self_type, context, depth + 1, span)?,
                        _ => {}
                    }
                    let Some(parent) = self.definitions[owner].parent else {
                        break Target::Unknown;
                    };
                    let Some(next) =
                        declaration_index(self.definitions, parent, self.budget, span)?
                    else {
                        break Target::Unknown;
                    };
                    owner = next;
                }
            }
            SemanticTypeRecipe::OriginalType { scope, .. } if self.observations.is_some() => {
                let Some(owner) = self.scope(owner, scope, span)? else { return Ok(Target::Unknown); };
                let mut found = Vec::new();
                self.premise_prototypes(owner, ty, &mut found, depth + 1, span)?;
                let kind = found.iter().map(|(signature, _)| signature.call_kind)
                    .reduce(|left, right| if left == CallKind::Suspend || right == CallKind::Suspend { CallKind::Suspend } else { CallKind::Direct });
                for (_, frames) in &mut found { self.budget.drop_lineage_vec(std::mem::take(frames), span)?; }
                self.budget.drop_lineage_vec(found, span)?;
                if let Some(kind) = kind {
                    let index = self.prototypes.len();
                    self.budget.lineage_push(self.prototypes, Prototype::Premises { owner, ty }, span)?;
                    Target::Prototype(index, kind)
                } else { Target::Unknown }
            }
            _ => Target::Unknown,
        })
    }
    fn code_sequence(
        &mut self,
        ty: &SemanticTypeRecipe,
        depth: usize,
        span: Span,
    ) -> R<Option<(u32, CallKind)>> {
        self.budget.node(span, depth)?;
        match ty {
            SemanticTypeRecipe::Code {
                kind: SemanticCodeKind::FunctionPointer,
                signature,
            } => Ok(Some((0, signature.call_kind))),
            SemanticTypeRecipe::Array { element, .. } | SemanticTypeRecipe::Slice(element) => {
                let Some((count, kind)) = self.code_sequence(element, depth + 1, span)? else {
                    return Ok(None);
                };
                Ok(Some((
                    count
                        .checked_add(1)
                        .ok_or_else(|| error(span, "effect code sequence depth overflows"))?,
                    kind,
                )))
            }
            SemanticTypeRecipe::Address { inner, .. }
            | SemanticTypeRecipe::Qualified { storage: inner, .. } => {
                self.code_sequence(inner, depth + 1, span)
            }
            _ => Ok(None),
        }
    }
    fn callable_requirements(
        &mut self,
        owner: usize,
        ty: &SemanticTypeRecipe,
        requirements: &[SemanticRequirementRecipe],
        found: &mut Option<CallKind>,
        depth: usize,
        span: Span,
    ) -> R<()> {
        for requirement in requirements {
            self.budget.node(span, depth)?;
            prepay_recipe_work(requirement, self.budget, span, depth)?;
            match requirement {
                SemanticRequirementRecipe::Callable {
                    ty: receiver,
                    signature,
                    ..
                } if receiver == ty => {
                    // Multiple roles remain separate obligations. Taking the
                    // larger ABI upper is conservative and proves no role.
                    *found = Some(
                        if *found == Some(CallKind::Suspend)
                            || signature.call_kind == CallKind::Suspend
                        {
                            CallKind::Suspend
                        } else {
                            CallKind::Direct
                        },
                    );
                }
                SemanticRequirementRecipe::ForAll { requirements, .. } => {
                    self.callable_requirements(owner, ty, requirements, found, depth + 1, span)?
                }
                _ => {}
            }
        }
        let _ = owner;
        Ok(())
    }
    fn names(&self) -> &'a [SemanticOriginalNameBinding] {
        match &self.definitions[self.owner].body {
            SemanticDefinitionBody::FunctionWithBody { names, .. } => names,
            _ => &[],
        }
    }
    fn path(
        &mut self,
        expression: &'a SemanticOriginalExpr,
        parts: &mut Vec<&'a str>,
        depth: usize,
    ) -> R<bool> {
        self.budget.node(expression.span, depth)?;
        match &expression.kind {
            CoreExprKind::Name(name) => {
                self.budget
                    .lineage_push(parts, name.as_str(), expression.span)?;
                Ok(true)
            }
            CoreExprKind::Member { object, member, .. } => {
                if !self.path(object, parts, depth + 1)? {
                    return Ok(false);
                }
                self.budget
                    .lineage_push(parts, member.as_str(), expression.span)?;
                Ok(true)
            }
            CoreExprKind::TypeApply { callee, .. }
            | CoreExprKind::GenericApply { callee, .. }
            | CoreExprKind::Group(callee) => self.path(callee, parts, depth + 1),
            _ => Ok(false),
        }
    }
    fn field_target(
        &mut self,
        receiver: Target,
        member: &str,
        depth: usize,
        span: Span,
    ) -> R<Target> {
        self.budget.node(span, depth)?;
        if let Target::Elements { index, tuple: true } = receiver {
            self.budget.identity_work(member.len().saturating_add(1), span)?;
            return Ok(member.parse::<usize>().ok().and_then(|ordinal| self.elements[index].get(ordinal)).copied().unwrap_or(Target::Unknown));
        }
        let Target::NominalReceiver(application) = receiver else {
            return Ok(Target::Unknown);
        };
        let index = self.applications[application].declaration;
        let SemanticDefinitionBody::Nominal {
            body: SemanticNominalBody::Product { fields },
            ..
        } = &self.definitions[index].body
        else {
            return Ok(Target::Unknown);
        };
        self.budget.identity_work(
            fields.len().saturating_mul(member.len().saturating_add(1)),
            span,
        )?;
        let mut selected = None;
        for field in fields {
            if field.name == member && selected.replace(field).is_some() {
                return Ok(Target::Unknown);
            }
        }
        let Some(field) = selected else {
            return Ok(Target::Unknown);
        };
        // The field is interpreted in the complete original application.
        // Its actual Type may itself be a caller's rigid formal: that formal's
        // original premises, rather than an environment image, supply the ABI.
        self.bound_type(index, &field.ty, Some(application), depth + 1, span)
    }
    fn applied_target(
        &mut self,
        callee: &'a SemanticOriginalExpr,
        arguments: Arguments<'a>,
        depth: usize,
        span: Span,
    ) -> R<Target> {
        self.budget.node(span, depth)?;
        match self.target(callee, depth + 1)? {
            Target::Definition(owner) => Ok(self
                .application(owner, self.owner, arguments, None, span)?
                .map_or(Target::Definition(owner), |application| {
                    Target::AppliedDefinition { owner, application }
                })),
            Target::AppliedDefinition { .. } => Ok(Target::Unknown),
            // Registered intrinsics retain their declared ABI; this pass
            // does not accept or solve their mixed application premises.
            target => Ok(target),
        }
    }
    fn target(&mut self, expression: &'a SemanticOriginalExpr, depth: usize) -> R<Target> {
        self.budget.node(expression.span, depth)?;
        match &expression.kind {
            CoreExprKind::Tuple(fields) | CoreExprKind::Array(fields) => {
                let mut values = Vec::new();
                for field in fields {
                    let target = self.target(field, depth + 1)?;
                    self.budget.lineage_push(&mut values, target, expression.span)?;
                }
                let index = self.elements.len();
                self.budget.lineage_push(self.elements, values, expression.span)?;
                return Ok(Target::Elements { index, tuple: matches!(expression.kind, CoreExprKind::Tuple(_)) });
            }
            CoreExprKind::Index { object, index: subscript } => {
                return Ok(match self.target(object, depth + 1)? {
                    Target::Elements { index, tuple: false } => {
                        let ordinal = match &subscript.kind { CoreExprKind::Integer(text) => text.parse::<usize>().ok(), _ => None };
                        if let Some(ordinal) = ordinal { self.elements[index].get(ordinal).copied().unwrap_or(Target::Unknown) }
                        else {
                            let mut result = Target::Unknown;
                            for ordinal in 0..self.elements[index].len() {
                                self.budget.node(expression.span, depth + 1)?;
                                let next = self.elements[index][ordinal];
                                if result == Target::Unknown { result = next; }
                                else if next != Target::Unknown && next != result {
                                    let index = self.alternatives.len();
                                    self.budget.lineage_push(self.alternatives, (result, next), expression.span)?;
                                    result = Target::Alternatives(index);
                                }
                            }
                            result
                        }
                    }
                    Target::PrototypeSequence { index, dimensions: 1, kind } => Target::Prototype(index, kind),
                    Target::PrototypeSequence { index, dimensions, kind } if dimensions > 1 => Target::PrototypeSequence { index, dimensions: dimensions - 1, kind },
                    Target::CodeSequence {
                        dimensions: 1,
                        kind,
                    } => Target::Kind(kind),
                    Target::CodeSequence { dimensions, kind } if dimensions > 1 => {
                        Target::CodeSequence {
                            dimensions: dimensions - 1,
                            kind,
                        }
                    }
                    _ => Target::Unknown,
                });
            }
            CoreExprKind::Slice { object, .. } => {
                return Ok(match self.target(object, depth + 1)? {
                    target @ (Target::CodeSequence { .. } | Target::PrototypeSequence { .. } | Target::LengthReceiver) => target,
                    _ => Target::Unknown,
                });
            }
            CoreExprKind::Unary {
                op: UnaryOp::Borrow | UnaryOp::BorrowMut | UnaryOp::Dereference,
                operand,
            } => {
                return self.target(operand, depth + 1);
            }
            CoreExprKind::Member { object, member, .. } => {
                let receiver = self.target(object, depth + 1)?;
                let target = self.field_target(receiver, member, depth + 1, expression.span)?;
                if target != Target::Unknown {
                    return Ok(target);
                }
            }
            CoreExprKind::ReceiverMember(member) => {
                self.budget
                    .identity_work(self.locals.len(), expression.span)?;
                let receiver = self
                    .locals
                    .iter()
                    .rev()
                    .find(|(name, _)| *name == "self")
                    .map_or(Target::Unknown, |(_, target)| *target);
                let target = self.field_target(receiver, member, depth + 1, expression.span)?;
                if target != Target::Unknown {
                    return Ok(target);
                }
            }
            CoreExprKind::Record { ty, .. } => {
                return self.callable_type(self.owner, ty, depth + 1, expression.span);
            }
            CoreExprKind::RecordConstructor {
                definition: Some(definition),
                ..
            } => {
                let Some(declaration) =
                    declaration_index(self.definitions, *definition, self.budget, expression.span)?
                else {
                    return Ok(Target::Unknown);
                };
                return Ok(self
                    .application(
                        declaration,
                        self.owner,
                        Arguments::Semantic(&[]),
                        None,
                        expression.span,
                    )?
                    .map_or(Target::Unknown, Target::NominalReceiver));
            }
            CoreExprKind::RecordUpdate { ty, base, .. } => {
                return if let Some(ty) = ty {
                    self.callable_type(self.owner, ty, depth + 1, expression.span)
                } else {
                    self.target(base, depth + 1)
                };
            }
            CoreExprKind::Try(value) => {
                return Ok(match self.target(value, depth + 1)? {
                    Target::FallibleTrait(id) => Target::TraitReceiver(id),
                    Target::FallibleNominal(id) => Target::NominalReceiver(id),
                    _ => Target::Unknown,
                });
            }
            CoreExprKind::Call { callee, arguments } => {
                let called = self.target(callee, depth + 1)?;
                if called == Target::Unknown && self.observations.is_some() {
                    // Method lookup uses the same lexical receiver navigation
                    // as the effect walker, while retaining every original
                    // selected member's full result prototype.
                    let previous_call = self.call.take();
                    let previous_collect = self.collect_targets;
                    let previous_targets = std::mem::take(&mut self.resolved_targets);
                    self.collect_targets = true;
                    self.invoke(callee, Some(arguments.len()), depth + 1)?;
                    self.collect_targets = previous_collect;
                    self.call = previous_call;
                    let mut candidates = std::mem::replace(&mut self.resolved_targets, previous_targets);
                    let mut result = Target::Unknown;
                    for candidate in &candidates {
                        let owner = match candidate { Target::Definition(owner) | Target::AppliedDefinition { owner, .. } => *owner, _ => continue };
                        if let Some(signature) = signature(self.definitions[owner]) {
                            let next = self.result_target(owner, signature, None, &[], depth + 1, expression.span)?;
                            result = if result == Target::Unknown { next } else if next == Target::Unknown || next == result { result } else {
                                let index = self.alternatives.len();
                                self.budget.lineage_push(self.alternatives, (result, next), expression.span)?;
                                Target::Alternatives(index)
                            };
                        }
                    }
                    self.budget.drop_lineage_vec(std::mem::take(&mut candidates), expression.span)?;
                    return Ok(result);
                }
                if called == Target::StoreBorrow && arguments.len() == 2 {
                    return Ok(match self.target(&arguments[0], depth + 1)? {
                        Target::StoreReceiver(index) => {
                            self.budget.identity_work(1, expression.span)?;
                            self.store_elements[index]
                        }
                        _ => Target::Unknown,
                    });
                }
                if called == Target::OwnedDynFactory {
                    return self.owned_factory_result(callee, depth + 1);
                }
                let (owner, context) = match called {
                    Target::Definition(owner) => (owner, None),
                    Target::AppliedDefinition { owner, application } => (owner, Some(application)),
                    Target::Prototype(index, _) => {
                        if let Prototype::Code { owner, signature, context, frames } = self.prototypes[index] {
                            let mut outer = Vec::new();
                            if let Some(index) = frames {
                                for count in &self.quantifiers[index] { self.budget.lineage_push(&mut outer, *count, expression.span)?; }
                            }
                            let result = self.result_target(owner, signature, context, &outer, depth + 1, expression.span)?;
                            self.budget.drop_lineage_vec(outer, expression.span)?;
                            return Ok(result);
                        }
                        let mut signatures = Vec::new();
                        let owner = match self.prototypes[index] {
                            Prototype::Formal { owner, ordinal } => { self.formal_prototypes(owner, ordinal, &mut signatures, depth + 1, expression.span)?; owner }
                            Prototype::Premises { owner, ty } => { self.premise_prototypes(owner, ty, &mut signatures, depth + 1, expression.span)?; owner }
                            Prototype::Code { .. } => unreachable!(),
                        };
                        let result = if let [(signature, frames)] = signatures.as_slice() {
                            self.result_target(owner, signature, None, frames, depth + 1, expression.span)?
                        } else { Target::Unknown };
                        for (_, frames) in &mut signatures { self.budget.drop_lineage_vec(std::mem::take(frames), expression.span)?; }
                        self.budget.drop_lineage_vec(signatures, expression.span)?;
                        return Ok(result);
                    }
                    _ => return Ok(Target::Unknown),
                };
                if let Some(signature) = signature(self.definitions[owner]) {
                    return if self.observations.is_some() {
                        self.result_target(owner, signature, context, &[], depth + 1, expression.span)
                    } else {
                        self.bound_type(owner, &signature.result, context, depth + 1, expression.span)
                    };
                }
                return Ok(Target::Unknown);
            }
            CoreExprKind::Ascribed { ty, .. } => {
                return self.callable_type(self.owner, ty, depth + 1, expression.span);
            }
            CoreExprKind::TypeApply {
                callee,
                type_arguments,
            } => {
                return self.applied_target(
                    callee,
                    Arguments::Types(type_arguments),
                    depth + 1,
                    expression.span,
                );
            }
            CoreExprKind::GenericApply { callee, arguments } => {
                return self.applied_target(
                    callee,
                    Arguments::Written(arguments),
                    depth + 1,
                    expression.span,
                );
            }
            CoreExprKind::Group(callee) => return self.target(callee, depth + 1),
            CoreExprKind::Name(name) => {
                self.budget
                    .identity_work(self.locals.len(), expression.span)?;
                if let Some((_, target)) =
                    self.locals.iter().rev().find(|(local, _)| *local == name)
                {
                    return Ok(*target);
                }
            }
            CoreExprKind::Closure { .. } => {
                let SemanticDefinitionBody::FunctionWithBody {
                    sites, site_nodes, ..
                } = &self.definitions[self.owner].body
                else {
                    return Ok(Target::Unknown);
                };
                self.budget.identity_work(
                    site_nodes.len().saturating_add(sites.len()),
                    expression.span,
                )?;
                for node in site_nodes {
                    if node.node != SemanticOriginalNode::Expression(expression.id) {
                        continue;
                    }
                    if let Some(SemanticSiteRecipe {
                        kind: SemanticSiteRecipeKind::Closure { definition, .. },
                        ..
                    }) = sites.iter().find(|site| site.id == node.site)
                    {
                        return Ok(declaration_index(
                            self.definitions,
                            *definition,
                            self.budget,
                            expression.span,
                        )?
                        .map_or(Target::Unknown, Target::Definition));
                    }
                }
                return Ok(Target::Unknown);
            }
            _ => {}
        }
        let mut path = Vec::new();
        let is_path = self.path(expression, &mut path, depth + 1)?;
        let local_root = if let Some(root) = path.first() {
            self.budget
                .identity_work(self.locals.len(), expression.span)?;
            self.locals.iter().rev().any(|(name, _)| name == root)
        } else {
            false
        };
        let target = if is_path && !local_root {
            self.budget.identity_work(
                self.names().len().saturating_mul(path.len() + 1),
                expression.span,
            )?;
            let binding = self.names().iter().find(|binding| {
                binding.namespace == SemanticOriginalNamespace::Value
                    && binding
                        .path
                        .iter()
                        .map(String::as_str)
                        .eq(path.iter().copied())
            });
            if let Some(SemanticOriginalNameBinding {
                value: SemanticOriginalNameValue::Definition(id),
                ..
            }) = binding
            {
                declaration_index(self.definitions, *id, self.budget, expression.span)?
                    .map_or(Target::Unknown, Target::Definition)
            } else if let [name] = path.as_slice() {
                if crate::primitive_name(name) {
                    if binding.is_none() && matches!(*name, "store_at" | "store_at_mut") {
                        Target::StoreBorrow
                    } else if binding.is_none() && *name == "owned_dyn" {
                        Target::OwnedDynFactory
                    } else {
                        Target::Kind(if matches!(*name, "task_yield" | "task_await") {
                            CallKind::Suspend
                        } else {
                            CallKind::Direct
                        })
                    }
                } else {
                    Target::Unknown
                }
            } else {
                Target::Unknown
            }
        } else {
            Target::Unknown
        };
        self.budget.drop_lineage_vec(path, expression.span)?;
        Ok(target)
    }
    fn owned_factory_result(
        &mut self,
        callee: &'a SemanticOriginalExpr,
        depth: usize,
    ) -> R<Target> {
        self.budget.node(callee.span, depth)?;
        let interface = match &callee.kind {
            CoreExprKind::Group(inner) => return self.owned_factory_result(inner, depth + 1),
            CoreExprKind::GenericApply { arguments, .. } => {
                self.budget.identity_work(arguments.len(), callee.span)?;
                match arguments.as_slice() {
                    [CoreGenericArgument::Type(SemanticTypeRecipe::Dyn { interface, .. })]
                    | [
                        CoreGenericArgument::Type(SemanticTypeRecipe::Dyn { interface, .. }),
                        CoreGenericArgument::Type(_),
                    ] => Some(interface),
                    _ => None,
                }
            }
            CoreExprKind::TypeApply { type_arguments, .. } => {
                self.budget
                    .identity_work(type_arguments.len(), callee.span)?;
                match type_arguments.as_slice() {
                    [SemanticTypeRecipe::Dyn { interface, .. }]
                    | [SemanticTypeRecipe::Dyn { interface, .. }, _] => Some(interface),
                    _ => None,
                }
            }
            _ => None,
        };
        Ok(interface.map_or(Target::Unknown, |interface| {
            Target::FallibleTrait(interface.application.declaration)
        }))
    }
    fn method_scope(
        &mut self,
        scope: SemanticDefinitionId,
        spelling: &str,
        depth: usize,
        span: Span,
    ) -> R<bool> {
        self.budget.node(span, depth)?;
        self.budget.identity_work(self.definitions.len(), span)?;
        let mut found = false;
        for (index, definition) in self.definitions.iter().enumerate() {
            if definition.parent == Some(scope) && definition.kind == SemanticDefinitionKind::Method
            {
                self.budget.identity_work(
                    definition
                        .identity
                        .declaration_path
                        .last()
                        .map_or(0, String::len)
                        .saturating_add(spelling.len()),
                    span,
                )?;
                if definition
                    .identity
                    .declaration_path
                    .last()
                    .is_some_and(|name| name == spelling)
                {
                    self.edge(Target::Definition(index), span)?;
                    found = true;
                }
            }
        }
        Ok(found)
    }
    fn trait_methods(
        &mut self,
        declaration: LifeDeclarationId,
        spelling: &str,
        depth: usize,
        span: Span,
        active: &mut Vec<SemanticDefinitionId>,
    ) -> R<bool> {
        self.budget.node(span, depth)?;
        let mut found = false;
        for definition in self.definitions {
            let SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait {
                declaration: source,
            }) = &definition.body
            else {
                continue;
            };
            if *source != declaration {
                continue;
            }
            self.budget.identity_work(active.len(), span)?;
            if active.contains(&definition.id) {
                self.vertex.suspend = true;
                return Ok(true);
            }
            self.budget.lineage_push(active, definition.id, span)?;
            found |= self.method_scope(definition.id, spelling, depth + 1, span)?;
            for requirement in &definition.requirements {
                self.budget.node(span, depth)?;
                if let SemanticRequirementRecipe::Trait {
                    ty: SemanticTypeRecipe::SelfType { .. },
                    application,
                } = requirement
                {
                    found |= self.trait_methods(
                        application.declaration,
                        spelling,
                        depth + 1,
                        span,
                        active,
                    )?;
                }
            }
            active.pop();
        }
        Ok(found)
    }
    fn receiver_method(
        &mut self,
        target: Target,
        spelling: &str,
        depth: usize,
        span: Span,
    ) -> R<bool> {
        self.budget.node(span, depth)?;
        match target {
            Target::LengthReceiver | Target::CodeSequence { .. } | Target::PrototypeSequence { .. } if spelling == "len" => {
                self.edge(Target::Kind(CallKind::Direct), span)?;
                Ok(true)
            }
            Target::TraitReceiver(declaration) => {
                let mut active = Vec::new();
                let found =
                    self.trait_methods(declaration, spelling, depth + 1, span, &mut active)?;
                self.budget.drop_lineage_vec(active, span)?;
                Ok(found)
            }
            Target::NominalReceiver(application) => {
                let index = self.applications[application].declaration;
                let owner = self.definitions[index].id;
                if let SemanticDefinitionBody::Nominal {
                    body: SemanticNominalBody::Product { fields },
                    ..
                } = &self.definitions[index].body
                {
                    self.budget.identity_work(fields.len(), span)?;
                    if let Some(field) = fields.iter().find(|field| field.name == spelling) {
                        let target =
                            self.bound_type(index, &field.ty, Some(application), depth + 1, span)?;
                        self.edge(target, span)?;
                        return Ok(true);
                    }
                }
                let mut found = false;
                for definition in self.definitions {
                    let self_type = match &definition.body {
                        SemanticDefinitionBody::Scope(
                            SemanticScopeRecipe::InherentImpl { self_type, .. }
                            | SemanticScopeRecipe::TraitImpl { self_type, .. }
                            | SemanticScopeRecipe::BuiltinImpl { self_type, .. },
                        ) => self_type,
                        _ => continue,
                    };
                    self.budget.node(span, depth)?;
                    if matches!(self_type,SemanticTypeRecipe::Nominal {definition,..} if *definition==owner)
                    {
                        found |= self.method_scope(definition.id, spelling, depth + 1, span)?;
                    } else if !matches!(self_type, SemanticTypeRecipe::Nominal { .. }) {
                        // An unresolved blanket/projection receiver is not
                        // discarded merely because its image is not known.
                        if self.method_scope(definition.id, spelling, depth + 1, span)? {
                            self.vertex.suspend = true;
                            found = true;
                        }
                    }
                }
                Ok(found)
            }
            Target::FormalReceiver { owner, ordinal } => {
                let definition = self.definitions[owner];
                let mut found = false;
                let empty = [];
                let own = if let Some(SemanticFormal::Type { requirements }) =
                    definition.parameters.get(ordinal as usize)
                {
                    requirements.as_slice()
                } else {
                    &empty
                };
                for requirements in [own, definition.requirements.as_slice()] {
                    for requirement in requirements {
                        self.budget.node(span, depth)?;
                        if let SemanticRequirementRecipe::Trait {
                            ty: SemanticTypeRecipe::Formal(reference),
                            application,
                        } = requirement
                            && reference.scope == SemanticScopeRef::Ancestor(0)
                            && reference.ordinal == ordinal
                        {
                            found |= self.receiver_method(
                                Target::TraitReceiver(application.declaration),
                                spelling,
                                depth + 1,
                                span,
                            )?;
                        }
                    }
                }
                Ok(found)
            }
            _ => Ok(false),
        }
    }
    // Construction executes its argument expressions, not an unknown method.
    // Resolve the original type namespace and actual enum variant; spelling
    // alone (including compiler-prelude spelling) never selects this case.
    fn enum_constructor(
        &mut self,
        callee: &'a SemanticOriginalExpr,
        arity: usize,
        depth: usize,
    ) -> R<bool> {
        let mut path = Vec::new();
        let is_path = self.path(callee, &mut path, depth + 1)?;
        let result = (|| {
            let Some((variant, owner)) = path.split_last() else {
                return Ok(false);
            };
            if !is_path || owner.is_empty() {
                return Ok(false);
            }
            self.budget.identity_work(self.locals.len(), callee.span)?;
            if self
                .locals
                .iter()
                .rev()
                .any(|(local, _)| *local == owner[0])
            {
                return Ok(false);
            }
            self.budget.identity_work(
                self.names().len().saturating_mul(owner.len() + 1),
                callee.span,
            )?;
            let mut selected = None;
            for binding in self.names() {
                if binding.namespace != SemanticOriginalNamespace::Type
                    || !binding
                        .path
                        .iter()
                        .map(String::as_str)
                        .eq(owner.iter().copied())
                {
                    continue;
                }
                let id = match &binding.value {
                    SemanticOriginalNameValue::Definition(id) => *id,
                    SemanticOriginalNameValue::Recipe(SemanticActualRecipe::Type(
                        SemanticTypeRecipe::Nominal { definition, .. },
                    )) => *definition,
                    _ => return Ok(false),
                };
                // A duplicate/ambiguous original namespace is not repaired by
                // choosing the first possible declaration.
                if selected.replace(id).is_some() {
                    return Ok(false);
                }
            }
            let Some(id) = selected else { return Ok(false) };
            let Some(index) = declaration_index(self.definitions, id, self.budget, callee.span)?
            else {
                return Ok(false);
            };
            let SemanticDefinitionBody::Nominal {
                body: SemanticNominalBody::Enum { variants },
                ..
            } = &self.definitions[index].body
            else {
                return Ok(false);
            };
            self.budget.identity_work(
                variants.len().saturating_mul(variant.len() + 1),
                callee.span,
            )?;
            let mut found = false;
            for row in variants {
                if row.name == *variant {
                    if found || row.fields.len() != arity {
                        return Ok(false);
                    }
                    if let (Some(call), Some(observations)) = (self.call, self.observations.as_deref_mut()) {
                        call_regions::variant(observations, self.definitions[self.owner].id, call,
                            self.definitions[index], &row.fields, self.budget, callee.span)?;
                    }
                    found = true;
                }
            }
            Ok(found)
        })();
        self.budget.drop_lineage_vec(path, callee.span)?;
        result
    }
    fn invoke(
        &mut self,
        callee: &'a SemanticOriginalExpr,
        arity: Option<usize>,
        depth: usize,
    ) -> R<()> {
        let target = self.target(callee, depth + 1)?;
        if target != Target::Unknown {
            return self.edge(target, callee.span);
        }
        if let Some(arity) = arity
            && self.enum_constructor(callee, arity, depth + 1)?
        {
            return self.edge(Target::Kind(CallKind::Direct), callee.span);
        }
        let receiver = match &callee.kind {
            CoreExprKind::Member { object, member, .. } => {
                Some((self.target(object, depth + 1)?, member.as_str()))
            }
            CoreExprKind::ReceiverMember(member) => {
                self.budget.identity_work(self.locals.len(), callee.span)?;
                self.locals
                    .iter()
                    .rev()
                    .find(|(name, _)| *name == "self")
                    .map(|(_, target)| (*target, member.as_str()))
            }
            CoreExprKind::GenericApply { callee, .. }
            | CoreExprKind::TypeApply { callee, .. }
            | CoreExprKind::Group(callee) => return self.invoke(callee, arity, depth + 1),
            _ => None,
        };
        if let Some((target, member)) = receiver
            && self.receiver_method(target, member, depth + 1, callee.span)?
        {
            return Ok(());
        }
        self.edge(Target::Unknown, callee.span)
    }
    fn pattern(&mut self, pattern: &'a SemanticOriginalPattern, depth: usize) -> R<()> {
        self.pattern_value(pattern, Target::Unknown, depth)
    }
    fn pattern_value(
        &mut self,
        pattern: &'a SemanticOriginalPattern,
        target: Target,
        depth: usize,
    ) -> R<()> {
        self.budget.node(pattern.span, depth)?;
        match &pattern.kind {
            CorePatternKind::Name(name) => self.bind(name, target, pattern.span)?,
            CorePatternKind::Tuple(fields) => {
                for (ordinal, field) in fields.iter().enumerate() {
                    let target = if let Target::Elements { index, tuple: true } = target { self.elements[index].get(ordinal).copied().unwrap_or(Target::Unknown) } else { Target::Unknown };
                    self.pattern_value(field, target, depth + 1)?;
                }
            }
            CorePatternKind::Variant { fields, .. }
            | CorePatternKind::Constructor { fields, .. } => {
                for field in fields {
                    self.pattern(field, depth + 1)?
                }
            }
            CorePatternKind::Record { fields, .. }
            | CorePatternKind::RecordConstructor { fields, .. } => {
                for (_, field) in fields {
                    self.pattern(field, depth + 1)?
                }
            }
            CorePatternKind::Reference { pattern, .. } => {
                self.pattern_value(pattern, target, depth + 1)?
            }
            CorePatternKind::ErrorArm { pattern, is_error } => {
                let target = if *is_error {
                    Target::Unknown
                } else {
                    match target {
                        Target::FallibleTrait(id) => Target::TraitReceiver(id),
                        Target::FallibleNominal(id) => Target::NominalReceiver(id),
                        _ => Target::Unknown,
                    }
                };
                self.pattern_value(pattern, target, depth + 1)?;
            }
            CorePatternKind::Wildcard
            | CorePatternKind::Integer(_)
            | CorePatternKind::Bool(_)
            | CorePatternKind::Float(_)
            | CorePatternKind::String(_)
            | CorePatternKind::Character(_) => {}
        }
        Ok(())
    }
    fn block(&mut self, block: &'a SemanticOriginalBlock, depth: usize) -> R<()> {
        self.budget.node(block.span, depth)?;
        let base = self.locals.len();
        for statement in &block.statements {
            self.statement(statement, depth + 1)?
        }
        self.locals.truncate(base);
        Ok(())
    }
    fn statement(&mut self, statement: &'a SemanticOriginalStatement, depth: usize) -> R<()> {
        self.budget.node(statement.span, depth)?;
        use CoreStatementKind as S;
        match &statement.kind {
            S::Binding {
                name,
                ty,
                initializer,
                ..
            } => {
                if let Some(value) = initializer {
                    self.expression(value, depth + 1)?
                }
                let target = if let Some(ty) = ty {
                    self.callable_type(self.owner, ty, depth + 1, statement.span)?
                } else if let Some(value) = initializer {
                    self.target(value, depth + 1)?
                } else {
                    Target::Unknown
                };
                self.bind(name, target, statement.span)?;
            }
            S::TupleBinding {
                pattern,
                initializer,
                ..
            } => {
                self.expression(initializer, depth + 1)?;
                let target = self.target(initializer, depth + 1)?;
                self.pattern_value(pattern, target, depth + 1)?;
            }
            S::Assign { target, value } => {
                self.expression(target, depth + 1)?;
                self.expression(value, depth + 1)?;
                if let CoreExprKind::Name(name) = &target.kind {
                    let target = self.target(value, depth + 1)?;
                    self.budget
                        .identity_work(self.locals.len(), statement.span)?;
                    if let Some((_, old)) = self
                        .locals
                        .iter_mut()
                        .rev()
                        .find(|(local, _)| *local == name)
                    {
                        if *old != target {
                            let index = self.alternatives.len();
                            self.budget.lineage_push(self.alternatives, (*old, target), statement.span)?;
                            *old = Target::Alternatives(index);
                        }
                    }
                }
            }
            S::CompoundAssign { target, value, .. } => {
                self.expression(target, depth + 1)?;
                self.expression(value, depth + 1)?;
            }
            S::Expression(value) | S::Yield(value) => self.expression(value, depth + 1)?,
            S::Return(value) => {
                if let Some(value) = value {
                    self.expression(value, depth + 1)?
                }
            }
            S::If {
                condition,
                then_body,
                else_body,
            } => {
                self.expression(condition, depth + 1)?;
                self.block(then_body, depth + 1)?;
                if let Some(body) = else_body {
                    self.block(body, depth + 1)?
                }
            }
            S::While { condition, body } => {
                self.expression(condition, depth + 1)?;
                self.block(body, depth + 1)?;
            }
            S::IfLet {
                pattern,
                value,
                then_body,
                else_body,
            } => {
                self.expression(value, depth + 1)?;
                let base = self.locals.len();
                let target = self.target(value, depth + 1)?;
                self.pattern_value(pattern, target, depth + 1)?;
                self.block(then_body, depth + 1)?;
                self.locals.truncate(base);
                if let Some(body) = else_body {
                    self.block(body, depth + 1)?
                }
            }
            S::WhileLet {
                pattern,
                value,
                body,
            }
            | S::ForEach {
                pattern,
                iterator: value,
                body,
            } => {
                self.expression(value, depth + 1)?;
                let base = self.locals.len();
                let target = if matches!(statement.kind, S::WhileLet { .. }) {
                    self.target(value, depth + 1)?
                } else {
                    Target::Unknown
                };
                self.pattern_value(pattern, target, depth + 1)?;
                self.block(body, depth + 1)?;
                self.locals.truncate(base);
                // The source spelling `for pattern in start..end` has the
                // same builtin integer iteration as ForRange. Ordinary
                // iterator protocols retain their fixed Suspend upper.
                let mut iterator = value;
                while let CoreExprKind::Group(inner) = &iterator.kind {
                    self.budget.node(iterator.span, depth + 1)?;
                    iterator = inner;
                }
                if matches!(statement.kind, S::ForEach { .. })
                    && !matches!(iterator.kind, CoreExprKind::Range { .. })
                {
                    self.vertex.suspend = true;
                }
            }
            S::ForRange {
                name,
                start,
                end,
                body,
            } => {
                self.expression(start, depth + 1)?;
                self.expression(end, depth + 1)?;
                let base = self.locals.len();
                self.bind(name, Target::Unknown, statement.span)?;
                self.block(body, depth + 1)?;
                self.locals.truncate(base);
            }
            S::Match { value, arms } => {
                self.expression(value, depth + 1)?;
                let target = self.target(value, depth + 1)?;
                for arm in arms {
                    let base = self.locals.len();
                    self.pattern_value(&arm.pattern, target, depth + 1)?;
                    self.block(&arm.body, depth + 1)?;
                    self.locals.truncate(base);
                }
            }
            S::Block(body) | S::Defer(body) | S::Unsafe(body) | S::Loop(body) => {
                self.block(body, depth + 1)?
            }
            S::With { name, value, body } => {
                self.expression(value, depth + 1)?;
                let base = self.locals.len();
                let target = self.target(value, depth + 1)?;
                self.bind(name, target, statement.span)?;
                self.block(body, depth + 1)?;
                self.locals.truncate(base);
            }
            S::CompileTime(_) => {}
            S::Break | S::Continue => {}
            S::Unsupported(_) => self.vertex.suspend = true,
        }
        Ok(())
    }
    fn expression(&mut self, expression: &'a SemanticOriginalExpr, depth: usize) -> R<()> {
        self.budget.node(expression.span, depth)?;
        use CoreExprKind as E;
        match &expression.kind {
            E::Call { callee, arguments }
            | E::CallDynSlot {
                callee, arguments, ..
            } => {
                self.expression(callee, depth + 1)?;
                for argument in arguments {
                    self.expression(argument, depth + 1)?
                }
                self.budget
                    .identity_work(arguments.len(), expression.span)?;
                let arity = (!arguments
                    .iter()
                    .any(|argument| matches!(argument.kind, E::Spread(_))))
                .then_some(arguments.len());
                let previous = self.call;
                self.call = Some(expression.id);
                let previous_collect = self.collect_targets;
                let previous_targets = std::mem::take(&mut self.resolved_targets);
                self.collect_targets = true;
                self.invoke(callee, arity, depth + 1)?;
                self.collect_targets = previous_collect;
                let targets = std::mem::replace(&mut self.resolved_targets, previous_targets);
                let mut head = callee.as_ref();
                while let E::Group(inner) | E::TypeApply { callee: inner, .. } | E::GenericApply { callee: inner, .. } = &head.kind { head = inner; }
                let receiver = matches!(head.kind, E::Member { .. } | E::ReceiverMember(_));
                for target in &targets {
                    self.forwarded_inputs(*target, arguments, receiver, expression.span, depth + 1)?;
                }
                self.budget.drop_lineage_vec(targets, expression.span)?;
                // A worker invocation is an actual call of the original worker
                // prototype, at this builtin call's real occurrence.
                if let Some(worker) = arguments.first() {
                    let mut builtin = callee.as_ref();
                    while let E::Group(inner) | E::TypeApply { callee: inner, .. } | E::GenericApply { callee: inner, .. } = &builtin.kind { builtin = inner; }
                    if matches!(&builtin.kind, E::Name(name) if matches!(name.as_str(), "thread_scope" | "task_create" | "task_create_owned" | "task_await"))
                        && matches!(self.target(callee, depth + 1)?, Target::Kind(_)) {
                        let target = self.target(worker, depth + 1)?;
                        self.observe_target(expression.id, target, expression.span)?;
                        self.forwarded_inputs(target, &arguments[1..], false, expression.span, depth + 1)?;
                    }
                }
                self.call = previous;
            }
            E::Pipeline { input, target } => {
                self.expression(input, depth + 1)?;
                self.expression(target, depth + 1)?;
                self.invoke(target, Some(1), depth + 1)?;
                let called = self.target(target, depth + 1)?;
                self.forwarded_inputs(called, std::slice::from_ref(input.as_ref()), false, expression.span, depth + 1)?;
            }
            E::MakeDynBorrow { source: value, .. }
            | E::MakeDynOwned { source: value, .. }
            | E::Ascribed { value, .. }
            | E::NumericConvert { value, .. }
            | E::IntegerConvert { value, .. }
            | E::Unary { operand: value, .. }
            | E::Try(value)
            | E::Spread(value)
            | E::Group(value) => self.expression(value, depth + 1)?,
            E::Binary { left, right, .. }
            | E::Range {
                start: left,
                end: right,
            }
            | E::Index {
                object: left,
                index: right,
            } => {
                self.expression(left, depth + 1)?;
                self.expression(right, depth + 1)?;
            }
            E::TypeApply { callee, .. } | E::GenericApply { callee, .. } => {
                self.expression(callee, depth + 1)?
            }
            E::Tuple(values) | E::Array(values) => {
                for value in values {
                    self.expression(value, depth + 1)?
                }
            }
            E::Slice { object, start, end } => {
                self.expression(object, depth + 1)?;
                for value in [start, end].into_iter().flatten() {
                    self.expression(value, depth + 1)?
                }
            }
            E::Member { object, .. } => self.expression(object, depth + 1)?,
            E::Record { fields, .. } | E::RecordConstructor { fields, .. } => {
                for (_, value) in fields {
                    self.expression(value, depth + 1)?
                }
                let constructed = match &expression.kind {
                    E::RecordConstructor { definition, .. } => *definition,
                    E::Record { ty, .. } => {
                        let mut ty = ty;
                        while let SemanticTypeRecipe::Managed(inner) | SemanticTypeRecipe::Desc { inner, .. } = ty {
                            self.budget.node(expression.span, depth + 1)?;
                            ty = inner;
                        }
                        match ty {
                            SemanticTypeRecipe::Nominal { definition, .. } => Some(*definition),
                            _ => None,
                        }
                    }
                    _ => None,
                };
                if let Some(definition) = constructed
                    && self.observations.is_some()
                {
                    let declaration = declaration_index(self.definitions, definition, self.budget, expression.span)?
                        .ok_or_else(|| error(expression.span, "Construct prototype declaration is absent"))?;
                    call_regions::constructor(
                        self.observations.as_deref_mut().unwrap(),
                        self.definitions[self.owner].id, expression.id,
                        self.definitions[declaration], self.budget, expression.span,
                    )?;
                }
            }
            E::RecordUpdate { base, fields, .. } => {
                self.expression(base, depth + 1)?;
                for (_, value) in fields {
                    self.expression(value, depth + 1)?
                }
            }
            E::Sequence { bindings, value } => {
                let base = self.locals.len();
                for (name, value) in bindings {
                    self.expression(value, depth + 1)?;
                    let target = self.target(value, depth + 1)?;
                    self.bind(name, target, expression.span)?
                }
                self.expression(value, depth + 1)?;
                self.locals.truncate(base);
            }
            E::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expression(condition, depth + 1)?;
                self.expression(then_value, depth + 1)?;
                self.expression(else_value, depth + 1)?
            }
            // Creation does not execute the child's body. Each authored child
            // definition is a separate graph vertex with its own full body.
            E::Closure { .. } => {
                let Target::Definition(child) = self.target(expression, depth + 1)? else {
                    return Err(error(
                        expression.span,
                        "effect closure has no exact original definition",
                    ));
                };
                let definition = self.definitions[child];
                if definition.parent != Some(self.definitions[self.owner].id) {
                    return Err(error(
                        expression.span,
                        "effect closure changes its original lexical creator",
                    ));
                }
                let mut captured = Vec::new();
                for slot in &definition.environment {
                    self.budget.node(expression.span, depth + 1)?;
                    let SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType {
                        source: SemanticOriginalTypeSource::NameAt { at, name },
                        ..
                    }) = &slot.source
                    else {
                        continue;
                    };
                    if *at != SemanticOriginalNode::Expression(expression.id) {
                        return Err(error(
                            expression.span,
                            "effect capture selects another original creator point",
                        ));
                    }
                    self.budget
                        .identity_work(self.locals.len(), expression.span)?;
                    let target = self
                        .locals
                        .iter()
                        .rev()
                        .find(|(local, _)| *local == name)
                        .map_or(Target::Unknown, |(_, target)| *target);
                    self.budget.lineage_push(
                        &mut captured,
                        (name.as_str(), target),
                        expression.span,
                    )?;
                }
                self.budget
                    .identity_work(self.captures.len(), expression.span)?;
                if self.captures.iter().any(|(owner, _)| *owner == child) {
                    return Err(error(
                        expression.span,
                        "effect closure repeats its original creator point",
                    ));
                }
                self.budget
                    .lineage_push(self.captures, (child, captured), expression.span)?;
            }
            E::CompileTime(_)
            | E::CompileTimeBlock(_)
            | E::SyntaxQuote { .. }
            | E::SyntaxIsland { .. }
            | E::Splice(_) => {}
            E::Literal(_)
            | E::Integer(_)
            | E::Float(_)
            | E::Bool(_)
            | E::String(_)
            | E::Unit
            | E::Name(_)
            | E::ReceiverMember(_)
            | E::Character(_) => {}
            E::Unsupported(_) => self.vertex.suspend = true,
        }
        Ok(())
    }
}

/// Run the same complete original lexical walker with a signature observer.
/// It retains original prototypes before the ABI proposal discards detail.
/// Unknown effect summaries never reject Source or create guessed roles.
pub(super) fn observe_call_prototypes(
    modules: &[(ModuleId, SemanticModule)], budget: &mut Budget<'_>,
) -> R<Vec<call_regions::Observation>> {
    let span = Span::default();
    let mut definitions = Vec::new();
    for (_, module) in modules { for definition in &module.definitions { budget.lineage_push(&mut definitions, definition, span)?; } }
    let mut order = Vec::new();
    for (index, definition) in definitions.iter().enumerate() {
        let mut depth = 0usize;
        let mut parent = definition.parent;
        while let Some(id) = parent {
            depth += 1;
            budget.node(span, depth)?;
            let owner = declaration_index(&definitions, id, budget, span)?.ok_or_else(|| error(span, "Call prototype parent is absent"))?;
            parent = definitions[owner].parent;
        }
        budget.lineage_push(&mut order, (depth, index), span)?;
    }
    budget.identity_work(order.len().saturating_mul(order.len().saturating_add(1).ilog2() as usize + 1), span)?;
    order.sort_unstable();
    let mut captures: Vec<(usize, Vec<(&str, Target)>)> = Vec::new();
    let mut applications = Vec::new();
    let mut store_elements = Vec::new();
    let mut prototypes = Vec::new();
    let mut alternatives = Vec::new();
    let mut quantifiers = Vec::new();
    let mut elements = Vec::new();
    let mut observations = Vec::new();
    for &(_, index) in &order {
        let definition = definitions[index];
        let SemanticDefinitionBody::FunctionWithBody { signature, parameters, body, .. } = &definition.body else { continue; };
        let mut locals = Vec::new();
        budget.identity_work(captures.len(), span)?;
        if let Some((_, environment)) = captures.iter().find(|(owner, _)| *owner == index) {
            for entry in environment { budget.lineage_push(&mut locals, *entry, span)?; }
        }
        let mut vertex = Vertex { explicit: None, suspend: false, edges: Vec::new(), consumes: Vec::new(), forwards: Vec::new() };
        let mut scan = Scan { definitions: &definitions, owner: index, locals,
            captures: &mut captures, applications: &mut applications, store_elements: &mut store_elements,
            vertex: &mut vertex, budget, prototypes: &mut prototypes, alternatives: &mut alternatives, quantifiers: &mut quantifiers, elements: &mut elements, active_quantifiers: None, observations: Some(&mut observations), call: None, collect_targets: false, resolved_targets: Vec::new() };
        for (ordinal, parameter) in parameters.iter().enumerate() {
            let target = if let Some(parameter_type) = signature.parameters.get(ordinal) {
                scan.callable_type(index, &parameter_type.ty, 0, parameter.span)?
            } else { Target::Unknown };
            scan.bind(&parameter.name, target, parameter.span)?;
        }
        if let SemanticOriginalFunctionBody::Defined(body) = body { scan.block(body, 0)?; }
        let locals = std::mem::take(&mut scan.locals);
        scan.budget.drop_lineage_vec(locals, span)?;
        let resolved = std::mem::take(&mut scan.resolved_targets);
        scan.budget.drop_lineage_vec(resolved, span)?;
        budget.drop_lineage_vec(vertex.edges, span)?;
        budget.drop_lineage_vec(vertex.consumes, span)?;
        budget.drop_lineage_vec(vertex.forwards, span)?;
    }
    for (_, environment) in &mut captures { budget.drop_lineage_vec(std::mem::take(environment), span)?; }
    budget.drop_lineage_vec(captures, span)?;
    budget.drop_lineage_vec(applications, span)?;
    budget.drop_lineage_vec(store_elements, span)?;
    budget.drop_lineage_vec(prototypes, span)?;
    budget.drop_lineage_vec(alternatives, span)?;
    for frames in &mut quantifiers { budget.drop_lineage_vec(std::mem::take(frames), span)?; }
    budget.drop_lineage_vec(quantifiers, span)?;
    for values in &mut elements { budget.drop_lineage_vec(std::mem::take(values), span)?; }
    budget.drop_lineage_vec(elements, span)?;
    budget.drop_lineage_vec(order, span)?;
    budget.drop_lineage_vec(definitions, span)?;
    Ok(observations)
}

pub(super) fn propose(
    modules: &mut [(ModuleId, SemanticModule)],
    originals: &SourceDeclarationInventory<'_>,
    budget: &mut Budget<'_>,
) -> R<()> {
    let mut definitions = Vec::new();
    for (_, module) in modules.iter() {
        for definition in &module.definitions {
            budget.lineage_push(&mut definitions, definition, Span::default())?
        }
    }
    let mut vertices = Vec::new();
    let mut order = Vec::new();
    for (index, definition) in definitions.iter().enumerate() {
        let mut explicit = None;
        budget.identity_work(originals.declarations.len(), Span::default())?;
        if let Some(row) = originals.declarations.iter().find(|row| {
            row.module == definition.id.module && row.declaration.0 == definition.id.index
        }) && let Some(function) = row.function()
        {
            let mut remaining = budget.remaining_limits();
            explicit = crate::tasks::selection(function, &mut remaining.max_analysis_steps)?.0;
            budget.adopt_remaining(&remaining);
        }
        let vertex = Vertex {
            explicit,
            suspend: explicit == Some(CallKind::Suspend),
            edges: Vec::new(),
            consumes: Vec::new(),
            forwards: Vec::new(),
        };
        budget.lineage_push(&mut vertices, vertex, Span::default())?;
        let mut parent = definition.parent;
        let mut depth = 0usize;
        while let Some(id) = parent {
            depth += 1;
            budget.node(Span::default(), depth)?;
            let Some(owner) = declaration_index(&definitions, id, budget, Span::default())? else {
                return Err(error(Span::default(), "effect lexical parent is absent"));
            };
            parent = definitions[owner].parent;
        }
        budget.lineage_push(&mut order, (depth, index), Span::default())?;
    }
    budget.identity_work(
        order
            .len()
            .saturating_mul(order.len().saturating_add(1).ilog2() as usize + 1),
        Span::default(),
    )?;
    order.sort_unstable();
    let mut captures: Vec<(usize, Vec<(&str, Target)>)> = Vec::new();
    let mut applications = Vec::new();
    let mut store_elements = Vec::new();
    let mut prototypes = Vec::new();
    let mut alternatives = Vec::new();
    let mut quantifiers = Vec::new();
    let mut elements = Vec::new();
    for &(_, index) in &order {
        let definition = definitions[index];
        if let SemanticDefinitionBody::FunctionWithBody {
            signature,
            parameters,
            body,
            ..
        } = &definition.body
        {
            let mut locals = Vec::new();
            budget.identity_work(captures.len(), Span::default())?;
            if let Some((_, environment)) = captures.iter().find(|(owner, _)| *owner == index) {
                for entry in environment {
                    budget.lineage_push(&mut locals, *entry, Span::default())?;
                }
            }
            let mut scan = Scan {
                definitions: &definitions,
                owner: index,
                locals,
                captures: &mut captures,
                applications: &mut applications,
                store_elements: &mut store_elements,
                vertex: &mut vertices[index],
                budget,
                prototypes: &mut prototypes,
                alternatives: &mut alternatives,
                quantifiers: &mut quantifiers,
                elements: &mut elements,
                active_quantifiers: None,
                observations: None,
                call: None,
                collect_targets: false,
                resolved_targets: Vec::new(),
            };
            for (ordinal, parameter) in parameters.iter().enumerate() {
                let target = signature
                    .parameters
                    .get(ordinal)
                    .map(|parameter| &parameter.ty);
                let target = if let Some(ty) = target {
                    scan.callable_type(index, ty, 0, parameter.span)?
                } else {
                    Target::Unknown
                };
                scan.bind(&parameter.name, target, parameter.span)?
            }
            if let SemanticOriginalFunctionBody::Defined(body) = body {
                scan.block(body, 0)?
            }
            let locals = std::mem::take(&mut scan.locals);
            scan.budget.drop_lineage_vec(locals, Span::default())?;
            let resolved = std::mem::take(&mut scan.resolved_targets);
            scan.budget.drop_lineage_vec(resolved, Span::default())?;
        }
    }
    budget.drop_lineage_vec(order, Span::default())?;
    for (_, environment) in &mut captures {
        budget.drop_lineage_vec(std::mem::take(environment), Span::default())?;
    }
    budget.drop_lineage_vec(captures, Span::default())?;
    budget.drop_lineage_vec(applications, Span::default())?;
    budget.drop_lineage_vec(store_elements, Span::default())?;
    budget.drop_lineage_vec(prototypes, Span::default())?;
    budget.drop_lineage_vec(alternatives, Span::default())?;
    for frames in &mut quantifiers { budget.drop_lineage_vec(std::mem::take(frames), Span::default())?; }
    budget.drop_lineage_vec(quantifiers, Span::default())?;
    for values in &mut elements { budget.drop_lineage_vec(std::mem::take(values), Span::default())?; }
    budget.drop_lineage_vec(elements, Span::default())?;
    // Monotone propagation reaches a fixed point after at most one promotion
    // per vertex. Closed recursive SCCs without a Suspend edge remain Direct.
    loop {
        let mut changed = false;
        for index in 0..vertices.len() {
            budget.node(Span::default(), 0)?;
            if vertices[index].explicit.is_some() || vertices[index].suspend {
                continue;
            }
            budget.identity_work(vertices[index].edges.len(), Span::default())?;
            if vertices[index].edges.iter().any(|target| {
                vertices[*target].explicit == Some(CallKind::Suspend)
                    || (vertices[*target].explicit.is_none() && vertices[*target].suspend)
            }) {
                vertices[index].suspend = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // Complete operation contexts, not actual closure instances, determine
    // the candidate upper of an original input role. A Direct use constrains
    // the role to Direct even when another use occurs in a Task continuation.
    let mut roles = Vec::new();
    for vertex in &vertices {
        for role in vertex.consumes.iter().copied().chain(vertex.forwards.iter().map(|(source, _)| *source)) {
            budget.identity_work(roles.len(), Span::default())?;
            if !roles.contains(&role) { budget.lineage_push(&mut roles, role, Span::default())?; }
        }
    }
    let mut eligible = Vec::new();
    let mut witnessed = Vec::new();
    for role in &roles {
        let mut permits_suspend = true;
        let mut consume = false;
        for vertex in &vertices {
            budget.identity_work(vertex.consumes.len(), Span::default())?;
            if vertex.consumes.contains(role) {
                consume = true;
                permits_suspend &= vertex.explicit.unwrap_or(if vertex.suspend { CallKind::Suspend } else { CallKind::Direct }) == CallKind::Suspend;
            }
        }
        budget.lineage_push(&mut eligible, permits_suspend, Span::default())?;
        budget.lineage_push(&mut witnessed, consume, Span::default())?;
    }
    // Forwarding cycles need a real consuming endpoint. Propagate both that
    // witness and every Direct restriction; equality of actual images never
    // joins roles. Each bit changes at most once, including recursive SCCs.
    loop {
        let mut progress = false;
        for vertex in &vertices {
            for (source, destination) in &vertex.forwards {
                budget.identity_work(roles.len().saturating_mul(2), Span::default())?;
                let source = roles.iter().position(|role| role == source).expect("enumerated forwarding source");
                let destination = roles.iter().position(|role| role == destination);
                if eligible[source] && !destination.is_some_and(|index| eligible[index]) {
                    eligible[source] = false;
                    progress = true;
                }
                if !witnessed[source] && destination.is_some_and(|index| witnessed[index]) {
                    witnessed[source] = true;
                    progress = true;
                }
            }
        }
        if !progress { break; }
    }
    let mut promoted = Vec::new();
    for (index, role) in roles.iter().enumerate() {
        budget.node(Span::default(), 0)?;
        if eligible[index] && witnessed[index] {
            budget.lineage_push(&mut promoted, *role, Span::default())?;
        }
    }
    budget.drop_lineage_vec(eligible, Span::default())?;
    budget.drop_lineage_vec(witnessed, Span::default())?;
    budget.drop_lineage_vec(roles, Span::default())?;
    budget.drop_lineage_vec(definitions, Span::default())?;
    let mut index = 0;
    for (_, module) in modules.iter_mut() {
        for definition in &mut module.definitions {
            let vertex = &vertices[index];
            budget.identity_work(promoted.len(), Span::default())?;
            for role in &promoted {
                if role.owner != index { continue; }
                if let Some(SemanticFormal::Type { requirements }) = definition.parameters.get_mut(role.ordinal as usize) {
                    promote_once(requirements, role.ordinal, budget, 0)?;
                }
                promote_once(&mut definition.requirements, role.ordinal, budget, 0)?;
            }
            index += 1;
            let kind = vertex.explicit.unwrap_or(if vertex.suspend {
                CallKind::Suspend
            } else {
                CallKind::Direct
            });
            if let SemanticDefinitionBody::FunctionWithBody { signature, .. }
            | SemanticDefinitionBody::Function { signature, .. } = &mut definition.body
            {
                signature.call_kind = kind;
            }
        }
    }
    budget.drop_lineage_vec(promoted, Span::default())?;
    for vertex in &mut vertices {
        budget.drop_lineage_vec(std::mem::take(&mut vertex.edges), Span::default())?;
        budget.drop_lineage_vec(std::mem::take(&mut vertex.consumes), Span::default())?;
        budget.drop_lineage_vec(std::mem::take(&mut vertex.forwards), Span::default())?;
    }
    budget.drop_lineage_vec(vertices, Span::default())?;
    // Every actual consume use of a promoted role already belongs to a
    // Suspend vertex. Thus publication cannot introduce a new function effect
    // edge, and never bootstraps a Suspend premise from its own invocation.
    cleanup_candidates::propose(modules, budget)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_context::{self, DeclarationPlacement, SourceIdentityContext};
    use std::collections::BTreeMap;
    use zil_base::{FileId, SourceUnit};

    #[test]
    fn original_effect_candidates_follow_calls_and_preserve_explicit_bounds() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "original-effects.zil",
            "fn leaf(){task_yield();}fn relay(){leaf();}fn a(){b();}fn b(){a();}fn declared_direct() contract{effects(io);}{task_yield();}fn declared_suspend() contract{effects(suspend);}{}fn creator(){let worker=()=>task_yield();}fn invoked(){let worker=()=>task_yield();worker();}fn through<F:Fn()->void>(worker:F){worker();}fn deferred(){defer{task_yield();}}fn unknown(){unresolved();}fn length(values:&mut[u8])->i64{return values.len()}",
        );
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let limits = Limits::default();
        let (origins, _) = source_context::capture_original_required(
            std::slice::from_ref(&source),
            &[&parsed],
            SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &BTreeMap::new(),
                declarations: DeclarationPlacement::Original,
            },
            &limits,
        )
        .unwrap();
        let mut remaining = limits.clone();
        let (_, program) =
            super::super::program::prepare(&[&parsed], &origins, &[Vec::new()], &mut remaining)
                .unwrap();
        let definitions = &program.modules[0].1.definitions;
        let kind = |index: usize| signature(&definitions[index]).unwrap().call_kind;
        assert_eq!(kind(0), CallKind::Suspend);
        assert_eq!(kind(1), CallKind::Suspend);
        assert_eq!(kind(2), CallKind::Direct);
        assert_eq!(kind(3), CallKind::Direct);
        assert_eq!(kind(4), CallKind::Direct); // Invalid body is left for consumers.
        assert_eq!(kind(5), CallKind::Suspend); // Empty body does not narrow a declaration.
        assert_eq!(kind(6), CallKind::Direct); // Making a child does not execute it.
        assert_eq!(kind(7), CallKind::Suspend);
        assert_eq!(kind(8), CallKind::Direct); // Original callable premise, not actual F.
        assert_eq!(kind(9), CallKind::Suspend);
        assert_eq!(kind(10), CallKind::Suspend); // Unresolved is not silently Direct.
        assert_eq!(kind(11), CallKind::Direct); // Registered slice method, from original annotation.
    }
    #[test]
    fn method_candidates_use_original_receiver_bindings_not_same_spelling() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "original-method-effects.zil",
            "trait Read{fn read(self:&Self)->i32;}struct Provider{n:i32}impl Read for Provider{fn read(self:&Self)->i32{return self.n}}fn declared_receiver(value:&Provider){value.read();}fn unknown_receiver<F>(value:F){value.read();}fn bounded_receiver<F:Read>(value:F){value.read();}",
        );
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let limits = Limits::default();
        let (origins, _) = source_context::capture_original_required(
            std::slice::from_ref(&source),
            &[&parsed],
            SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &BTreeMap::new(),
                declarations: DeclarationPlacement::Original,
            },
            &limits,
        )
        .unwrap();
        let mut remaining = limits.clone();
        let (_, program) =
            super::super::program::prepare(&[&parsed], &origins, &[Vec::new()], &mut remaining)
                .unwrap();
        let kind = |node| {
            signature(
                program.modules[0]
                    .1
                    .definitions
                    .iter()
                    .find(|definition| {
                        definition.id
                            == SemanticDefinitionId {
                                module: source.module_id,
                                index: node,
                            }
                    })
                    .unwrap(),
            )
            .unwrap()
            .call_kind
        };
        assert_eq!(kind(3), CallKind::Direct);
        assert_eq!(kind(4), CallKind::Suspend);
        assert_eq!(kind(5), CallKind::Direct);
    }
    #[test]
    fn enum_constructor_candidates_use_original_type_bindings_and_preserve_argument_effects() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "enum-constructor-effects.zil",
            "enum Choice<T>{Empty,Value(T)}fn empty(){Choice::<i32>.Empty();}fn value(){Choice::<i32>.Value(42);}fn argument(){Choice::<void>.Value(task_yield());}fn wrong_arity(){Choice::<i32>.Value();}fn shadow< F >(Choice:F){Choice.Value(42);}fn unknown(){Other.Value(42);}",
        );
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let limits = Limits::default();
        let (origins, _) = source_context::capture_original_required(
            std::slice::from_ref(&source),
            &[&parsed],
            SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &BTreeMap::new(),
                declarations: DeclarationPlacement::Original,
            },
            &limits,
        )
        .unwrap();
        let (_, program) = super::super::program::prepare(
            &[&parsed],
            &origins,
            &[Vec::new()],
            &mut limits.clone(),
        )
        .unwrap();
        let kind = |index| {
            signature(
                program.modules[0]
                    .1
                    .definitions
                    .iter()
                    .find(|row| row.id.index == index)
                    .unwrap(),
            )
            .unwrap()
            .call_kind
        };
        assert_eq!(kind(1), CallKind::Direct);
        assert_eq!(kind(2), CallKind::Direct);
        assert_eq!(kind(3), CallKind::Suspend);
        assert_eq!(kind(4), CallKind::Suspend);
        assert_eq!(kind(5), CallKind::Suspend);
        assert_eq!(kind(6), CallKind::Suspend);
    }
    fn original_module(text: &str) -> SemanticModule {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "consume-upper.zil", text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let limits = Limits::default();
        let (origins, _) = source_context::capture_original_required(
            std::slice::from_ref(&source), &[&parsed],
            SourceIdentityContext::ClosedSnapshot {
                root: source.module_id, aliases: &BTreeMap::new(),
                declarations: DeclarationPlacement::Original,
            }, &limits,
        ).unwrap();
        let mut remaining = limits;
        let (_, mut program) = super::super::program::prepare(
            &[&parsed], &origins, &[Vec::new()], &mut remaining,
        ).unwrap();
        program.modules.remove(0).1
    }
    fn function<'a>(module: &'a SemanticModule, name: &str) -> &'a SemanticDefinition {
        module.definitions.iter().find(|definition|
            definition.identity.declaration_path.last().is_some_and(|part| part == name)
                && signature(definition).is_some()).unwrap()
    }
    fn callable_uppers(requirements: &[SemanticRequirementRecipe], output: &mut Vec<CallKind>) {
        for requirement in requirements {
            match requirement {
                SemanticRequirementRecipe::Callable { signature, .. } => output.push(signature.call_kind),
                SemanticRequirementRecipe::ForAll { requirements, .. } => callable_uppers(requirements, output),
                _ => {}
            }
        }
    }
    fn assert_input_upper(definition: &SemanticDefinition, ordinal: usize, expected: CallKind) {
        let SemanticFormal::Type { requirements } = &definition.parameters[ordinal] else { panic!("original Type formal"); };
        let mut uppers = Vec::new();
        callable_uppers(requirements, &mut uppers);
        callable_uppers(&definition.requirements, &mut uppers);
        assert!(!uppers.is_empty());
        assert!(uppers.iter().all(|upper| *upper == expected), "{uppers:?}");
    }
    #[test]
    fn consuming_bounds_follow_original_contexts_not_actual_instances() {
        let module = original_module(r#"
fn pending<F:FnOnce()->i32>(callback:F)->i32{task_yield();return callback();}
fn direct<F:FnOnce()->i32>(callback:F)->i32{return callback();}
fn declared<F:FnOnce()->i32>(callback:F)->i32 contract{effects(io);}{task_yield();return callback();}
fn unused<F:FnOnce()->i32>(callback:F){task_yield();}
fn reading<F:Fn()->i32>(callback:F)->i32{task_yield();return callback();}
fn mutating<F:FnMut()->i32>(callback:F)->i32{var work=callback;task_yield();return work();}
fn forwarded<F:FnOnce()->i32>(callback:F)->i32{return pending(callback);}
fn sync_forwarded<F:FnOnce()->i32>(callback:F)->i32{task_yield();return direct(callback);}
fn tuple_alias<F:FnOnce()->i32>(callback:F)->i32{let pair=(callback,);let (work,)=pair;task_yield();return work();}
fn captured<F:FnOnce()->i32>(callback:F)->i32{let worker=move()=>{task_yield();return callback();};return worker();}
fn ranked<F:for<life a> FnOnce(&'a i32)->i32>(callback:F,value:&i32)->i32{task_yield();return callback(value);}
fn where_bound<F>(callback:F)->i32 where F:FnOnce()->i32{task_yield();return callback();}
fn main(){}
"#);
        for name in ["pending", "forwarded", "tuple_alias", "captured", "ranked", "where_bound"] {
            assert_input_upper(function(&module, name), 0, CallKind::Suspend);
        }
        for name in ["direct", "declared", "unused", "reading", "mutating", "sync_forwarded"] {
            assert_input_upper(function(&module, name), 0, CallKind::Direct);
        }
        assert_eq!(signature(function(&module, "captured")).unwrap().call_kind, CallKind::Suspend);
        assert_eq!(signature(function(&module, "declared")).unwrap().call_kind, CallKind::Direct);
        assert_eq!(signature(function(&module, "sync_forwarded")).unwrap().call_kind, CallKind::Suspend);
    }
    #[test]
    fn original_pending_fixtures_publish_the_complete_consuming_upper() {
        for text in [
            include_str!(
                "../../../tests/language-readiness/task-owner-transport/once-pending-callable-cleanup.zil"
            ),
            include_str!(
                "../../../tests/language-readiness/task-owner-transport/once-pending-owned-result-cleanup.zil"
            ),
            include_str!(
                "../../../tests/language-readiness/task-owner-transport/once-pending-returned-task-cleanup.zil"
            ),
        ] {
            let module = original_module(text);
            let parent = function(&module, "parent");
            assert_eq!(signature(parent).unwrap().call_kind, CallKind::Suspend);
            assert_input_upper(parent, 0, CallKind::Suspend);
            let keep = function(&module, "keep");
            // Constructing a callable does not execute its body or cleanup,
            // and the opaque callable result is not an input consuming role.
            assert_eq!(signature(keep).unwrap().call_kind, CallKind::Direct);
            for requirement in &keep.requirements {
                if let SemanticRequirementRecipe::Callable {
                    kind, signature, ..
                } = requirement
                {
                    assert_eq!(
                        signature.call_kind,
                        if *kind == CallableKind::FnOnce {
                            CallKind::Suspend
                        } else {
                            CallKind::Direct
                        }
                    );
                }
            }
        }
    }

    #[test]
    fn opaque_consumption_tracks_remaining_task_not_the_body_abi() {
        let module = original_module(
            r#"
fn close(give task:Task<i32>)->i32{task_finish_destroy(give task);return 0;}
fn observed(give task:Task<i32>)->impl FnOnce()->i32{return move()=>{task_valid(task);return 0;};}
fn transferred(give task:Task<i32>)->impl FnOnce()->Managed<Task<i32>>{return move()=>give task;}
fn closed(give task:Task<i32>)->impl FnOnce()->i32{return move()=>{close(give task);return 0;};}
fn partial(give task:Task<i32>,flag:bool)->impl FnOnce()->i32{return move()=>{if flag{close(give task);return 1;}return 0;};}
fn both(give task:Task<i32>,flag:bool)->impl FnOnce()->i32{return move()=>{if flag{close(give task);return 1;}close(give task);return 0;};}
fn forwarded<F:FnOnce()->i32>(original:F)->impl FnOnce()->i32{return move()=>original();}
fn nonconsuming<F:FnMut()->i32>(original:F)->impl FnOnce()->i32{return move()=>original();}
fn literal()->impl FnOnce()->i32{return move()=>42;}
resource Token{value:i32}
impl Drop for Token{fn drop(self:Self){}}
fn ordinary(give token:Token)->impl FnOnce()->i32{return move()=>token.value;}
struct Holder<T:Value>{item:T}
fn ordinary_nominal(value:Holder<i32>)->impl FnOnce()->i32{return move()=>value.item;}
fn sync_inner()->impl Fn()->i32{return move()=>42;}
fn sync_outer()->impl FnOnce()->i32{let inner=sync_inner();return move()=>inner();}
fn task_inner(give task:Task<i32>)->impl FnMut()->i32{return move()=>{task_valid(task);return 0;};}
fn task_outer(give task:Task<i32>)->impl FnOnce()->i32{var inner=task_inner(give task);return move()=>inner();}
fn through_tuple(give task:Task<i32>)->impl FnOnce()->Managed<Task<i32>>{return move()=>{let pair=(give task,);let (selected,)=pair;return give selected;};}
fn main(){}
"#,
        );
        for (name, expected) in [
            ("observed", CallKind::Suspend),
            ("partial", CallKind::Suspend),
            ("nonconsuming", CallKind::Suspend),
            ("transferred", CallKind::Direct),
            ("closed", CallKind::Direct),
            ("both", CallKind::Direct),
            ("forwarded", CallKind::Direct),
            ("literal", CallKind::Direct),
            ("ordinary", CallKind::Direct),
            ("ordinary_nominal", CallKind::Direct),
            ("through_tuple", CallKind::Direct),
            ("sync_outer", CallKind::Direct),
            ("task_outer", CallKind::Suspend),
        ] {
            let definition = function(&module, name);
            let mut found = false;
            for requirement in &definition.requirements {
                if let SemanticRequirementRecipe::Callable {
                    ty:
                        SemanticTypeRecipe::OriginalType {
                            source: SemanticOriginalTypeSource::Result,
                            ..
                        },
                    kind: CallableKind::FnOnce,
                    signature,
                } = requirement
                {
                    assert_eq!(signature.call_kind, expected, "{name}");
                    found = true;
                }
            }
            assert!(found, "{name}");
            assert_eq!(signature(definition).unwrap().call_kind, CallKind::Direct);
        }
        // Candidate formation does not establish the close helper's Closed
        // precondition, nor permit any actual Direct consume to suspend.
    }
}
