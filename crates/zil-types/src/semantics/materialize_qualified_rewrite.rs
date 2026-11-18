//! One request-local replacement pass over every actual Type payload. Changes
//! are private until finish completes; an error aborts the whole publication.
use super::*;
use crate::aggregates::dyn_metadata::{self as dynamic, TypeResolver};

pub(super) struct Rewrite<'a> {
    pub replacements: &'a [(Type, Type)],
    pub limits: &'a Limits,
    pub left: &'a mut usize,
    changed: bool,
    invalidated: BTreeSet<ClosureId>,
}
impl Rewrite<'_> {
    fn pay(&mut self, work: usize) -> R<()> {
        *self.left = self.left.checked_sub(work).ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "qualified type rewrite work exceeded", None)
        })?;
        Ok(())
    }
    fn rewrite(&mut self, value: &mut Type, depth: usize, root: bool) -> R<()> {
        self.charge(depth, Span::default())?;
        if root {
            for (before, after) in self.replacements {
                inference::charge_type_snapshot(
                    before,
                    Span::default(),
                    self.limits,
                    self.left,
                    1,
                )?;
                inference::charge_type_snapshot(value, Span::default(), self.limits, self.left, 1)?;
                if value == before {
                    inference::charge_type_snapshot(
                        after,
                        Span::default(),
                        self.limits,
                        self.left,
                        1,
                    )?;
                    *value = after.clone();
                    self.changed = true;
                    // Children can contain other independently formed applications.
                    return self.rewrite(value, depth + 1, false);
                }
            }
        }
        match value {
            Type::BorrowedStorage { storage, .. } => self.rewrite(storage, depth + 1, false)?,
            Type::OwnedStore { element }
            | Type::Array { element, .. }
            | Type::Slice { element }
            | Type::Mutex { payload: element }
            | Type::Registration {
                environment: element,
            }
            | Type::AsyncRegistration {
                environment: element,
            }
            | Type::Address { inner: element, .. }
            | Type::Managed(element)
            | Type::Desc(element)
            | Type::Opaque {
                hidden: element, ..
            } => self.rewrite(element, depth + 1, true)?,
            Type::Task {
                result,
                retained_life,
            }
            | Type::MutexGuard {
                payload: result,
                retained_life,
            } => {
                self.rewrite(result, depth + 1, true)?;
                for port in &mut retained_life.ports {
                    self.rewrite(&mut port.leaf, depth + 1, true)?;
                }
            }
            Type::Tuple(fields) => {
                for field in fields {
                    self.rewrite(field, depth + 1, true)?;
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                for (_, field) in fields {
                    self.rewrite(field, depth + 1, true)?;
                }
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    for field in fields {
                        self.rewrite(field, depth + 1, true)?;
                    }
                }
            }
            Type::ErrorUnion { error, result } => {
                self.rewrite(error, depth + 1, true)?;
                self.rewrite(result, depth + 1, true)?;
            }
            Type::FunctionPointer { parameters, result }
            | Type::BorrowedFunctionPointer {
                parameters, result, ..
            }
            | Type::ForeignCallback { parameters, result } => {
                for parameter in parameters {
                    self.rewrite(parameter, depth + 1, true)?;
                }
                self.rewrite(result, depth + 1, true)?;
            }
            Type::Closure(id) => {
                self.pay(self.invalidated.len().saturating_add(1).ilog2() as usize + 1)?;
                self.changed |= self.invalidated.contains(id);
            }
            Type::Dyn { .. }
            | Type::OwnedDyn { .. }
            | Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::F32
            | Type::F64
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::I8
            | Type::I16
            | Type::Isize
            | Type::ByteBuffer
            | Type::ForeignHandle
            | Type::SyncCell(_)
            | Type::TaskI32
            | Type::CWideString
            | Type::Named(_)
            | Type::Unsupported(_) => {}
        }
        Ok(())
    }
    fn apply(&mut self, value: &mut Type) -> R<bool> {
        self.changed = false;
        *value = self.ty(value, 0, Span::default())?;
        Ok(self.changed)
    }
    fn row(&mut self, row: &mut LifeNominalInstance) -> R<()> {
        self.apply(&mut row.ty)?;
        for actual in &mut row.arguments {
            match actual {
                LifeNominalActual::Type(ty) => {
                    self.apply(ty)?;
                    if let Type::BorrowedStorage { life, .. } = &*ty {
                        inference::charge_type_snapshot(
                            ty,
                            Span::default(),
                            self.limits,
                            self.left,
                            1,
                        )?;
                        *actual = LifeNominalActual::QualifiedType {
                            ty: ty.clone(),
                            life: life.clone(),
                        };
                    }
                }
                LifeNominalActual::QualifiedType { ty, life } => {
                    if self.apply(ty)?
                        && let Type::BorrowedStorage { life: formed, .. } = &*ty
                    {
                        inference::charge_type_snapshot(
                            ty,
                            Span::default(),
                            self.limits,
                            self.left,
                            1,
                        )?;
                        *life = formed.clone();
                    }
                }
                LifeNominalActual::Const(_) | LifeNominalActual::Lifetime(_) => {}
            }
        }
        Ok(())
    }
    fn sources(&mut self, rows: &mut [StorageSourceBinding]) -> R<()> {
        for row in rows {
            self.pay(1)?;
            for source in &mut row.sources {
                self.pay(1)?;
                match source {
                    StorageSource::OwnedView { leaf, .. } | StorageSource::Static { leaf, .. } => {
                        self.apply(leaf)?;
                    }
                    StorageSource::Exact(_)
                    | StorageSource::PlaceAddress { .. }
                    | StorageSource::ReborrowedLeaf { .. } => {}
                }
            }
        }
        Ok(())
    }
}
impl TypeResolver for Rewrite<'_> {
    fn charge(&mut self, depth: usize, _: Span) -> R<()> {
        self.pay(1)?;
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "qualified type rewrite depth exceeded",
                None,
            ));
        }
        Ok(())
    }
    fn ty(&mut self, value: &Type, depth: usize, _: Span) -> R<Type> {
        inference::charge_type_snapshot(value, Span::default(), self.limits, self.left, 1)?;
        let mut value = value.clone();
        self.rewrite(&mut value, depth, true)?;
        Ok(value)
    }
    fn receiver_type(&mut self, value: &Type) -> R<Type> {
        self.ty(value, 0, Span::default())
    }
    fn snapshot(&mut self, value: &impl DynRecipeMetadata) -> R<()> {
        crate::dyn_metadata::semantic_operand_snapshot(value, self.limits, self.left, 1)
    }
}
impl crate::semantics::rewrite::Context for Rewrite<'_> {
    fn node(&mut self, depth: usize) -> R<()> {
        self.charge(depth, Span::default())
    }
    fn payload(&mut self, value: &Type, depth: usize) -> R<Type> {
        self.ty(value, depth, Span::default())
    }
}

pub(super) fn canonical_hint<'a>(
    actual: &'a Type,
    proposals: &[(Type, LifeNominalInstance)],
    limits: &Limits,
    left: &mut usize,
) -> R<std::borrow::Cow<'a, Type>> {
    let mut replacements = Vec::new();
    for (before, row) in proposals {
        inference::charge_type_snapshot(before, Span::default(), limits, left, 1)?;
        inference::charge_type_snapshot(&row.ty, Span::default(), limits, left, 1)?;
        if before != &row.ty {
            replacements.push((before.clone(), row.ty.clone()));
        }
    }
    if replacements.is_empty() {
        return Ok(std::borrow::Cow::Borrowed(actual));
    }
    let mut rewrite = Rewrite {
        replacements: &replacements,
        limits,
        left,
        changed: false,
        invalidated: BTreeSet::new(),
    };
    let expected = rewrite.ty(actual, 0, Span::default())?;
    Ok(if rewrite.changed {
        std::borrow::Cow::Owned(expected)
    } else {
        std::borrow::Cow::Borrowed(actual)
    })
}

pub(super) fn apply(
    program: &mut SemanticModule,
    module: &mut TypedModule,
    ledger: &mut Ledger,
    replacements: &[(Type, Type)],
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    if replacements.is_empty() {
        return Ok(());
    }
    let mut cx = Rewrite {
        replacements,
        limits,
        left,
        changed: false,
        invalidated: BTreeSet::new(),
    };
    loop {
        let mut progress = false;
        for closure in &mut module.closures {
            cx.pay(cx.invalidated.len().saturating_add(1).ilog2() as usize + 1)?;
            if cx.invalidated.contains(&closure.id) {
                continue;
            }
            cx.pay(1)?;
            let mut changed = cx.apply(&mut closure.result)?;
            for ty in closure.captures.iter_mut().chain(&mut closure.parameters) {
                changed |= cx.apply(ty)?;
            }
            if changed {
                cx.pay(cx.invalidated.len().saturating_add(1).ilog2() as usize + 1)?;
                cx.invalidated.insert(closure.id);
                closure.storage_life = None;
                progress = true;
            }
        }
        if !progress {
            break;
        }
    }
    let invalidated = std::mem::take(&mut cx.invalidated);
    for function in &mut module.functions {
        cx.pay(1)?;
        cx.pay(invalidated.len().saturating_add(1).ilog2() as usize + 1)?;
        if function
            .environment
            .is_some_and(|id| invalidated.contains(&id))
        {
            function.storage_life = None;
        }
        cx.apply(&mut function.return_type)?;
        for local in &mut function.locals {
            cx.apply(&mut local.ty)?;
        }
        for ty in &mut function.values {
            cx.apply(ty)?;
        }
        if let Some(instance) = &mut function.dyn_instance {
            dynamic::function_instance(instance, &mut cx)?;
        }
        if let Some(sources) = &mut function.retained_return_sources {
            cx.sources(sources)?;
        }
        if let Some(contract) = &mut function.callable_return_contract {
            if let Some(sources) = &mut contract.capture_sources {
                cx.sources(sources)?;
            }
        }
        for block in &mut function.blocks {
            cx.pay(block.instructions.len().saturating_add(1))?;
            for instruction in &mut block.instructions {
                match &mut instruction.kind {
                    InstructionKind::OwnedStoreNew { element: ty }
                    | InstructionKind::ConvertInteger { target: ty, .. }
                    | InstructionKind::ConvertNumber { target: ty, .. } => {
                        cx.apply(ty)?;
                    }
                    _ => {}
                }
            }
        }
    }
    for resource in &mut module.resources {
        cx.apply(&mut resource.payload)?;
    }
    for declaration in &mut module.life_declarations {
        cx.pay(1)?;
        for formal in &mut declaration.parameters {
            cx.pay(1)?;
            if let LifeParameterKind::Const(ty) = formal {
                cx.apply(ty)?;
            }
        }
    }
    for row in &mut module.life_instances {
        cx.row(row)?;
    }
    for declaration in &module.dyn_declarations {
        crate::dyn_metadata::actual_declaration_snapshot(declaration, limits, cx.left, 1)?;
    }
    dynamic::declarations_types(&mut module.dyn_declarations, &mut cx)?;
    for interface in &module.dyn_interfaces {
        crate::dyn_metadata::actual_interface_snapshot(interface, limits, cx.left, 1)?;
    }
    dynamic::interfaces_types(&mut module.dyn_interfaces, &mut cx)?;
    for table in &mut module.dyn_vtables {
        cx.pay(1)?;
        dynamic::table_types(table, &mut cx)?;
    }
    if let Some(semantic) = &mut module.semantic {
        crate::semantics::rewrite::module(semantic, &mut cx)?;
    }
    crate::semantics::rewrite::module(program, &mut cx)?;
    // A pre-formation header cannot retain an old N0 payload after its actual
    // representation changes. Re-query the original creator on the next round.
    for id in &invalidated {
        cx.pay(ledger.formed_headers.len().saturating_add(1).ilog2() as usize + 1)?;
        ledger.formed_headers.remove(id);
    }
    for instance in &mut program.instances {
        cx.pay(1)?;
        cx.pay(invalidated.len().saturating_add(1).ilog2() as usize + 1)?;
        if instance.environment.as_ref().is_some_and(|env| matches!(env.closure, SemanticClosureTarget::Typed(id) if invalidated.contains(&id))) {
            instance.header_embedding.clear();
        }
    }
    for row in ledger.formed_polls.values_mut() {
        cx.row(row)?;
    }
    Ok(())
}
