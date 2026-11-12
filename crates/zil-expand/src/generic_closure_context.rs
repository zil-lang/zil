//! Request-local transport from an actual Engine closure visit to CoreLower.
//! Rows are proposals, not permissions. Capture *slots* belong to Types and
//! never appear here; input ordinals below refer to the real creator header.
use super::*;
use crate::source_context::DeclarationOrigin;
use zil_base::{ModuleId, NodeId};
use zil_ir::{SignatureLifeClass, StorageBinder, StorageLifeContract, StoragePosition};

#[derive(Debug)]
pub(crate) struct Owner {
    pub(crate) origin: origins::Origin,
    pub(crate) source: DeclarationOrigin,
    /// Complete original generic order, never lifetime-compacted.
    pub(crate) parameters: Vec<GenericParam>,
    pub(crate) header: StorageLifeContract,
    /// Header binder -> original generic ordinal. None is an actual anonymous
    /// input class, not an invented declaration parameter.
    pub(crate) fixed_sources: Vec<Option<u32>>,
    pub(crate) invocation_sources: Vec<Option<u32>>,
    shared_inputs: BTreeSet<u32>,
    anonymous_end: u32,
}

#[derive(Clone, Debug)]
pub(crate) enum InputSelector {
    Borrow(StoragePosition),
    Class(SignatureLifeClass),
}

#[derive(Clone, Debug)]
pub(crate) struct InputAnchor {
    /// Actual runtime parameter ordinal in the creator, not a capture slot.
    pub(crate) parameter: u32,
    /// Full original generic ordinal, only for a genuinely declared lifetime.
    pub(crate) source_parameter: Option<u32>,
    pub(crate) header_binder: StorageBinder,
    pub(crate) selector: InputSelector,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct NamedFixed {
    /// Closure signature Fixed ordinal, not the header Fixed ordinal.
    pub(crate) fixed: u32,
    /// Full original creator generic ordinal; owner identifies its scope.
    pub(crate) parameter: u32,
}

#[derive(Debug)]
pub(crate) struct ProducedClosure {
    pub(crate) owner: u32,
    /// A fresh actual production event attached to this concrete AST node.
    /// It is never reconciled by assuming two traversals have the same order.
    pub(crate) occurrence: u32,
    pub(crate) signature: StorageLifeContract,
    pub(crate) named_fixed: Vec<NamedFixed>,
    pub(crate) inputs: Vec<InputAnchor>,
}

#[derive(Default)]
pub(crate) struct Ledger {
    pub(crate) owners: Vec<Owner>,
    entries: Vec<Option<ProducedClosure>>,
    creators: BTreeMap<(ModuleId, NodeId), u32>,
    closures: BTreeSet<(ModuleId, NodeId)>,
}

impl Ledger {
    pub(crate) fn owner(&self, index: u32) -> Option<&Owner> {
        self.owners.get(index as usize)
    }

    /// Consume after lowering the body and minting the actual outer closure
    /// id. `charge` must debit the caller's live request remainder. Moving the
    /// proposal out avoids a second uncharged signature/identity clone.
    pub(crate) fn consume(
        &mut self,
        raw: u32,
        actual_origin: origins::Origin,
        creator: NodeId,
        closure: NodeId,
        mut charge: impl FnMut(usize) -> R<()>,
    ) -> R<ProducedClosure> {
        charge(
            self.creators
                .len()
                .saturating_add(self.closures.len())
                .saturating_add(2),
        )?;
        let proposal = self
            .entries
            .get(raw as usize)
            .and_then(Option::as_ref)
            .ok_or_else(|| unavailable("closure context marker is absent or already consumed"))?;
        if proposal.occurrence != raw {
            return Err(unavailable(
                "closure context marker does not select its actual production",
            ));
        }
        let owner = self
            .owner(proposal.owner)
            .ok_or_else(|| unavailable("closure context owner is absent"))?;
        if owner.origin != actual_origin {
            return Err(unavailable(
                "closure context belongs to another actual creator",
            ));
        }
        let module = actual_origin.declaration.module;
        if self
            .creators
            .get(&(module, creator))
            .is_some_and(|index| *index != proposal.owner)
            || self
                .creators
                .iter()
                .any(|(key, index)| *index == proposal.owner && *key != (module, creator))
            || self.closures.contains(&(module, closure))
        {
            return Err(unavailable(
                "closure context creator or expression identity is reused",
            ));
        }
        self.creators.insert((module, creator), proposal.owner);
        self.closures.insert((module, closure));
        Ok(self.entries[raw as usize]
            .take()
            .expect("checked actual proposal"))
    }

    /// Reconcile once after *all* output modules, not after one module builder.
    pub(crate) fn finish(&self, mut charge: impl FnMut(usize) -> R<()>) -> R<()> {
        charge(self.entries.len())?;
        if self.entries.iter().any(Option::is_some) {
            return Err(unavailable(
                "source closure context has no actual lowered expression",
            ));
        }
        Ok(())
    }
}

fn unavailable(message: &str) -> Diagnostic {
    Diagnostic::unsupported(Phase::Expand, message, None)
}

fn scalar_result(ty: &TypeRef) -> bool {
    match &ty.kind {
        TypeKind::Group(inner) => scalar_result(inner),
        TypeKind::Named { path, args } if path.len() == 1 && args.is_empty() => matches!(
            path[0].as_str(),
            "unit"
                | "void"
                | "bool"
                | "i32"
                | "i64"
                | "u8"
                | "u16"
                | "u32"
                | "i8"
                | "i16"
                | "isize"
                | "u64"
                | "usize"
                | "f32"
                | "f64"
        ),
        _ => false,
    }
}

/// This scan limits the first profile. It discovers rewrite *capability* in
/// the actual aligned creator; it neither numbers closures nor matches two
/// traversals. It runs only after a contextual closure actually requests an
/// owner, never as an unrelated whole-module discovery fee.
fn original_creator_capability(item: &Item, budget: &mut Budget<'_>) -> R<()> {
    use crate::ct_selection::Visitor;
    struct Original<'a, 'l> {
        budget: &'a mut Budget<'l>,
    }
    impl Visitor for Original<'_, '_> {
        fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
            self.budget.node(span, depth).map_err(|error| vec![error])
        }
        fn text(&mut self, value: &str, span: Span) -> CompileResult<()> {
            self.budget.text(value, span).map_err(|error| vec![error])
        }
        fn statement(&mut self, statement: &Stmt, depth: usize) -> CompileResult<()> {
            if matches!(statement.kind, StmtKind::Meta(_)) {
                return Err(vec![unavailable(
                    "closure creator contains CT rewrite capability",
                )]);
            }
            crate::ct_selection::walk_statement(self, statement, depth)
        }
        fn expression(&mut self, expression: &Expr, depth: usize) -> CompileResult<()> {
            if matches!(
                expression.kind,
                ExprKind::MetaEval(_)
                    | ExprKind::MetaSplice(_)
                    | ExprKind::MetaBlock(_)
                    | ExprKind::SyntaxIsland { .. }
                    | ExprKind::Quote { .. }
            ) {
                return Err(vec![unavailable(
                    "closure creator contains CT rewrite capability",
                )]);
            }
            crate::ct_selection::walk_expression(self, expression, depth)
        }
        fn ty(&mut self, ty: &TypeRef, depth: usize) -> CompileResult<()> {
            if matches!(
                ty.kind,
                TypeKind::MetaEval(_) | TypeKind::MetaSplice(_) | TypeKind::MetaBlock(_)
            ) {
                return Err(vec![unavailable(
                    "closure creator contains CT rewrite capability",
                )]);
            }
            crate::ct_selection::walk_type(self, ty, depth)
        }
    }
    if !item.attributes.is_empty() {
        return Err(unavailable(
            "closure creator attributes need a complete producer",
        ));
    }
    Original { budget }
        .item(item, 0)
        .map_err(|mut errors| errors.remove(0))
}

pub(super) struct Recording {
    owner: u32,
    occurrences: Vec<crate::stored_types::Occurrence>,
    captured: BTreeSet<u32>,
}

impl Engine<'_> {
    fn closure_owner(&mut self, span: Span) -> R<u32> {
        let origin = self
            .current_origin
            .ok_or_else(|| unavailable("closure creator has no original declaration"))?;
        if origin.member.is_some()
            || origin.provider
            || origin.compiler
            || origin.declaration.compiler
        {
            return Err(unavailable(
                "closure source context requires an original free-function Definition",
            ));
        }
        self.budget.identity_work(
            self.products
                .closure_contexts
                .owners
                .len()
                .saturating_add(1),
            span,
        )?;
        if let Some(index) = self
            .products
            .closure_contexts
            .owners
            .iter()
            .position(|owner| owner.origin == origin)
        {
            return u32::try_from(index)
                .map_err(|_| unavailable("closure owner ordinal exhausted"));
        }
        let origins = self
            .origins
            .ok_or_else(|| unavailable("closure creator source lineage is unavailable"))?;
        self.budget
            .identity_work(origins.entries.len().saturating_add(1), span)?;
        let index = origins
            .entries
            .iter()
            .position(|candidate| *candidate == origin)
            .ok_or_else(|| unavailable("closure creator has no actual original AST item"))?;
        let item = self
            .original_module
            .items
            .get(index)
            .ok_or_else(|| unavailable("closure creator original AST is absent"))?;
        let ItemKind::Function(function) = &item.kind else {
            return Err(unavailable(
                "closure source context requires a free-function Definition",
            ));
        };
        if function.generics.is_empty()
            || !function
                .generics
                .iter()
                .all(|parameter| matches!(parameter.kind, GenericParamKind::Lifetime))
            || !function.where_clause.is_empty()
            || matches!(function.body, FunctionBody::Declaration)
            || function
                .return_type
                .as_ref()
                .is_some_and(|result| !scalar_result(result))
        {
            return Err(unavailable(
                "closure creator requires the complete life-only Definition profile",
            ));
        }
        if self.life_scope.len() != function.generics.len() {
            return Err(unavailable(
                "closure creator lost its actual source lifetime scope",
            ));
        }
        for (name, parameter) in self.life_scope.iter().zip(&function.generics) {
            self.budget.text(name, span)?;
            self.budget.text(&parameter.name, span)?;
            if name != &parameter.name {
                return Err(unavailable("closure creator source lifetime order changed"));
            }
        }
        let source = origins.identity(origin.declaration, &mut self.budget, span)?;
        if source.closure_sites != crate::source_context::ClosureSiteLineage::Original {
            return Err(unavailable(
                "closure creator has no original expression-site lineage",
            ));
        }
        origins::pay_identity(&source.identity, &mut self.budget, span)?;
        origins::pay_declaration_copy(source, &mut self.budget, span)?;
        if !source.identity.generation_path.is_empty() {
            return Err(unavailable(
                "generated closure creators need complete materialization lineage",
            ));
        }
        let source = source.clone();
        original_creator_capability(item, &mut self.budget)?;
        for parameter in &function.generics {
            self.budget.node(parameter.span, 0)?;
            self.budget.text(&parameter.name, parameter.span)?;
        }
        let parameters = function.generics.clone();
        self.budget
            .identity_work(function.params.len().saturating_add(1), span)?;
        let mut inputs = Vec::with_capacity(function.params.len());
        for parameter in &function.params {
            self.budget.ty(&parameter.ty)?;
            inputs.push(parameter.ty.clone());
        }
        let mut result = if let Some(result) = &function.return_type {
            self.budget.ty(result)?;
            Some(result.clone())
        } else {
            None
        };
        let mut occurrences = Vec::new();
        let mut stored = false;
        let mut next = u32::try_from(parameters.len())
            .map_err(|_| unavailable("creator source lifetime count exhausted"))?;
        for (index, ty) in inputs.iter_mut().enumerate() {
            self.storage_header(
                ty,
                zil_ir::StorageBase::Parameter(index as u32),
                &mut Vec::new(),
                &mut Vec::new(),
                &mut occurrences,
                &mut stored,
                &mut next,
                0,
            )?;
        }
        if let Some(ty) = &mut result {
            self.storage_header(
                ty,
                zil_ir::StorageBase::Result,
                &mut Vec::new(),
                &mut Vec::new(),
                &mut occurrences,
                &mut stored,
                &mut next,
                0,
            )?;
        }
        if !stored {
            return Err(unavailable(
                "closure creator requires an actual qualified input header",
            ));
        }
        let (header, fixed, invocation) = crate::stored_types::contract_with_sources(
            occurrences,
            Vec::new(),
            &mut self.budget,
            span,
        )?;
        let mut shared_inputs = BTreeSet::new();
        for (index, ty) in inputs.iter().enumerate() {
            let mut outer = ty;
            while let TypeKind::Group(inner) = &outer.kind {
                outer = inner;
            }
            if !matches!(outer.kind, TypeKind::Reference { mutable: false, .. }) {
                continue;
            }
            let actual = self.products.storage.pointer_parameter_type(
                ty,
                &self.nominals,
                &mut self.budget,
                0,
            )?;
            if let zil_ir::Type::Address {
                mutable: false,
                inner,
            } = actual
                && let zil_ir::Type::BorrowedStorage { life, .. } = inner.as_ref()
                && life
                    .slots
                    .iter()
                    .all(|slot| slot.mode == zil_ir::StoredBorrowMode::SharedAddress)
                && life.retained.is_empty()
                && header.classes.iter().any(|binding| {
                    binding.position.base == zil_ir::StorageBase::Parameter(index as u32)
                })
            {
                self.budget
                    .identity_work(shared_inputs.len().saturating_add(1), span)?;
                shared_inputs.insert(index as u32);
            }
        }
        let parameter_count = parameters.len();
        let source_of = |life: crate::stored_types::LexicalLife| {
            ((life.0 as usize) < parameter_count).then_some(life.0)
        };
        self.budget.identity_work(
            fixed
                .len()
                .saturating_add(invocation.len())
                .saturating_add(1),
            span,
        )?;
        let owner = Owner {
            origin,
            source,
            parameters,
            header,
            fixed_sources: fixed.into_iter().map(source_of).collect(),
            invocation_sources: invocation.into_iter().map(source_of).collect(),
            shared_inputs,
            anonymous_end: next,
        };
        let index = u32::try_from(self.products.closure_contexts.owners.len())
            .map_err(|_| unavailable("closure owner ordinal exhausted"))?;
        self.budget.identity_work(1, span)?;
        self.products.closure_contexts.owners.push(owner);
        Ok(index)
    }

    pub(super) fn start_closure_context(
        &mut self,
        parameters: &mut [ClosureParam],
        result: &mut Option<TypeRef>,
        depth: u32,
        span: Span,
    ) -> R<Recording> {
        if depth != 0 || self.closure_recording.is_some() {
            return Err(unavailable(
                "nested closure source contexts require their complete producer",
            ));
        }
        let owner = self.closure_owner(span)?;
        let mut next = self.products.closure_contexts.owners[owner as usize].anonymous_end;
        let mut occurrences = Vec::new();
        let mut stored = false;
        for (index, parameter) in parameters.iter_mut().enumerate() {
            let ty = parameter.ty.as_mut().ok_or_else(|| {
                unavailable("contextual closure parameter needs an actual source annotation")
            })?;
            self.storage_header(
                ty,
                zil_ir::StorageBase::Parameter(index as u32),
                &mut Vec::new(),
                &mut Vec::new(),
                &mut occurrences,
                &mut stored,
                &mut next,
                0,
            )?;
        }
        // Environment results carry no signature-life row: their actual
        // BorrowCapture source and receiver loan are proved after typing.
        if let Some(result) = result.as_mut() {
            if scalar_result(result) {
                self.storage_header(
                    result,
                    zil_ir::StorageBase::Result,
                    &mut Vec::new(),
                    &mut Vec::new(),
                    &mut occurrences,
                    &mut stored,
                    &mut next,
                    0,
                )?;
            } else if !matches!(&result.kind,
                TypeKind::Reference { mutable:false, lifetime:None, inner }
                    if scalar_result(inner))
            {
                return Err(unavailable(
                    "contextual closure result needs the complete scalar or environment lending profile",
                ));
            }
        }
        Ok(Recording {
            owner,
            occurrences,
            captured: BTreeSet::new(),
        })
    }

    pub(super) fn record_closure_input(&mut self, name: &str, env: &Env, span: Span) -> R<()> {
        if self.closure_recording.is_none() {
            return Ok(());
        }
        self.budget
            .identity_work(env.len().saturating_add(name.len()).saturating_add(1), span)?;
        let Some(binding) = env.bindings.get(name) else {
            return Ok(());
        };
        if binding.closure_depth >= env.closure_depth {
            return Ok(());
        }
        if let Some(parameter) = binding.input {
            let recording = self.closure_recording.as_mut().expect("active producer");
            self.budget
                .identity_work(recording.captured.len().saturating_add(1), span)?;
            recording.captured.insert(parameter);
        } else if let Some(ty) = &binding.ty {
            // A local qualified value, including an identical-name/type shadow,
            // is not an input anchor. Ordinary scalar/owned captures retain
            // their established checking path and add no generic binding.
            rich_pointer::reject_fixed_closure(
                ty,
                &self.products.storage.instances,
                &mut self.budget,
            )?;
        }
        Ok(())
    }

    pub(super) fn finish_closure_context(&mut self, recording: Recording, span: Span) -> R<u32> {
        let owner = &self.products.closure_contexts.owners[recording.owner as usize];
        self.budget.identity_work(
            recording
                .captured
                .len()
                .saturating_add(owner.shared_inputs.len())
                .saturating_add(1),
            span,
        )?;
        if !recording
            .captured
            .iter()
            .any(|input| owner.shared_inputs.contains(input))
        {
            return Err(unavailable(
                "closure context needs an actual shared qualified creator input capture",
            ));
        }
        let mut inputs = Vec::new();
        for &parameter in &recording.captured {
            self.budget.identity_work(
                owner
                    .header
                    .classes
                    .len()
                    .saturating_add(owner.header.bindings.len())
                    .saturating_add(1),
                span,
            )?;
            if owner
                .header
                .classes
                .iter()
                .any(|binding| binding.position.base == zil_ir::StorageBase::Parameter(parameter))
                && !owner.shared_inputs.contains(&parameter)
            {
                return Err(unavailable(
                    "closure context only supports shared qualified input anchors",
                ));
            }
            for binding in &owner.header.bindings {
                if binding.position.base != zil_ir::StorageBase::Parameter(parameter) {
                    continue;
                }
                self.budget
                    .identity_work(binding.position.path.len().saturating_add(1), span)?;
                inputs.push(InputAnchor {
                    parameter,
                    source_parameter: binder_source(owner, binding.binder)?,
                    header_binder: binding.binder,
                    selector: InputSelector::Borrow(binding.position.clone()),
                });
            }
            for binding in &owner.header.classes {
                if binding.position.base != zil_ir::StorageBase::Parameter(parameter) {
                    continue;
                }
                self.budget
                    .identity_work(binding.position.path.len().saturating_add(1), span)?;
                inputs.push(InputAnchor {
                    parameter,
                    source_parameter: binder_source(owner, binding.binder)?,
                    header_binder: binding.binder,
                    selector: InputSelector::Class(binding.position.clone()),
                });
            }
        }
        // Declared outer lifetime names retain their meaning even when the
        // actual capture fails to ground them. The independent semantic
        // consumers must reject that nearest mismatch, not freshen it into an
        // Invocation class. Anonymous closure input borrows stay Invocation.
        self.budget
            .identity_work(owner.parameters.len().saturating_add(1), span)?;
        let declared = (0..owner.parameters.len())
            .map(|index| crate::stored_types::LexicalLife(index as u32))
            .collect();
        let (signature, fixed, _) = crate::stored_types::contract_with_sources(
            recording.occurrences,
            declared,
            &mut self.budget,
            span,
        )?;
        let mut named_fixed = Vec::new();
        self.budget
            .identity_work(fixed.len().saturating_add(1), span)?;
        for (fixed, source) in fixed.into_iter().enumerate() {
            if (source.0 as usize) < owner.parameters.len() {
                named_fixed.push(NamedFixed {
                    fixed: fixed as u32,
                    parameter: source.0,
                });
            }
        }
        if named_fixed.is_empty() {
            return Err(unavailable(
                "closure annotation has no complete original named-class bridge",
            ));
        }
        let raw = u32::try_from(self.products.closure_contexts.entries.len())
            .map_err(|_| unavailable("source closure occurrence ordinal exhausted"))?;
        if raw == u32::MAX {
            return Err(Diagnostic::limit(
                Phase::Expand,
                "source closure occurrence collides with the absent-marker sentinel",
                Some(span),
            ));
        }
        self.budget.identity_work(1, span)?;
        self.products
            .closure_contexts
            .entries
            .push(Some(ProducedClosure {
                owner: recording.owner,
                occurrence: raw,
                signature,
                named_fixed,
                inputs,
            }));
        Ok(raw)
    }
}

fn binder_source(owner: &Owner, binder: StorageBinder) -> R<Option<u32>> {
    let source = match binder {
        StorageBinder::Fixed(index) => owner.fixed_sources.get(index as usize),
        StorageBinder::Invocation(index) => owner.invocation_sources.get(index as usize),
    };
    source
        .copied()
        .ok_or_else(|| unavailable("actual creator header binder has no source inverse"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_context::{DeclarationPlacement, SourceIdentityContext};
    use zil_base::{FileId, SourceUnit};

    const DECL: &str = "struct Tag<life s>{p:&'s i64}";
    const BODY: &str = "fn apply<life s>(left:&Tag<'s>)->i32{let f=(p:&Tag<'s>)->i32=>i32(*p.p+*left.p);return f(left)}";

    fn produce(
        text: &str,
        inject: bool,
        steps: usize,
    ) -> CompileResult<(Module, products::Products)> {
        let input = SourceUnit::new(FileId(0), ModuleId(0), "context-producer.zil", text);
        let mut syntax = zil_syntax::parse(&input).into_result()?;
        if inject {
            let ItemKind::Function(function) = &mut syntax.items[1].kind else {
                panic!("fixture function")
            };
            let FunctionBody::Block(body) = &mut function.body else {
                panic!("fixture body")
            };
            let StmtKind::Binding { value, .. } = &mut body.statements[0].kind else {
                panic!("fixture binding")
            };
            let ExprKind::Closure { context, .. } = &mut value.kind else {
                panic!("fixture closure")
            };
            *context = ClosureContextMarker::untrusted(912);
        }
        let sources = [input];
        let aliases = BTreeMap::new();
        let mut products = products::Products::default();
        let limits = Limits {
            max_analysis_steps: steps,
            ..Limits::default()
        };
        let module = specialize_products_with_context(
            &syntax,
            &limits,
            &mut products,
            Some((
                &sources,
                SourceIdentityContext::ClosedSnapshot {
                    root: ModuleId(0),
                    aliases: &aliases,
                    declarations: DeclarationPlacement::Original,
                },
            )),
        )?
        .into_owned();
        Ok((module, products))
    }

    fn markers(module: &Module) -> Vec<Option<u32>> {
        use crate::ct_selection::Visitor;
        struct Find(Vec<Option<u32>>);
        impl Visitor for Find {
            fn node(&mut self, _: Span, _: usize) -> CompileResult<()> {
                Ok(())
            }
            fn text(&mut self, _: &str, _: Span) -> CompileResult<()> {
                Ok(())
            }
            fn expression(&mut self, e: &Expr, depth: usize) -> CompileResult<()> {
                if let ExprKind::Closure { context, .. } = &e.kind {
                    self.0.push(context.raw_index());
                }
                crate::ct_selection::walk_expression(self, e, depth)
            }
        }
        let mut found = Find(Vec::new());
        for item in &module.items {
            found.item(item, 0).unwrap();
        }
        found.0
    }

    #[test]
    fn source_closure_context_actual_input_and_raw_marker_reproduction() {
        for inject in [false, true] {
            let (module, products) = produce(
                &format!("{DECL}{BODY}"),
                inject,
                Limits::default().max_analysis_steps,
            )
            .unwrap();
            assert_eq!(markers(&module), [Some(0)]);
            let owner = products.closure_contexts.owner(0).unwrap();
            assert_eq!(owner.origin.declaration.ordinal, 1);
            assert!(owner.origin.member.is_none());
            assert_eq!(owner.parameters.len(), 1);
            assert!(owner.source.identity.generation_path.is_empty());
            assert_eq!(owner.fixed_sources, [Some(0)]);
            assert!(owner.invocation_sources.contains(&None));
            let proposal = products.closure_contexts.entries[0].as_ref().unwrap();
            assert_eq!(proposal.signature.fixed_binder_count, 1);
            assert_eq!(proposal.signature.invocation_binder_count, 1);
            assert_eq!(
                (
                    proposal.named_fixed[0].fixed,
                    proposal.named_fixed[0].parameter
                ),
                (0, 0)
            );
            assert!(proposal.inputs.iter().any(|anchor| anchor.parameter == 0
                && anchor.source_parameter == Some(0)
                && matches!(&anchor.selector, InputSelector::Class(position)
                    if position.base == zil_ir::StorageBase::Parameter(0) && position.path == [zil_ir::LifeTypeProjection::Pointee])));
            assert!(proposal.inputs.iter().any(|anchor| anchor.source_parameter.is_none()
                && matches!(&anchor.selector, InputSelector::Borrow(position)
                    if position.base == zil_ir::StorageBase::Parameter(0) && position.path.is_empty())
                && matches!(anchor.header_binder, StorageBinder::Invocation(_))));
        }
    }

    #[test]
    fn source_closure_context_keeps_full_life_and_runtime_ordinals_separate() {
        let text = format!(
            "{DECL}fn apply<life unused,life s>(n:i32,m:i32,left:&Tag<'s>)->i32{{let f=(p:&Tag<'s>)->i32=>i32(*p.p+*left.p)+n+m;return f(left)}}"
        );
        let (_, products) = produce(&text, false, Limits::default().max_analysis_steps).unwrap();
        let owner = products.closure_contexts.owner(0).unwrap();
        assert_eq!(owner.parameters.len(), 2);
        assert_eq!(owner.fixed_sources, [Some(1)]);
        let row = products.closure_contexts.entries[0].as_ref().unwrap();
        assert_eq!(
            (row.named_fixed[0].fixed, row.named_fixed[0].parameter),
            (0, 1)
        );
        assert!(
            row.inputs
                .iter()
                .any(|anchor| anchor.parameter == 2 && anchor.source_parameter == Some(1))
        );
        assert!(row.inputs.iter().all(|anchor| anchor.parameter == 2));
    }

    #[test]
    fn source_closure_context_shadow_nested_and_local_class_do_not_become_inputs() {
        for (body, message) in [
            (
                "let left:&Tag<'s>=left;let f=(p:&Tag<'s>)->i32=>i32(*p.p+*left.p);return f(left)",
                "genuine enclosing definition context",
            ),
            (
                "let f=(left:&Tag<'s>)->i32=>i32(*left.p);return f(left)",
                "actual shared qualified creator input capture",
            ),
            (
                "let f=(p:&Tag<'s>)->i32=>{let inner=()=>1;return i32(*left.p)+inner()};return f(left)",
                "nested closure source contexts",
            ),
        ] {
            let text = format!("{DECL}fn apply<life s>(left:&Tag<'s>)->i32{{{body}}}");
            let Err(errors) = produce(&text, false, Limits::default().max_analysis_steps) else {
                panic!("unsupported context was produced")
            };
            assert!(
                errors.iter().any(
                    |error| error.category == zil_base::ResultCategory::Unsupported
                        && error.message.contains(message)
                ),
                "{errors:?}"
            );
        }
    }

    #[test]
    fn source_closure_context_one_owned_ledger_consumes_actual_id_once() {
        let (_, mut products) = produce(
            &format!("{DECL}{BODY}"),
            false,
            Limits::default().max_analysis_steps,
        )
        .unwrap();
        let origin = products.closure_contexts.owner(0).unwrap().origin;
        assert!(products.closure_contexts.finish(|_| Ok(())).is_err());
        let mut wrong = origin;
        wrong.declaration.ordinal += 1;
        assert!(
            products
                .closure_contexts
                .consume(912, origin, NodeId(80), NodeId(81), |_| Ok(()))
                .is_err()
        );
        assert!(
            products
                .closure_contexts
                .consume(0, wrong, NodeId(80), NodeId(81), |_| Ok(()))
                .is_err()
        );
        let paid = products
            .closure_contexts
            .consume(0, origin, NodeId(80), NodeId(81), |_| {
                Err(Diagnostic::limit(
                    Phase::Expand,
                    "live ledger budget exhausted",
                    None,
                ))
            });
        assert_eq!(
            paid.unwrap_err().category,
            zil_base::ResultCategory::ResourceLimit
        );
        let produced = products
            .closure_contexts
            .consume(0, origin, NodeId(80), NodeId(81), |_| Ok(()))
            .unwrap();
        assert_eq!(produced.occurrence, 0);
        assert!(
            products
                .closure_contexts
                .consume(0, origin, NodeId(80), NodeId(82), |_| Ok(()))
                .is_err()
        );
        products.closure_contexts.finish(|_| Ok(())).unwrap();
    }

    #[test]
    fn source_closure_context_live_budget_is_repeatable() {
        let text = format!("{DECL}{BODY}");
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            match produce(&text, false, middle) {
                Ok(_) => high = middle,
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|error| error.category == zil_base::ResultCategory::ResourceLimit),
                        "{errors:?}"
                    );
                    low = middle;
                }
            }
        }
        produce(&text, false, high).unwrap();
        let Err(errors) = produce(&text, false, high - 1) else {
            panic!("N-1 producer succeeded")
        };
        assert!(
            errors
                .iter()
                .all(|error| error.category == zil_base::ResultCategory::ResourceLimit)
        );
        produce(&text, false, high).unwrap();
        println!("actual source closure producer N={high}; N-1 ResourceLimit; fresh retry agrees");
    }

    #[test]
    fn source_closure_context_preserves_actual_fnmut_and_fnonce_capture_syntax() {
        for (extra, body) in [
            (
                "",
                "var count=0;var f=(p:&Tag<'s>)->i32=>{count+=1;return i32(*p.p+*left.p)+count-1};return f(left)",
            ),
            (
                "resource Mark{n:i32}",
                "let mark=Mark{n:1};let f=move(p:&Tag<'s>)->i32=>{let taken=give mark;return i32(*p.p+*left.p)+taken.n-1};return f(left)",
            ),
        ] {
            let text = format!("{DECL}{extra}fn apply<life s>(left:&Tag<'s>)->i32{{{body}}}");
            let (module, products) =
                produce(&text, false, Limits::default().max_analysis_steps).unwrap();
            assert_eq!(markers(&module), [Some(0)]);
            assert!(
                products.closure_contexts.entries[0]
                    .as_ref()
                    .unwrap()
                    .inputs
                    .iter()
                    .any(|input| input.source_parameter == Some(0))
            );
        }
    }

    #[test]
    fn source_closure_context_wrong_capture_class_remains_an_unverified_proposal() {
        let text = format!(
            "{DECL}fn apply<life a,life b>(left:&Tag<'a>,right:&Tag<'b>)->i32{{let f=(p:&Tag<'a>)->i32=>i32(*p.p+*right.p);return f(left)}}"
        );
        let (_, products) = produce(&text, false, Limits::default().max_analysis_steps).unwrap();
        let proposal = products.closure_contexts.entries[0].as_ref().unwrap();
        assert_eq!(proposal.named_fixed[0].parameter, 0);
        assert!(
            proposal
                .inputs
                .iter()
                .any(|input| input.parameter == 1 && input.source_parameter == Some(1))
        );
        assert!(
            proposal
                .inputs
                .iter()
                .all(|input| input.source_parameter != Some(0))
        );
        // No Typed/kernel/native acceptance is claimed: the independent
        // consumers must reject this genuinely different captured class.
    }

    #[test]
    fn source_closure_context_ordinary_processed_node_clears_raw_marker_without_rows() {
        let text = format!("{DECL}fn apply()->i32{{let f=(n:i32)->i32=>n+1;return f(41)}}");
        let (module, products) =
            produce(&text, true, Limits::default().max_analysis_steps).unwrap();
        assert_eq!(markers(&module), [None]);
        assert!(products.closure_contexts.entries.is_empty());
        assert!(products.closure_contexts.owners.is_empty());
    }

    #[test]
    fn source_closure_context_direct_original_creator_rejects_ct_rewrite_capability() {
        for (prefix, actual_stage_message) in [
            ("let ignored=#{yield 1};", "CT rewrite capability"),
            (
                "#if(true){let ignored=1}",
                "generic expansion of meta statement",
            ),
            (
                "#for(n in [0]){let ignored=n}",
                "generic expansion of meta statement",
            ),
        ] {
            let text = format!(
                "{DECL}fn apply<life s>(left:&Tag<'s>)->i32{{{prefix}let f=(p:&Tag<'s>)->i32=>i32(*p.p+*left.p);return f(left)}}"
            );
            let Err(errors) = produce(&text, false, Limits::default().max_analysis_steps) else {
                panic!("direct Original metadata hid a CT-capable creator")
            };
            assert!(
                errors
                    .iter()
                    .any(|error| error.message.contains(actual_stage_message)),
                "{errors:?}"
            );
            // MetaIf/For currently stop in the earlier generic preflight.
            // The independently exercised capability component still closes
            // them if that producer later learns to materialize statements.
            let input = SourceUnit::new(FileId(0), ModuleId(0), "capability-component.zil", &text);
            let syntax = zil_syntax::parse(&input).into_result().unwrap();
            let error =
                original_creator_capability(&syntax.items[1], &mut Budget::new(&Limits::default()))
                    .unwrap_err();
            assert!(error.message.contains("CT rewrite capability"));
        }
        // Lexical text is not a capability or an expression identity.
        let text = format!(
            "{DECL}fn apply<life s>(left:&Tag<'s>)->i32{{let text=\"#{{yield 1}}\";let f=(p:&Tag<'s>)->i32=>i32(*p.p+*left.p);return f(left)}}"
        );
        produce(&text, false, Limits::default().max_analysis_steps).unwrap();
    }

    #[test]
    fn source_closure_context_aligned_declaration_without_original_sites_is_closed() {
        let text = format!("{DECL}{BODY}");
        let input = SourceUnit::new(FileId(0), ModuleId(0), "unavailable-sites.zil", &text);
        let syntax = zil_syntax::parse(&input).into_result().unwrap();
        let sources = [input];
        let modules = [&syntax];
        let aliases = BTreeMap::new();
        let (captured, remaining) = crate::source_context::capture_original(
            &sources,
            &modules,
            SourceIdentityContext::ClosedSnapshot {
                root: ModuleId(0),
                aliases: &aliases,
                declarations: DeclarationPlacement::Original,
            },
            &Limits::default(),
        )
        .unwrap();
        let mut captured = captured.expect("real source declaration inventory");
        let original_nominal_identity = captured[0].items[0].clone();
        captured[0].items[1].closure_sites = crate::source_context::ClosureSiteLineage::Unavailable;
        let mut products = products::Products::default();
        let result = specialize_products_with_context(
            &syntax,
            &remaining,
            &mut products,
            Some((
                &sources,
                SourceIdentityContext::ClosedSnapshot {
                    root: ModuleId(0),
                    aliases: &aliases,
                    declarations: DeclarationPlacement::Aligned(&captured),
                },
            )),
        );
        let Err(errors) = result else {
            panic!("unavailable closure site was treated as original")
        };
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("original expression-site lineage")),
            "{errors:?}"
        );
        assert_eq!(captured[0].items[0], original_nominal_identity);
    }
}
