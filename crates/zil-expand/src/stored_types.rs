//! Source lifetime arguments are retained before reference layout is lowered.
//! This module produces untrusted type/contract data; it grants no loan.
use super::*;

type R<T> = Result<T, Diagnostic>;

/// A lexical lifetime reference, resolved in the actual enclosing declaration.
/// The number is local to that declaration; only its equality partition is
/// copied into an instantiated storage type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LexicalLife(pub(super) u32);

pub(super) struct Instance {
    pub(super) declaration: LifeNominalDeclaration,
    pub(super) instance: LifeNominalInstance,
    /// Instantiated type class -> actual enclosing source lifetime.
    pub(super) classes: Vec<LexicalLife>,
}

#[derive(Default)]
pub(super) struct Catalog {
    pub(super) instances: Vec<Instance>,
    /// Actual specialized function spelling -> retained source relation. These
    /// are output lookup keys, never declaration identities or capabilities.
    pub(super) functions: BTreeMap<String, StorageLifeContract>,
    pub(super) project_functions: Vec<(zil_base::ModuleId, String, StorageLifeContract)>,
    pub(super) pointers: BTreeMap<String, Type>,
    pub(super) borrowed_signatures: bool,
    pub(super) source_helpers: bool,
}

impl Catalog {
    pub(super) fn pointer_lookup_fee(
        &self,
        key: &str,
        budget: &mut generics::Budget<'_>,
        span: Span,
    ) -> R<()> {
        budget.identity_work(
            key.len()
                .saturating_add(1)
                .saturating_mul(
                    self.pointers
                        .len()
                        .checked_ilog2()
                        .map_or(1, |n| n as usize + 2),
                )
                .saturating_mul(12),
            span,
        )
    }
    pub(super) fn pointer_type(
        &self,
        source: &s::TypeRef,
        budget: &mut generics::Budget<'_>,
    ) -> R<Option<Type>> {
        let Some(key) = generics::rich_pointer::key(source, budget)? else {
            generics::rich_pointer::reject_unhandled(source, budget, &self.instances)?;
            return Ok(None);
        };
        self.pointer_lookup_fee(&key, budget, source.span)?;
        let ty = self.pointers.get(&key).ok_or_else(|| {
            Diagnostic::unsupported(
                Phase::Expand,
                "rich code has no actual prepared source signature",
                Some(source.span),
            )
        })?;
        pay_type(ty, budget, source.span, 0)?;
        Ok(Some(ty.clone()))
    }
    pub(super) fn pointer_parameter_type(
        &self,
        source: &s::TypeRef,
        nominals: &BTreeMap<String, &s::Item>,
        budget: &mut generics::Budget<'_>,
        depth: usize,
    ) -> R<Type> {
        budget.node(source.span, depth)?;
        match &source.kind {
            s::TypeKind::Reference { mutable, inner, .. } => Ok(Type::Address {
                mutable: *mutable,
                inner: Box::new(self.pointer_parameter_type(inner, nominals, budget, depth + 1)?),
            }),
            s::TypeKind::Group(inner) => {
                self.pointer_parameter_type(inner, nominals, budget, depth + 1)
            }
            s::TypeKind::Tuple(fields) => {
                let mut result = Vec::new();
                for field in fields {
                    result.push(self.pointer_parameter_type(field, nominals, budget, depth + 1)?);
                }
                Ok(Type::Tuple(result))
            }
            s::TypeKind::Named { path, args } if path.len() == 1 && args.is_empty() => {
                for instance in &self.instances {
                    budget.node(source.span, depth)?;
                    if let Type::BorrowedStorage { storage, .. } = &instance.instance.ty
                        && let Type::Record { name, .. }
                        | Type::Package { name, .. }
                        | Type::Enum { name, .. } = storage.as_ref()
                    {
                        budget.text(name, source.span)?;
                        budget.text(&path[0], source.span)?;
                        if name == &path[0] {
                            pay_type(&instance.instance.ty, budget, source.span, depth + 1)?;
                            return Ok(instance.instance.ty.clone());
                        }
                    }
                }
                budget.ty_depth(source, depth)?;
                let ty = type_ref(source);
                if let Type::Named(name) = &ty {
                    budget.identity_work(
                        name.len()
                            .saturating_add(1)
                            .saturating_mul(
                                nominals.len().checked_ilog2().map_or(1, |n| n as usize + 2),
                            )
                            .saturating_mul(12),
                        source.span,
                    )?;
                    let declaration = nominals.get(name).ok_or_else(|| {
                        invalid(source.span, "unknown nominal type in rich code signature")
                    })?;
                    let formals = match &declaration.kind {
                        s::ItemKind::Aggregate(a) => a.generics.as_slice(),
                        s::ItemKind::Enum(e) => e.generics.as_slice(),
                        _ => {
                            return Err(invalid(
                                source.span,
                                "rich code type needs an actual nominal declaration",
                            ));
                        }
                    };
                    budget.identity_work(formals.len(), source.span)?;
                    if !formals.is_empty() {
                        return Err(invalid(
                            source.span,
                            "rich code nominal type lacks its actual arguments",
                        ));
                    }
                }
                if contains_unresolved(&ty) {
                    return Err(Diagnostic::unsupported(
                        Phase::Expand,
                        "rich code parameter needs its complete source type",
                        Some(source.span),
                    ));
                }
                Ok(ty)
            }
            _ => Err(Diagnostic::unsupported(
                Phase::Expand,
                "rich code parameter needs its complete source constructor",
                Some(source.span),
            )),
        }
    }
    pub(super) fn restore_functions(
        &mut self,
        functions: &BTreeMap<String, (zil_base::ModuleId, String, bool)>,
        budget: &mut generics::Budget<'_>,
    ) -> R<()> {
        for (canonical, contract) in std::mem::take(&mut self.functions) {
            let span = Span::default();
            spend(
                budget,
                span,
                functions
                    .len()
                    .saturating_add(1)
                    .saturating_mul(canonical.len().saturating_add(1)),
            )?;
            let (module, restored, _) = functions
                .get(&canonical)
                .ok_or_else(|| invalid(span, "stored function lost actual project declaration"))?;
            budget.text(restored, span)?;
            budget.node(span, 0)?;
            self.project_functions
                .push((*module, restored.clone(), contract));
        }
        Ok(())
    }

    pub(super) fn publish(
        &self,
        core: &mut CoreModule,
        budget: &mut generics::Budget<'_>,
    ) -> R<()> {
        if self.instances.is_empty()
            && self.functions.is_empty()
            && self.project_functions.is_empty()
        {
            return Ok(());
        }
        for produced in &self.instances {
            let Type::BorrowedStorage { storage, .. } = &produced.instance.ty else {
                return Err(invalid(
                    Span::default(),
                    "stored producer lost qualified instance",
                ));
            };
            let owner_span = match storage.as_ref() {
                Type::Record { name, fields } | Type::Package { name, fields } => {
                    for aggregate in &core.aggregates {
                        budget.text(&aggregate.name, aggregate.span)?;
                        budget.text(name, aggregate.span)?;
                    }
                    let Some(aggregate) = core
                        .aggregates
                        .iter_mut()
                        .find(|aggregate| aggregate.name == *name)
                    else {
                        continue;
                    };
                    spend(
                        budget,
                        aggregate.span,
                        fields.len().saturating_add(aggregate.fields.len()),
                    )?;
                    if aggregate.fields.len() != fields.len() {
                        return Err(invalid(
                            aggregate.span,
                            "stored instance field count changed during lowering",
                        ));
                    }
                    for (field, (name, ty)) in aggregate.fields.iter_mut().zip(fields) {
                        budget.text(&field.name, field.span)?;
                        budget.text(name, field.span)?;
                        if field.name != *name {
                            return Err(invalid(
                                field.span,
                                "stored instance field order changed during lowering",
                            ));
                        }
                        pay_type(ty, budget, field.span, 0)?;
                        field.ty = ty.clone();
                    }
                    aggregate.span
                }
                Type::Enum { name, variants } => {
                    for item in &core.enums {
                        budget.text(&item.name, item.span)?;
                        budget.text(name, item.span)?;
                    }
                    let Some(item) = core.enums.iter_mut().find(|item| item.name == *name) else {
                        continue;
                    };
                    spend(
                        budget,
                        item.span,
                        variants.len().saturating_add(item.variants.len()),
                    )?;
                    if variants.len() != item.variants.len() {
                        return Err(invalid(
                            item.span,
                            "stored enum variant count changed during lowering",
                        ));
                    }
                    for ((name, fields), (physical_name, physical_fields)) in
                        variants.iter().zip(&mut item.variants)
                    {
                        budget.text(name, item.span)?;
                        budget.text(physical_name, item.span)?;
                        if name != physical_name || fields.len() != physical_fields.len() {
                            return Err(invalid(
                                item.span,
                                "stored enum variant or payload order changed during lowering",
                            ));
                        }
                        for (field, output) in fields.iter().zip(physical_fields.iter_mut()) {
                            pay_type(field, budget, item.span, 0)?;
                            *output = field.clone();
                        }
                    }
                    item.span
                }
                _ => {
                    return Err(invalid(
                        Span::default(),
                        "stored producer lost nominal physical layout",
                    ));
                }
            };
            pay_instance(produced, budget, owner_span)?;
            spend(budget, owner_span, core.life_declarations.len())?;
            if !core
                .life_declarations
                .iter()
                .any(|prior| prior.id == produced.declaration.id)
            {
                core.life_declarations.push(produced.declaration.clone());
            }
            core.life_instances.push(produced.instance.clone());
        }
        for function in &mut core.functions {
            for name in self.functions.keys() {
                budget.text(name, function.span)?;
                budget.text(&function.name, function.span)?;
            }
            for (_, name, _) in &self.project_functions {
                budget.node(function.span, 0)?;
                budget.text(name, function.span)?;
                budget.text(&function.name, function.span)?;
            }
            let contract = if self.project_functions.is_empty() {
                self.functions.get(&function.name)
            } else {
                self.project_functions
                    .iter()
                    .find_map(|(module, name, contract)| {
                        (*module == core.module_id && *name == function.name).then_some(contract)
                    })
            };
            if let Some(contract) = contract {
                for binding in &contract.bindings {
                    spend(
                        budget,
                        function.span,
                        binding.position.path.len().saturating_add(1),
                    )?;
                }
                for class in &contract.classes {
                    spend(
                        budget,
                        function.span,
                        class.position.path.len().saturating_add(1),
                    )?;
                }
                function.life = None;
                function.storage_life = Some(contract.clone());
            }
        }
        Ok(())
    }
}

fn pay_instance(instance: &Instance, budget: &mut generics::Budget<'_>, span: Span) -> R<()> {
    let identity = &instance.declaration.identity;
    budget.text(&identity.package, span)?;
    for part in identity
        .module_path
        .iter()
        .chain(&identity.declaration_path)
    {
        budget.text(part, span)?;
        budget.node(span, 0)?;
    }
    spend(
        budget,
        span,
        identity
            .generation_path
            .len()
            .saturating_add(instance.declaration.parameters.len()),
    )?;
    for parameter in &instance.declaration.parameters {
        if let LifeParameterKind::Const(ty) = parameter {
            pay_type(ty, budget, span, 0)?;
        }
    }
    for actual in &instance.instance.arguments {
        budget.node(span, 0)?;
        if let LifeNominalActual::Type(ty) = actual {
            pay_type(ty, budget, span, 0)?;
        }
    }
    pay_type(&instance.instance.ty, budget, span, 0)
}

pub(super) fn pay_type(
    ty: &Type,
    budget: &mut generics::Budget<'_>,
    span: Span,
    depth: usize,
) -> R<()> {
    budget.node(span, depth)?;
    match ty {
        Type::BorrowedStorage { storage, life } => {
            for slot in &life.slots {
                spend(budget, span, slot.path.len().saturating_add(1))?;
            }
            for row in &life.retained {
                spend(budget, span, row.path.len().saturating_add(1))?;
            }
            for row in &life.nominal_arguments {
                spend(budget, span, row.path.len().saturating_add(1))?;
            }
            pay_type(storage, budget, span, depth + 1)
        }
        Type::Named(name) | Type::Unsupported(name) => budget.text(name, span),
        Type::Record { name, fields } | Type::Package { name, fields } => {
            budget.text(name, span)?;
            for (name, ty) in fields {
                budget.text(name, span)?;
                pay_type(ty, budget, span, depth + 1)?;
            }
            Ok(())
        }
        Type::Enum { name, variants } => {
            budget.text(name, span)?;
            for (variant, fields) in variants {
                budget.text(variant, span)?;
                for field in fields {
                    pay_type(field, budget, span, depth + 1)?;
                }
            }
            Ok(())
        }
        Type::Tuple(fields) => {
            for ty in fields {
                pay_type(ty, budget, span, depth + 1)?;
            }
            Ok(())
        }
        Type::Address { inner, .. } | Type::Desc(inner) | Type::Managed(inner) => {
            pay_type(inner, budget, span, depth + 1)
        }
        Type::Array { element, .. } | Type::Slice { element } => {
            pay_type(element, budget, span, depth + 1)
        }
        Type::ErrorUnion { error, result } => {
            pay_type(error, budget, span, depth + 1)?;
            pay_type(result, budget, span, depth + 1)
        }
        Type::FunctionPointer { parameters, result }
        | Type::BorrowedFunctionPointer {
            parameters, result, ..
        } => {
            if let Type::BorrowedFunctionPointer { life, .. } = ty {
                for row in &life.bindings {
                    spend(budget, span, row.position.path.len().saturating_add(1))?;
                }
                for row in &life.classes {
                    spend(budget, span, row.position.path.len().saturating_add(1))?;
                }
            }
            for parameter in parameters {
                pay_type(parameter, budget, span, depth + 1)?;
            }
            pay_type(result, budget, span, depth + 1)
        }
        Type::Unit
        | Type::Bool
        | Type::U8
        | Type::U16
        | Type::U32
        | Type::U64
        | Type::Usize
        | Type::I32
        | Type::I8
        | Type::I16
        | Type::Isize
        | Type::I64
        | Type::F32
        | Type::F64 => Ok(()),
        _ => Err(Diagnostic::unsupported(
            Phase::Expand,
            "stored type clone needs its complete semantic traversal",
            Some(span),
        )),
    }
}

pub(super) struct Occurrence {
    pub(super) position: OccurrencePosition,
    pub(super) life: LexicalLife,
    /// True only for a class of an actual qualified INPUT carrier. A result
    /// occurrence cannot introduce a fixed upper or revive a dead target.
    pub(super) fixed_input: bool,
}

pub(super) enum OccurrencePosition {
    Borrow(StoragePosition),
    Class(SignatureLifeClass),
}
impl OccurrencePosition {
    fn base(&self) -> StorageBase {
        match self {
            Self::Borrow(position) => position.base,
            Self::Class(position) => position.base,
        }
    }
    fn path_len(&self) -> usize {
        match self {
            Self::Borrow(position) => position.path.len(),
            Self::Class(position) => position.path.len(),
        }
    }
}

/// Form the complete retained relation from source occurrences before type
/// labels disappear. The same lexical name has one class across all positions;
/// a pre-existing input class remains Fixed even on an ordinary result address.
pub(super) fn contract(
    occurrences: Vec<Occurrence>,
    budget: &mut generics::Budget<'_>,
    span: Span,
) -> R<StorageLifeContract> {
    contract_with_sources(occurrences, Vec::new(), budget, span).map(|(contract, _, _)| contract)
}

/// Retain the inverse of the actual occurrence-to-binder construction. This
/// is producer provenance, never reconstructed from a generated type spelling.
pub(super) fn contract_with_sources(
    occurrences: Vec<Occurrence>,
    mut grounded: Vec<LexicalLife>,
    budget: &mut generics::Budget<'_>,
    span: Span,
) -> R<(StorageLifeContract, Vec<LexicalLife>, Vec<LexicalLife>)> {
    for occurrence in &occurrences {
        spend(
            budget,
            span,
            occurrence.position.path_len().saturating_add(1),
        )?;
        if occurrence.fixed_input {
            if !matches!(
                occurrence.position.base(),
                StorageBase::Parameter(_) | StorageBase::Capture(_)
            ) {
                return Err(invalid(span, "a result cannot introduce a fixed lifetime"));
            }
            spend(budget, span, grounded.len().saturating_add(1))?;
            if !grounded.contains(&occurrence.life) {
                grounded.push(occurrence.life);
            }
        }
    }
    let mut fixed = Vec::new();
    let mut invocation = Vec::new();
    let mut bindings: Vec<StorageLifeBinding> = Vec::new();
    let mut classes: Vec<SignatureLifeClassBinding> = Vec::new();
    for occurrence in occurrences {
        // Every comparison includes all actual path segments, before storing
        // any relation row. An equal physical position cannot carry two labels.
        match &occurrence.position {
            OccurrencePosition::Borrow(position) => {
                for prior in &bindings {
                    spend(
                        budget,
                        span,
                        prior
                            .position
                            .path
                            .len()
                            .saturating_add(position.path.len())
                            .saturating_add(1),
                    )?;
                    if prior.position == *position {
                        return Err(invalid(span, "duplicate stored-lifetime borrow position"));
                    }
                }
            }
            OccurrencePosition::Class(position) => {
                for prior in &classes {
                    spend(
                        budget,
                        span,
                        prior
                            .position
                            .path
                            .len()
                            .saturating_add(position.path.len())
                            .saturating_add(1),
                    )?;
                    if prior.position == *position {
                        return Err(invalid(span, "duplicate signature class position"));
                    }
                }
            }
        }
        spend(
            budget,
            span,
            grounded
                .len()
                .saturating_add(fixed.len())
                .saturating_add(invocation.len())
                .saturating_add(1),
        )?;
        let binder = if grounded.contains(&occurrence.life) {
            let index = fixed
                .iter()
                .position(|life| *life == occurrence.life)
                .unwrap_or_else(|| {
                    fixed.push(occurrence.life);
                    fixed.len() - 1
                });
            StorageBinder::Fixed(
                u32::try_from(index).map_err(|_| invalid(span, "fixed binder overflow"))?,
            )
        } else {
            let index = invocation
                .iter()
                .position(|life| *life == occurrence.life)
                .unwrap_or_else(|| {
                    invocation.push(occurrence.life);
                    invocation.len() - 1
                });
            StorageBinder::Invocation(
                u32::try_from(index).map_err(|_| invalid(span, "invocation binder overflow"))?,
            )
        };
        match occurrence.position {
            OccurrencePosition::Borrow(position) => {
                bindings.push(StorageLifeBinding { position, binder })
            }
            OccurrencePosition::Class(position) => {
                classes.push(SignatureLifeClassBinding { position, binder })
            }
        }
    }
    Ok((
        StorageLifeContract {
            static_bindings: Vec::new(),
            fixed_binder_count: u32::try_from(fixed.len())
                .map_err(|_| invalid(span, "fixed binder overflow"))?,
            invocation_binder_count: u32::try_from(invocation.len())
                .map_err(|_| invalid(span, "invocation binder overflow"))?,
            bindings,
            classes,
        },
        fixed,
        invocation,
    ))
}

fn invalid(span: Span, message: &str) -> Diagnostic {
    Diagnostic::error(Phase::Expand, "E-STORED-LIFE", Some(span), message)
}

fn spend(budget: &mut generics::Budget<'_>, span: Span, count: usize) -> R<()> {
    for _ in 0..count {
        budget.node(span, 0)?;
    }
    Ok(())
}

pub(super) struct QualifiedActual {
    pub(super) physical: s::TypeRef,
    pub(super) ty: Type,
    pub(super) classes: Vec<LexicalLife>,
}

struct Shape<'a, 'l> {
    budget: &'a mut generics::Budget<'l>,
    formals: &'a [(String, LexicalLife)],
    classes: Vec<LexicalLife>,
    slots: Vec<StorageLifeSlot>,
    retained: Vec<StorageRetainedBinding>,
    qualified: &'a BTreeMap<usize, QualifiedActual>,
    original: Option<&'a s::Aggregate>,
    enum_formals: Option<&'a [s::GenericParam]>,
    direct: &'a BTreeMap<Vec<LifeTypeProjection>, QualifiedActual>,
}

impl Shape<'_, '_> {
    fn address_argument(
        &mut self,
        actual: &QualifiedActual,
        ordinal: usize,
        span: Span,
    ) -> R<bool> {
        if !matches!(actual.ty, Type::Address { .. }) {
            return Ok(false);
        }
        let [lexical] = actual.classes.as_slice() else {
            return Err(invalid(
                span,
                "address Type actual requires its one source class",
            ));
        };
        let binder = self.class(*lexical, span)?;
        spend(self.budget, span, 2)?;
        self.retained.push(StorageRetainedBinding {
            path: vec![LifeTypeProjection::TypeArgument(
                u32::try_from(ordinal)
                    .map_err(|_| invalid(span, "Type actual ordinal overflow"))?,
            )],
            class: 0,
            binder,
        });
        Ok(true)
    }

    fn qualified_child(
        &mut self,
        actual: &QualifiedActual,
        path: &[PlaceProjection],
        type_path: &[LifeTypeProjection],
        span: Span,
        depth: usize,
    ) -> R<Type> {
        if let Type::Address { mutable, .. } = &actual.ty {
            let [lexical] = actual.classes.as_slice() else {
                return Err(invalid(
                    span,
                    "address Type actual requires its one source class",
                ));
            };
            let binder = self.class(*lexical, span)?;
            spend(self.budget, span, path.len().saturating_add(1))?;
            self.slots.push(StorageLifeSlot {
                path: path.to_vec(),
                binder,
                mode: if *mutable {
                    StoredBorrowMode::ExclusiveAddress
                } else {
                    StoredBorrowMode::SharedAddress
                },
            });
            pay_type(&actual.ty, self.budget, span, depth + 1)?;
            return Ok(actual.ty.clone());
        }
        let Type::BorrowedStorage { life, .. } = &actual.ty else {
            return Err(invalid(span, "qualified child lacks storage relation"));
        };
        if life.binder_count as usize != actual.classes.len() {
            return Err(invalid(span, "qualified child class vector incomplete"));
        }
        if type_path.contains(&LifeTypeProjection::Element) && !life.slots.is_empty() {
            return Err(Diagnostic::unsupported(
                Phase::Expand,
                "qualified array child with physical borrow slots needs element place projection",
                Some(span),
            ));
        }
        for slot in &life.slots {
            let lexical = *actual
                .classes
                .get(slot.binder as usize)
                .ok_or_else(|| invalid(span, "qualified child slot class absent"))?;
            let binder = self.class(lexical, span)?;
            spend(
                self.budget,
                span,
                path.len().saturating_add(slot.path.len()).saturating_add(1),
            )?;
            let mut physical_path = path.to_vec();
            physical_path.extend_from_slice(&slot.path);
            self.slots.push(StorageLifeSlot {
                path: physical_path,
                binder,
                mode: slot.mode,
            });
        }
        for (class, lexical) in actual.classes.iter().enumerate() {
            let binder = self.class(*lexical, span)?;
            spend(self.budget, span, type_path.len().saturating_add(1))?;
            self.retained.push(StorageRetainedBinding {
                path: type_path.to_vec(),
                class: u32::try_from(class)
                    .map_err(|_| invalid(span, "qualified child class overflow"))?,
                binder,
            });
        }
        pay_type(&actual.ty, self.budget, span, depth + 1)?;
        Ok(actual.ty.clone())
    }

    fn class(&mut self, actual: LexicalLife, span: Span) -> R<u32> {
        // The complete scan and the eventual new row are paid before either
        // comparison or allocation. No global cache gives a cheaper request.
        spend(self.budget, span, self.classes.len().saturating_add(1))?;
        if let Some(index) = self.classes.iter().position(|value| *value == actual) {
            return u32::try_from(index).map_err(|_| invalid(span, "life class overflow"));
        }
        let index =
            u32::try_from(self.classes.len()).map_err(|_| invalid(span, "life class overflow"))?;
        self.classes.push(actual);
        Ok(index)
    }

    fn actual(&mut self, name: &str, span: Span) -> R<LexicalLife> {
        for (formal, _) in self.formals {
            self.budget.text(formal, span)?;
            self.budget.text(name, span)?;
        }
        self.formals
            .iter()
            .find(|(formal, _)| formal == name)
            .map(|(_, actual)| *actual)
            .ok_or_else(|| invalid(span, "stored reference uses an undeclared lifetime"))
    }

    fn field(
        &mut self,
        source: &s::TypeRef,
        original: Option<&s::TypeRef>,
        path: &mut Vec<PlaceProjection>,
        type_path: &mut Vec<LifeTypeProjection>,
        depth: usize,
    ) -> R<Type> {
        self.budget.node(source.span, depth)?;
        if let Some(actual) = self.direct.get(type_path) {
            let mut physical = source;
            while let s::TypeKind::Group(inner) = &physical.kind {
                self.budget.node(physical.span, depth)?;
                physical = inner;
            }
            self.budget.ty(physical)?;
            self.budget.ty(&actual.physical)?;
            if type_ref(physical) != type_ref(&actual.physical) {
                return Err(invalid(
                    source.span,
                    "direct physical child differs from original scoped application",
                ));
            }
            return self.qualified_child(actual, path, type_path, source.span, depth);
        }
        // Resolve the original formal occurrence before looking at the physical
        // specialization. Two source Type formals may have identical layouts
        // while carrying distinct caller classes.
        let formals = self
            .original
            .map(|aggregate| aggregate.generics.as_slice())
            .or(self.enum_formals);
        let selected = if let (Some(formals), Some(original)) = (formals, original) {
            let mut original = original;
            while let s::TypeKind::Group(inner) = &original.kind {
                self.budget.node(original.span, depth)?;
                original = inner;
            }
            if let s::TypeKind::Named { path, args } = &original.kind
                && path.len() == 1
                && args.is_empty()
            {
                let mut ordinal = None;
                for (index, formal) in formals.iter().enumerate() {
                    self.budget.text(&formal.name, source.span)?;
                    self.budget.text(&path[0], source.span)?;
                    if formal.name == path[0] {
                        ordinal = Some(index);
                    }
                }
                ordinal.and_then(|index| self.qualified.get(&index))
            } else {
                None
            }
        } else {
            None
        };
        if let Some(actual) = selected {
            let mut physical = source;
            while let s::TypeKind::Group(inner) = &physical.kind {
                self.budget.node(physical.span, depth)?;
                physical = inner;
            }
            self.budget.ty(physical)?;
            self.budget.ty(&actual.physical)?;
            let matches_actual = type_ref(physical) == type_ref(&actual.physical);
            if !matches_actual {
                return Err(invalid(
                    source.span,
                    "physical field differs from its original Type formal substitution",
                ));
            }
            return self.qualified_child(actual, path, type_path, source.span, depth);
        }
        match &source.kind {
            s::TypeKind::Group(inner) => self.field(
                inner,
                original.map(|ty| {
                    if let s::TypeKind::Group(inner) = &ty.kind {
                        inner.as_ref()
                    } else {
                        ty
                    }
                }),
                path,
                type_path,
                depth + 1,
            ),
            s::TypeKind::Reference {
                lifetime,
                mutable,
                inner,
            } => {
                if type_path.contains(&LifeTypeProjection::Element) {
                    return Err(Diagnostic::unsupported(
                        Phase::Expand,
                        "array element physical borrow needs an actual element place projection",
                        Some(source.span),
                    ));
                }
                let name = lifetime.as_deref().ok_or_else(|| {
                    invalid(
                        source.span,
                        "stored reference requires its declared lifetime",
                    )
                })?;
                let actual = self.actual(name, source.span)?;
                let binder = self.class(actual, source.span)?;
                // The pointee is a semantic type, not inline storage. A second
                // borrowed carrier must supply its own scoped relation before
                // it can be embedded here; do not invent a physical projection.
                self.budget.ty_depth(inner, depth + 1)?;
                let inner = type_ref(inner);
                if contains_unresolved(&inner) {
                    return Err(Diagnostic::unsupported(
                        Phase::Expand,
                        "stored reference pointee needs a resolved scoped type",
                        Some(source.span),
                    ));
                }
                spend(self.budget, source.span, path.len().saturating_add(1))?;
                self.slots.push(StorageLifeSlot {
                    path: path.clone(),
                    binder,
                    mode: if *mutable {
                        StoredBorrowMode::ExclusiveAddress
                    } else {
                        StoredBorrowMode::SharedAddress
                    },
                });
                Ok(Type::Address {
                    mutable: *mutable,
                    inner: Box::new(inner),
                })
            }
            s::TypeKind::Tuple(fields) if !fields.is_empty() => {
                let mut types = Vec::new();
                for (index, field) in fields.iter().enumerate() {
                    let index = u32::try_from(index)
                        .map_err(|_| invalid(field.span, "field ordinal overflow"))?;
                    self.budget.node(field.span, depth)?;
                    path.push(PlaceProjection::Field(index));
                    type_path.push(LifeTypeProjection::Place(PlaceProjection::Field(index)));
                    let source_field = if let Some(mut original) = original {
                        let mut nesting = depth;
                        while let s::TypeKind::Group(inner) = &original.kind {
                            self.budget.node(original.span, nesting)?;
                            nesting = nesting.saturating_add(1);
                            original = inner;
                        }
                        if let s::TypeKind::Tuple(fields) = &original.kind {
                            fields.get(index as usize)
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    let ty = self.field(field, source_field, path, type_path, depth + 1)?;
                    path.pop();
                    type_path.pop();
                    types.push(ty);
                }
                Ok(Type::Tuple(types))
            }
            s::TypeKind::Array {
                element,
                length: Some(length),
            } => {
                let mut original_element = None;
                if let Some(original) = original
                    && let s::TypeKind::Array { element, .. } = &original.kind
                {
                    original_element = Some(element.as_ref());
                }
                if matches!(element.kind, s::TypeKind::Reference { mutable: false, .. }) {
                    self.budget.const_expression(length)?;
                    let s::ExprKind::Literal(s::Literal::Integer(text)) = &length.kind else {
                        return Err(invalid(
                            source.span,
                            "stored array length needs normalized constant",
                        ));
                    };
                    let count = text
                        .replace('_', "")
                        .parse::<u32>()
                        .map_err(|_| invalid(source.span, "stored array length invalid"))?;
                    if count == 0 {
                        return Err(Diagnostic::unsupported(
                            Phase::Expand,
                            "physical borrowed arrays require a nonzero fixed length",
                            Some(source.span),
                        ));
                    }
                    spend(
                        self.budget,
                        source.span,
                        (count as usize).saturating_mul(
                            path.len().saturating_add(type_path.len()).saturating_add(4),
                        ),
                    )?;
                    let mut actual = None;
                    for index in 0..count {
                        path.push(PlaceProjection::Field(index));
                        type_path.push(LifeTypeProjection::Place(PlaceProjection::Field(index)));
                        let child =
                            self.field(element, original_element, path, type_path, depth + 1)?;
                        path.pop();
                        type_path.pop();
                        if actual.is_none() {
                            actual = Some(child);
                        }
                    }
                    return Ok(Type::Array {
                        element: Box::new(actual.expect("nonzero array")),
                        length: count,
                    });
                }
                type_path.push(LifeTypeProjection::Element);
                let child = self.field(element, original_element, path, type_path, depth + 1)?;
                type_path.pop();
                self.budget.const_expression(length)?;
                let s::ExprKind::Literal(s::Literal::Integer(text)) = &length.kind else {
                    return Err(invalid(
                        source.span,
                        "stored array length needs normalized constant",
                    ));
                };
                let length = text
                    .replace('_', "")
                    .parse::<u32>()
                    .map_err(|_| invalid(source.span, "stored array length invalid"))?;
                Ok(Type::Array {
                    element: Box::new(child),
                    length,
                })
            }
            _ => {
                self.budget.ty_depth(source, depth)?;
                let ty = type_ref(source);
                if contains_unresolved(&ty) {
                    return Err(Diagnostic::unsupported(
                        Phase::Expand,
                        "stored field needs its complete instantiated type",
                        Some(source.span),
                    ));
                }
                Ok(ty)
            }
        }
    }
}

fn contains_unresolved(ty: &Type) -> bool {
    match ty {
        Type::Unsupported(_) => true,
        Type::BorrowedStorage { storage, .. } => contains_unresolved(storage),
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            fields.iter().any(|(_, field)| contains_unresolved(field))
        }
        Type::Enum { variants, .. } => variants
            .iter()
            .any(|(_, fields)| fields.iter().any(contains_unresolved)),
        Type::Address { inner, .. } | Type::Desc(inner) | Type::Managed(inner) => {
            contains_unresolved(inner)
        }
        Type::Tuple(fields) => fields.iter().any(contains_unresolved),
        Type::Array { element, .. } | Type::Slice { element } => contains_unresolved(element),
        Type::ErrorUnion { error, result } => {
            contains_unresolved(error) || contains_unresolved(result)
        }
        _ => false,
    }
}

/// Anonymous compound values retain their real physical constructor and a
/// complete joint domain; no synthetic nominal declaration stands in for it.
pub(super) fn compound_actual(
    physical: &s::TypeRef,
    direct: &BTreeMap<Vec<LifeTypeProjection>, QualifiedActual>,
    budget: &mut generics::Budget<'_>,
) -> R<QualifiedActual> {
    let empty = BTreeMap::new();
    let mut shape = Shape {
        budget,
        formals: &[],
        classes: vec![],
        slots: vec![],
        retained: vec![],
        qualified: &empty,
        original: None,
        enum_formals: None,
        direct,
    };
    let storage = shape.field(physical, None, &mut vec![], &mut vec![], 0)?;
    let mut nominal_arguments = Vec::new();
    for (prefix, actual) in direct {
        if let Type::BorrowedStorage { life, .. } = &actual.ty {
            for row in &life.nominal_arguments {
                let lexical = *actual
                    .classes
                    .get(row.binder as usize)
                    .ok_or_else(|| invalid(physical.span, "compound nominal class absent"))?;
                let binder = shape.class(lexical, physical.span)?;
                spend(
                    shape.budget,
                    physical.span,
                    prefix
                        .len()
                        .saturating_add(row.path.len())
                        .saturating_add(1),
                )?;
                let mut path = prefix.clone();
                path.extend_from_slice(&row.path);
                nominal_arguments.push(NominalLifeArgument {
                    path,
                    declaration: row.declaration,
                    argument: row.argument,
                    binder,
                });
            }
        }
    }
    shape.budget.ty(physical)?;
    Ok(QualifiedActual {
        physical: physical.clone(),
        ty: Type::BorrowedStorage {
            storage: Box::new(storage),
            life: StorageLifeShape {
                binder_count: u32::try_from(shape.classes.len())
                    .map_err(|_| invalid(physical.span, "compound classes overflow"))?,
                slots: shape.slots,
                retained: shape.retained,
                nominal_arguments,
            },
        },
        classes: shape.classes,
    })
}

/// `aggregate` is the actual substituted declaration and `identity` comes from
/// the original SourceIndex/Origins chain. Names are used only for lookup and
/// storage spelling, never parsed to reconstruct declaration identity.
// The full actual vector, original fields, qualified children and direct
// projection map are independent evidence for one stored instance.
#[allow(clippy::too_many_arguments)]
pub(super) fn instantiate_joint_with_fields(
    aggregate: &s::Aggregate,
    declaration: LifeNominalDeclaration,
    arguments: Vec<LifeNominalActual>,
    lifetime_arguments: &[(String, LexicalLife)],
    joint: Option<(&s::Aggregate, &BTreeMap<usize, QualifiedActual>)>,
    direct: &BTreeMap<Vec<LifeTypeProjection>, QualifiedActual>,
    budget: &mut generics::Budget<'_>,
    span: Span,
) -> R<Instance> {
    if !matches!(
        (aggregate.kind, declaration.kind),
        (s::AggregateKind::Struct, LifeNominalKind::Record)
            | (s::AggregateKind::Package, LifeNominalKind::Package)
    ) {
        return Err(Diagnostic::unsupported(
            Phase::Expand,
            "stored nominal ownership shape requires its consumer",
            Some(span),
        ));
    }
    spend(
        budget,
        span,
        declaration.parameters.len().saturating_add(arguments.len()),
    )?;
    if declaration.parameters.len() != arguments.len() {
        return Err(invalid(
            span,
            "stored nominal argument arity differs from declaration",
        ));
    }
    let mut life_count = 0usize;
    for (parameter, argument) in declaration.parameters.iter().zip(&arguments) {
        match (parameter, argument) {
            (LifeParameterKind::Lifetime, LifeNominalActual::Lifetime(_)) => life_count += 1,
            (LifeParameterKind::Type, LifeNominalActual::Type(_)) => {}
            (LifeParameterKind::Const(domain), LifeNominalActual::Const(value))
                if *domain == value.ty() => {}
            _ => {
                return Err(invalid(
                    span,
                    "stored nominal argument kind differs from declaration",
                ));
            }
        }
    }
    if life_count != lifetime_arguments.len() {
        return Err(invalid(
            span,
            "stored nominal lexical lifetime mapping is incomplete",
        ));
    }
    let empty = BTreeMap::new();
    let (original, qualified) = match joint {
        Some((original, qualified)) => (Some(original), qualified),
        None => (None, &empty),
    };
    let mut shape = Shape {
        budget,
        formals: lifetime_arguments,
        classes: vec![],
        slots: vec![],
        retained: vec![],
        qualified,
        original,
        enum_formals: None,
        direct,
    };
    for (index, (name, _)) in lifetime_arguments.iter().enumerate() {
        for (prior, _) in &lifetime_arguments[..index] {
            shape.budget.text(name, span)?;
            shape.budget.text(prior, span)?;
            if name == prior {
                return Err(invalid(span, "duplicate lifetime formal in stored nominal"));
            }
        }
    }
    // Canonical traversal starts at the full ordered nominal actuals, including
    // phantom classes, and only then visits physical leaves. Field order cannot
    // accidentally rename nominal lifetime positions.
    for (_, actual) in lifetime_arguments {
        shape.class(*actual, span)?;
    }
    // The outer nominal's direct life actuals precede every Type argument.
    // Nested rows retain their real declaration/ordinal and full TypeArgument path.
    let mut nested_nominal_arguments = Vec::new();
    for (ordinal, actual) in qualified {
        let Some(LifeNominalActual::Type(ty)) = arguments.get(*ordinal) else {
            return Err(invalid(
                span,
                "qualified Type actual ordinal is not a Type parameter",
            ));
        };
        pay_type(ty, shape.budget, span, 0)?;
        if ty != &actual.ty {
            return Err(invalid(span, "qualified Type actual metadata differs"));
        }
        if shape.address_argument(actual, *ordinal, span)? {
            continue;
        }
        let Type::BorrowedStorage { life, .. } = ty else {
            return Err(invalid(
                span,
                "qualified actual lacks its complete storage shape",
            ));
        };
        if life.binder_count as usize != actual.classes.len() {
            return Err(invalid(
                span,
                "qualified actual source classes are incomplete",
            ));
        }
        for row in &life.nominal_arguments {
            let lexical = *actual
                .classes
                .get(row.binder as usize)
                .ok_or_else(|| invalid(span, "qualified actual class is absent"))?;
            let binder = shape.class(lexical, span)?;
            spend(shape.budget, span, row.path.len().saturating_add(2))?;
            let mut path = Vec::with_capacity(row.path.len() + 1);
            path.push(LifeTypeProjection::TypeArgument(
                u32::try_from(*ordinal)
                    .map_err(|_| invalid(span, "Type actual ordinal overflow"))?,
            ));
            path.extend_from_slice(&row.path);
            nested_nominal_arguments.push(NominalLifeArgument {
                path,
                declaration: row.declaration,
                argument: row.argument,
                binder,
            });
        }
    }
    let mut fields = Vec::new();
    for (index, field) in aggregate.fields.iter().enumerate() {
        shape.budget.text(&field.name, field.span)?;
        let index =
            u32::try_from(index).map_err(|_| invalid(field.span, "field ordinal overflow"))?;
        let original_field = original
            .and_then(|aggregate| aggregate.fields.get(index as usize))
            .map(|field| &field.ty);
        let ty = shape.field(
            &field.ty,
            original_field,
            &mut vec![PlaceProjection::Field(index)],
            &mut vec![LifeTypeProjection::Place(PlaceProjection::Field(index))],
            0,
        )?;
        let ty = if field.manages {
            match ty {
                Type::Desc(payload) => Type::Managed(payload),
                Type::Named(_) => Type::Managed(Box::new(ty)),
                Type::Managed(_) => ty,
                _ => {
                    return Err(invalid(
                        field.span,
                        "stored package manages field lacks a resource type",
                    ));
                }
            }
        } else {
            ty
        };
        fields.push((field.name.clone(), ty));
    }
    let mut nominal_arguments = Vec::new();
    let mut arguments = arguments;
    let mut lives = lifetime_arguments.iter();
    for (index, argument) in arguments.iter_mut().enumerate() {
        if let LifeNominalActual::Lifetime(class) = argument {
            let (_, actual) = lives
                .next()
                .ok_or_else(|| invalid(span, "lifetime actual missing"))?;
            *class = shape.class(*actual, span)?;
            nominal_arguments.push(NominalLifeArgument {
                path: vec![],
                declaration: declaration.id,
                argument: u32::try_from(index)
                    .map_err(|_| invalid(span, "argument ordinal overflow"))?,
                binder: *class,
            });
        }
    }
    nominal_arguments.extend(nested_nominal_arguments);
    shape.budget.text(&aggregate.name, span)?;
    let binder_count =
        u32::try_from(shape.classes.len()).map_err(|_| invalid(span, "life class overflow"))?;
    let ty = Type::BorrowedStorage {
        storage: Box::new(if aggregate.kind == s::AggregateKind::Package {
            Type::Package {
                name: aggregate.name.clone(),
                fields,
            }
        } else {
            Type::Record {
                name: aggregate.name.clone(),
                fields,
            }
        }),
        life: StorageLifeShape {
            binder_count,
            slots: shape.slots,
            retained: shape.retained,
            nominal_arguments,
        },
    };
    Ok(Instance {
        instance: LifeNominalInstance {
            declaration: declaration.id,
            ty,
            arguments,
        },
        declaration,
        classes: shape.classes,
    })
}

// Keep original and physical variants plus both actual maps explicit: merging
// them would let a generated name stand in for source class evidence.
#[allow(clippy::too_many_arguments)]
pub(super) fn instantiate_enum_joint(
    physical: &s::EnumDecl,
    original: &s::EnumDecl,
    declaration: LifeNominalDeclaration,
    mut arguments: Vec<LifeNominalActual>,
    lifetime_arguments: &[(String, LexicalLife)],
    qualified: &BTreeMap<usize, QualifiedActual>,
    direct: &BTreeMap<Vec<LifeTypeProjection>, QualifiedActual>,
    budget: &mut generics::Budget<'_>,
    span: Span,
) -> R<Instance> {
    if declaration.kind != LifeNominalKind::Enum
        || declaration.parameters.len() != arguments.len()
        || physical.variants.len() != original.variants.len()
    {
        return Err(invalid(
            span,
            "stored enum declaration or full actual shape differs",
        ));
    }
    spend(
        budget,
        span,
        arguments.len().saturating_add(lifetime_arguments.len()),
    )?;
    let mut life_count = 0;
    for (formal, actual) in declaration.parameters.iter().zip(&arguments) {
        match (formal, actual) {
            (LifeParameterKind::Lifetime, LifeNominalActual::Lifetime(_)) => life_count += 1,
            (LifeParameterKind::Type, LifeNominalActual::Type(_)) => {}
            (LifeParameterKind::Const(domain), LifeNominalActual::Const(value))
                if *domain == value.ty() => {}
            _ => {
                return Err(invalid(
                    span,
                    "stored enum actual kind or scalar domain differs",
                ));
            }
        }
    }
    if life_count != lifetime_arguments.len() {
        return Err(invalid(
            span,
            "stored enum lifetime actual vector is incomplete",
        ));
    }
    let mut shape = Shape {
        budget,
        formals: lifetime_arguments,
        classes: Vec::new(),
        slots: Vec::new(),
        retained: Vec::new(),
        qualified,
        original: None,
        enum_formals: Some(&original.generics),
        direct,
    };
    for (index, (name, actual)) in lifetime_arguments.iter().enumerate() {
        for (prior, _) in &lifetime_arguments[..index] {
            shape.budget.text(name, span)?;
            shape.budget.text(prior, span)?;
            if name == prior {
                return Err(invalid(span, "duplicate lifetime formal in stored enum"));
            }
        }
        shape.class(*actual, span)?;
    }
    let mut nested_arguments = Vec::new();
    for (ordinal, actual) in qualified {
        let Some(LifeNominalActual::Type(ty)) = arguments.get(*ordinal) else {
            return Err(invalid(span, "qualified enum Type actual ordinal differs"));
        };
        pay_type(ty, shape.budget, span, 0)?;
        if ty != &actual.ty {
            return Err(invalid(span, "qualified enum Type actual metadata differs"));
        }
        if shape.address_argument(actual, *ordinal, span)? {
            continue;
        }
        let Type::BorrowedStorage { life, .. } = ty else {
            return Err(invalid(
                span,
                "qualified enum Type actual has no storage relation",
            ));
        };
        if life.binder_count as usize != actual.classes.len() {
            return Err(invalid(
                span,
                "qualified enum Type actual class mapping incomplete",
            ));
        }
        for row in &life.nominal_arguments {
            let lexical = *actual
                .classes
                .get(row.binder as usize)
                .ok_or_else(|| invalid(span, "qualified enum Type actual class absent"))?;
            let binder = shape.class(lexical, span)?;
            spend(shape.budget, span, row.path.len().saturating_add(2))?;
            let ordinal = u32::try_from(*ordinal)
                .map_err(|_| invalid(span, "enum Type actual ordinal overflow"))?;
            let mut path = vec![LifeTypeProjection::TypeArgument(ordinal)];
            path.extend_from_slice(&row.path);
            nested_arguments.push(NominalLifeArgument {
                path,
                declaration: row.declaration,
                argument: row.argument,
                binder,
            });
        }
    }
    let mut variants = Vec::new();
    for (variant_index, (actual, source)) in
        physical.variants.iter().zip(&original.variants).enumerate()
    {
        shape.budget.text(&actual.name, actual.span)?;
        shape.budget.text(&source.name, source.span)?;
        if actual.name != source.name {
            return Err(invalid(span, "stored enum variant order differs"));
        }
        let physical_count = match &actual.fields {
            s::VariantFields::Unit => 0,
            s::VariantFields::Tuple(fields) => fields.len(),
            s::VariantFields::Record(fields) => fields.len(),
        };
        let source_count = match &source.fields {
            s::VariantFields::Unit => 0,
            s::VariantFields::Tuple(fields) => fields.len(),
            s::VariantFields::Record(fields) => fields.len(),
        };
        spend(
            shape.budget,
            span,
            physical_count.saturating_add(source_count),
        )?;
        shape.budget.lineage_bytes(
            physical_count
                .saturating_add(source_count)
                .saturating_mul(std::mem::size_of::<&s::TypeRef>()),
            span,
        )?;
        let physical_fields: Vec<&s::TypeRef> = match &actual.fields {
            s::VariantFields::Unit => Vec::new(),
            s::VariantFields::Tuple(fields) => fields.iter().map(|field| &field.ty).collect(),
            s::VariantFields::Record(fields) => fields.iter().map(|field| &field.ty).collect(),
        };
        let source_fields: Vec<&s::TypeRef> = match &source.fields {
            s::VariantFields::Unit => Vec::new(),
            s::VariantFields::Tuple(fields) => fields.iter().map(|field| &field.ty).collect(),
            s::VariantFields::Record(fields) => fields.iter().map(|field| &field.ty).collect(),
        };
        if std::mem::discriminant(&actual.fields) != std::mem::discriminant(&source.fields)
            || physical_fields.len() != source_fields.len()
        {
            return Err(invalid(span, "stored enum variant payload shape differs"));
        }
        if let (s::VariantFields::Record(fields), s::VariantFields::Record(source_fields)) =
            (&actual.fields, &source.fields)
        {
            for (field, source_field) in fields.iter().zip(source_fields) {
                shape.budget.text(&field.name, field.span)?;
                shape.budget.text(&source_field.name, source_field.span)?;
                if field.name != source_field.name {
                    return Err(invalid(
                        field.span,
                        "stored enum record payload label differs",
                    ));
                }
            }
        }
        let mut fields = Vec::new();
        for (field_index, (field, source_field)) in
            physical_fields.iter().zip(source_fields).enumerate()
        {
            let variant = u32::try_from(variant_index)
                .map_err(|_| invalid(span, "variant ordinal overflow"))?;
            let index = u32::try_from(field_index)
                .map_err(|_| invalid(span, "variant field ordinal overflow"))?;
            let place = PlaceProjection::Variant(zil_ir::VariantProjection { variant, index });
            fields.push(shape.field(
                field,
                Some(source_field),
                &mut vec![place],
                &mut vec![LifeTypeProjection::Place(place)],
                0,
            )?);
        }
        variants.push((actual.name.clone(), fields));
    }
    let mut nominal_arguments = Vec::new();
    let mut lives = lifetime_arguments.iter();
    for (ordinal, actual) in arguments.iter_mut().enumerate() {
        if let LifeNominalActual::Lifetime(class) = actual {
            let (_, lexical) = lives
                .next()
                .ok_or_else(|| invalid(span, "stored enum life actual missing"))?;
            *class = shape.class(*lexical, span)?;
            nominal_arguments.push(NominalLifeArgument {
                path: Vec::new(),
                declaration: declaration.id,
                argument: u32::try_from(ordinal)
                    .map_err(|_| invalid(span, "enum argument ordinal overflow"))?,
                binder: *class,
            });
        }
    }
    nominal_arguments.extend(nested_arguments);
    shape.budget.text(&physical.name, span)?;
    let binder_count =
        u32::try_from(shape.classes.len()).map_err(|_| invalid(span, "enum class overflow"))?;
    let ty = Type::BorrowedStorage {
        storage: Box::new(Type::Enum {
            name: physical.name.clone(),
            variants,
        }),
        life: StorageLifeShape {
            binder_count,
            slots: shape.slots,
            retained: shape.retained,
            nominal_arguments,
        },
    };
    Ok(Instance {
        instance: LifeNominalInstance {
            declaration: declaration.id,
            ty,
            arguments,
        },
        declaration,
        classes: shape.classes,
    })
}

#[cfg(test)]
mod source_tests {
    use super::*;
    use zil_base::{FileId, ModuleId};

    #[test]
    fn slice_iterator_reaches_complete_core() {
        let core = expand_text(include_str!(
            "../../../tests/language-readiness/terminal/higher-rank-borrows/slice_iter.zil"
        ))
        .unwrap();
        assert!(
            core.functions
                .iter()
                .any(|function| function.name == "result")
        );
        assert!(
            core.life_declarations
                .iter()
                .any(|declaration| declaration.id.compiler)
        );
    }

    #[test]
    fn option_result_flows_through_a_real_parameter() {
        expand_text("fn next<life s>(slice:&'s [i64])->Option<&'s i64>{return Option::<&'s i64>.None();}fn id<life s>(value:Option<&'s i64>)->Option<&'s i64>{return value;}fn forward<life s>(slice:&'s [i64])->Option<&'s i64>{let value=next(slice);return id(value);}fn main(){}")
            .unwrap();
    }

    #[test]
    fn grouped_reference_option_keeps_catalog_but_not_callee_label_in_hint() {
        let core = expand_text("fn next<life s>(slice:&'s [i64])->Option<(&'s i64)>{return Option::<(&'s i64)>.Some(&slice[0]);}fn result()->i64{let values:[i64;1]=[42];let value=next(&values[..]);match value{Option.Some(n)=>{return *n},Option.None()=>{return 0}}}").unwrap();
        let instance = core
            .life_instances
            .iter()
            .find(|instance| instance.declaration.compiler)
            .unwrap();
        assert_eq!(
            instance.arguments,
            vec![LifeNominalActual::Type(Type::Address {
                mutable: false,
                inner: Box::new(Type::I64),
            })]
        );
        let Type::BorrowedStorage { life, .. } = &instance.ty else {
            panic!("{instance:?}")
        };
        assert_eq!(life.retained.len(), 1);
        assert_eq!(life.slots.len(), 1);
        assert!(
            core.functions
                .iter()
                .any(|function| function.name == "result")
        );
    }

    #[test]
    fn option_reference_actual_keeps_real_source_class_and_variant_slot() {
        let core = expand_text("fn next<life s>(slice:&'s [i64])->Option<&'s i64>{return Option::<&'s i64>.Some(&slice[0]);}fn main(){}").unwrap();
        let declaration = core
            .life_declarations
            .iter()
            .find(|declaration| declaration.id.compiler)
            .expect("sealed Option declaration");
        assert_eq!(declaration.id.module, ModuleId(u32::MAX));
        assert_eq!(declaration.identity.package, "@zil.builtin");
        let instance = core
            .life_instances
            .iter()
            .find(|instance| instance.declaration == declaration.id)
            .unwrap();
        assert_eq!(
            instance.arguments,
            vec![LifeNominalActual::Type(Type::Address {
                mutable: false,
                inner: Box::new(Type::I64),
            })]
        );
        let Type::BorrowedStorage { life, .. } = &instance.ty else {
            panic!("{instance:?}")
        };
        assert_eq!(life.binder_count, 1);
        assert_eq!(
            life.retained,
            vec![StorageRetainedBinding {
                path: vec![LifeTypeProjection::TypeArgument(0)],
                class: 0,
                binder: 0,
            }]
        );
        assert_eq!(
            life.slots,
            vec![StorageLifeSlot {
                path: vec![PlaceProjection::Variant(zil_ir::VariantProjection {
                    variant: 1,
                    index: 0
                })],
                binder: 0,
                mode: StoredBorrowMode::SharedAddress,
            }]
        );
        assert!(life.nominal_arguments.is_empty());
        assert!(
            core.functions
                .iter()
                .find(|f| f.name == "next")
                .unwrap()
                .storage_life
                .is_some()
        );
    }

    #[test]
    fn nested_type_actual_keeps_callers_distinct_life_from_outer_formal() {
        let core = expand_text("struct Leaf<life a>{n:i32}struct Wrap<T:Copy>{v:T}struct Outer<life a,T:Copy>{v:Wrap<T>}fn accept<life a,life b>(p:&'a i32,q:&'b i32,x:Outer<'b,Leaf<'a>>)->i32{return 0}fn main(){}").unwrap();
        let outer = core.life_instances.iter().find(|instance|
            matches!(&instance.ty, Type::BorrowedStorage { storage, .. }
                if matches!(storage.as_ref(), Type::Record { name, .. } if name.starts_with("$stored_Outer_"))))
            .expect("actual Outer specialization");
        let Type::BorrowedStorage { life, .. } = &outer.ty else {
            unreachable!()
        };
        assert_eq!(
            life.binder_count, 2,
            "caller Type life must not collapse into outer formal life"
        );
        assert!(life.retained.iter().any(|row| row.binder == 1
            && row.path == [LifeTypeProjection::Place(PlaceProjection::Field(0))]));
        assert!(matches!(
            outer.arguments.as_slice(),
            [
                LifeNominalActual::Lifetime(0),
                LifeNominalActual::Type(Type::BorrowedStorage { .. })
            ]
        ));
    }

    #[test]
    fn scoped_unit_enum_and_type_only_enum_keep_source_actuals_for_patterns() {
        for source in [
            "enum Empty<life a>{Only}fn read<life a>(p:&'a i32)->i32{let x:Empty<'a>=Empty::<'a>.Only();match x{Empty.Only()=>{return 1}}}fn main(){}",
            "struct Leaf<life a>{n:i32}enum Boxed<T:Copy>{V(T)}fn read<life a>(p:&'a i32)->i32{let x:Boxed<Leaf<'a>>=Boxed::<Leaf<'a>>.V(Leaf::<'a>{n:3});match x{Boxed.V(v)=>{return v.n}}}fn main(){}",
            "struct Leaf<life a>{n:i32}enum Boxed<T:Copy>{V(T)}fn make<life a>(p:&'a i32)->Boxed<Leaf<'a>>{return Boxed.V(Leaf::<'a>{n:1})}fn main(){}",
        ] {
            expand_text(source).unwrap_or_else(|errors| panic!("{source}: {errors:?}"));
        }
    }

    #[test]
    fn scoped_enum_payload_cannot_exchange_distinct_phantom_lifetime_actuals() {
        let source = "struct Leaf<life a>{n:i32}enum Boxed<T:Copy>{V(T)}fn read<life a,life b>(p:&'a i32,q:&'b i32)->i32{let x:Boxed<Leaf<'a>>=Boxed::<Leaf<'a>>.V(Leaf::<'b>{n:3});match x{Boxed.V(v)=>{return v.n}}}fn main(){}";
        assert!(expand_text(source).is_err());
    }

    #[test]
    fn stored_const_actuals_preserve_full_life_type_const_ordinals_and_bits() {
        for (domain, literal, expected) in [
            ("i32", "7", Constant::I32(7)),
            ("u64", "18446744073709551615", Constant::U64(u64::MAX)),
            ("bool", "true", Constant::Bool(true)),
        ] {
            let core = expand_text(&format!("struct Phantom<life s,T:Copy,#N:{domain}>{{n:T}}fn make<life s>(p:&'s i64)->Phantom<'s,i32,{literal}>{{return Phantom::<'s,i32,{literal}>{{n:42}}}}fn main(){{}}"))
                .unwrap();
            assert_eq!(
                core.life_declarations[0].parameters,
                vec![
                    LifeParameterKind::Lifetime,
                    LifeParameterKind::Type,
                    LifeParameterKind::Const(expected.ty())
                ]
            );
            assert_eq!(
                core.life_instances[0].arguments,
                vec![
                    LifeNominalActual::Lifetime(0),
                    LifeNominalActual::Type(Type::I32),
                    LifeNominalActual::Const(expected)
                ]
            );
        }
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "usize-const.zil",
            "struct Phantom<life s,#N:usize>{n:i32}fn make<life s>(p:&'s i64)->Phantom<'s,4294967296>{return Phantom::<'s,4294967296>{n:42}}fn main(){}",
        );
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let aliases = BTreeMap::new();
        let target = zil_base::TargetProfile {
            pointer_width: 32,
            ..Default::default()
        };
        let rejected = expand_with_map_limits_target_context(
            &source,
            &syntax,
            &Limits::default(),
            &target,
            crate::source_context::SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &aliases,
                declarations: crate::source_context::DeclarationPlacement::Original,
            },
        )
        .unwrap_err();
        assert!(
            rejected
                .iter()
                .any(|error| error.message.contains("outside its target integer domain")),
            "{rejected:?}"
        );
    }

    const VIEW: &str = r#"
struct View<life a> { p: &'a i64 }
fn make<life a>(p: &'a i64) -> View<'a> {
    return View::<'a> { p: p };
}
fn read<life a>(v: &View<'a>) -> &'a i64 { return v.p; }
fn main() -> i32 {
    let n: i64 = 42;
    let v = make(&n);
    let q = read(&v);
    return i32(*q);
}
"#;

    fn expand_text(text: &str) -> CompileResult<CoreModule> {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "stored-view.zil", text);
        let syntax = zil_syntax::parse(&source).into_result()?;
        let aliases = BTreeMap::new();
        let context = source_context::SourceIdentityContext::ClosedSnapshot {
            root: source.module_id,
            aliases: &aliases,
            declarations: source_context::DeclarationPlacement::Original,
        };
        Ok(expand_with_map_limits_target_context(
            &source,
            &syntax,
            &Limits::default(),
            &zil_base::TargetProfile::default(),
            context,
        )?
        .core)
    }

    #[test]
    fn source_view_factory_and_helper_keep_scoped_relations() {
        let core = expand_text(VIEW).unwrap();
        assert_eq!(core.life_declarations.len(), 1);
        assert_eq!(core.life_instances.len(), 1);
        let Type::BorrowedStorage { life, .. } = &core.life_instances[0].ty else {
            panic!("source nominal lost retained shape");
        };
        assert_eq!(life.binder_count, 1);
        assert_eq!(
            life.slots,
            vec![StorageLifeSlot {
                path: vec![PlaceProjection::Field(0)],
                binder: 0,
                mode: StoredBorrowMode::SharedAddress
            }]
        );
        assert_eq!(life.nominal_arguments[0].argument, 0);
        let factory = core.functions.iter().find(|f| f.name == "make").unwrap();
        let contract = factory.storage_life.as_ref().unwrap();
        assert_eq!(
            (
                contract.fixed_binder_count,
                contract.invocation_binder_count
            ),
            (0, 1)
        );
        assert_eq!(contract.bindings.len(), 1);
        assert_eq!(contract.classes.len(), 1);
        assert_eq!(contract.bindings[0].binder, contract.classes[0].binder);
        assert_eq!(
            contract.classes[0].position,
            SignatureLifeClass {
                base: StorageBase::Result,
                path: vec![],
                class: 0,
            }
        );
        let helper = core.functions.iter().find(|f| f.name == "read").unwrap();
        let contract = helper.storage_life.as_ref().unwrap();
        assert_eq!(
            (
                contract.fixed_binder_count,
                contract.invocation_binder_count
            ),
            (1, 1)
        );
        let result = contract
            .bindings
            .iter()
            .find(|binding| binding.position.base == StorageBase::Result)
            .unwrap();
        assert_eq!(result.binder, StorageBinder::Fixed(0));
        assert_eq!(
            contract.classes,
            vec![SignatureLifeClassBinding {
                position: SignatureLifeClass {
                    base: StorageBase::Parameter(0),
                    path: vec![LifeTypeProjection::Pointee],
                    class: 0
                },
                binder: StorageBinder::Fixed(0),
            }]
        );
        assert!(
            contract
                .bindings
                .iter()
                .all(|binding| binding.position.path.is_empty())
        );
    }

    #[test]
    fn stored_source_headers_are_available_before_callee_bodies() {
        let source = VIEW.replace("fn main() -> i32 {", "fn entry() -> i32 {");
        let cut = source.find("fn entry").unwrap();
        let reordered = format!("{}\n{}", &source[cut..], &source[..cut]);
        let core = expand_text(&reordered).unwrap();
        assert_eq!(core.life_instances.len(), 1);
        assert!(
            core.functions
                .iter()
                .find(|f| f.name == "read")
                .unwrap()
                .storage_life
                .is_some()
        );
    }

    #[test]
    fn full_nominal_ordinals_seed_phantom_and_reversed_field_classes() {
        for (a, b, p) in [("a", "b", "p"), ("renamed", "other", "phantom")] {
            let source = format!(
                r#"
struct Pair<life {p}, T: Copy, life {a}, life {b}> {{ second: &'{b} i64, first: &'{a} i64 }}
fn make<life {p}, life {a}, life {b}>(unused: &'{p} i64, x: &'{a} i64, y: &'{b} i64) -> Pair<'{p}, i64, '{a}, '{b}> {{
 return Pair::<'{p}, i64, '{a}, '{b}> {{ second: y, first: x }};
}}
fn same<life {a}>(x: &'{a} i64) -> Pair<'{a}, i64, '{a}, '{a}> {{
 return Pair::<'{a}, i64, '{a}, '{a}> {{ second: x, first: x }};
}}
"#
            );
            let core = expand_text(&source).unwrap();
            assert_eq!(core.life_declarations.len(), 1);
            assert_eq!(core.life_instances.len(), 2);
            let shapes: Vec<_> = core
                .life_instances
                .iter()
                .map(|instance| {
                    let Type::BorrowedStorage { life, .. } = &instance.ty else {
                        panic!("missing shape");
                    };
                    life
                })
                .collect();
            let split = shapes.iter().find(|life| life.binder_count == 3).unwrap();
            assert_eq!(
                split
                    .nominal_arguments
                    .iter()
                    .map(|row| (row.argument, row.binder))
                    .collect::<Vec<_>>(),
                [(0, 0), (2, 1), (3, 2)]
            );
            assert_eq!(
                split
                    .slots
                    .iter()
                    .map(|slot| slot.binder)
                    .collect::<Vec<_>>(),
                [2, 1]
            );
            let shared = shapes.iter().find(|life| life.binder_count == 1).unwrap();
            assert!(shared.nominal_arguments.iter().all(|row| row.binder == 0));
            assert!(shared.slots.iter().all(|row| row.binder == 0));
            let make = core
                .functions
                .iter()
                .find(|function| function.name == "make")
                .unwrap();
            let contract = make.storage_life.as_ref().unwrap();
            assert_eq!(
                contract
                    .classes
                    .iter()
                    .map(|row| (row.position.class, row.binder))
                    .collect::<Vec<_>>(),
                [
                    (0, StorageBinder::Invocation(0)),
                    (1, StorageBinder::Invocation(1)),
                    (2, StorageBinder::Invocation(2))
                ]
            );
            assert!(contract.classes.iter().all(
                |row| row.position.base == StorageBase::Result && row.position.path.is_empty()
            ));
        }
    }

    #[test]
    fn earlier_reference_to_fixed_input_preserves_full_binding_order() {
        let core = expand_text(
            r#"
struct View<life a> { p: &'a i64 }
fn first<life a, life b>(x: &'b i64, va: View<'a>, vb: View<'b>) -> &'b i64 { return x; }
"#,
        )
        .unwrap();
        let contract = core.functions[0].storage_life.as_ref().unwrap();
        assert_eq!(
            (
                contract.fixed_binder_count,
                contract.invocation_binder_count
            ),
            (2, 0)
        );
        assert_eq!(
            contract
                .bindings
                .iter()
                .map(|row| row.binder)
                .collect::<Vec<_>>(),
            [StorageBinder::Fixed(0), StorageBinder::Fixed(0)]
        );
        assert_eq!(
            contract
                .classes
                .iter()
                .map(|row| row.binder)
                .collect::<Vec<_>>(),
            [StorageBinder::Fixed(1), StorageBinder::Fixed(0)]
        );
    }

    #[test]
    fn borrowed_type_actuals_cannot_lose_joint_context() {
        let plain = r#"
struct Phantom<life a, T> { p: &'a i64 }
fn make<life a, life b>(x: &'a i64, y: &'b i64) -> Phantom<'a, ACTUAL> {
 return Phantom::<'a, ACTUAL> { p: x };
}
"#;
        expand_text(&plain.replace("ACTUAL", "i64")).unwrap();
        for actual in ["&'a i64", "&'b i64"] {
            let errors = expand_text(&plain.replace("ACTUAL", actual)).unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|error| error.category == zil_base::ResultCategory::Unsupported)
            );
        }
    }

    #[test]
    fn rich_bfp_source_classes_keep_joint_invocation_labels() {
        let mut types = Vec::new();
        let mut inventories = Vec::new();
        // Alpha tests across edited requests need an explicit stable naming
        // context; ClosedSnapshot deliberately invalidates on byte changes.
        let expand_signature = |text: &str| -> CompileResult<CoreModule> {
            let source = SourceUnit::new(FileId(0), ModuleId(0), "alpha-code.zil", text);
            let syntax = zil_syntax::parse(&source).into_result()?;
            let modules = BTreeMap::from([(ModuleId(0), Vec::new())]);
            Ok(expand_with_map_limits_target_context(
                &source,
                &syntax,
                &Limits::default(),
                &zil_base::TargetProfile::default(),
                source_context::SourceIdentityContext::Explicit {
                    package: "terminal341-rich-code-alpha",
                    modules: &modules,
                    declarations: source_context::DeclarationPlacement::Original,
                },
            )?
            .core)
        };
        for (signature, class, binders) in [
            ("for<life a> fn(&'a Tag<'a>)->i32", 0, 1),
            ("for<life a> for<life b> fn(&'a Tag<'b>,&'b i32)->i32", 1, 2),
            ("for<life a> for<life b> fn(&'a Tag<'a>,&'b i32)->i32", 0, 2),
            ("for<life x> for<life y> fn(&'x Tag<'y>,&'y i32)->i32", 1, 2),
        ] {
            let core = expand_signature(&format!("struct Tag<life s>{{n:i32}}type Handler={signature};fn use_handler(f:Handler){{}}fn main(){{}}"))
                .unwrap();
            let ty = core
                .functions
                .iter()
                .find(|f| f.name == "use_handler")
                .unwrap()
                .parameters[0]
                .ty
                .clone();
            let Type::BorrowedFunctionPointer {
                parameters, life, ..
            } = &ty
            else {
                panic!("{ty:?}")
            };
            assert_eq!(
                (life.fixed_binder_count, life.invocation_binder_count),
                (0, binders)
            );
            assert_eq!(life.bindings[0].binder, StorageBinder::Invocation(0));
            assert_eq!(
                life.classes,
                vec![SignatureLifeClassBinding {
                    position: SignatureLifeClass {
                        base: StorageBase::Parameter(0),
                        path: vec![LifeTypeProjection::Pointee],
                        class: 0
                    },
                    binder: StorageBinder::Invocation(class)
                }]
            );
            assert!(
                matches!(&parameters[0],Type::Address {inner,..} if matches!(inner.as_ref(),Type::BorrowedStorage {..}))
            );
            types.push(ty);
            inventories.push(core.life_declarations);
        }
        assert_ne!(types[1], types[2]);
        assert_eq!(types[1], types[3]);
        assert_eq!(inventories[1], inventories[3]);
    }

    #[test]
    fn rich_bfp_phantom_actual_and_unused_aliases_are_checked() {
        let core = expand_text("struct Tag<life a>{n:i32}type Code=for<life a>fn(&'a Tag<'a>)->i32;struct Holder<T:Copy>{n:i32}fn make()->Holder<Code>{return Holder::<Code>{n:42}}fn main()->i32{let h=make();return h.n}").unwrap();
        assert_eq!(core.life_instances.len(), 1);
        assert!(
            core.aggregates
                .iter()
                .any(|a| a.name.starts_with("Holder$") || a.name.contains("Holder"))
        );
        for alias in [
            "for<life a>fn(&'a Missing<'a>)->i32",
            "for<life a>fn(&'a Tag<'missing>)->i32",
            "for<life a>fn(&'a Tag<'a,'a>)->i32",
            "for<life a>fn(&'a Tag<i32>)->i32",
            "for<life a>fn(&'a Tag<'a>)->for<life b>fn(&'b Tag<'b>)->i32",
        ] {
            let rejected = expand_text(&format!(
                "struct Tag<life a>{{n:i32}}type Unused={alias};fn main(){{}}"
            ));
            assert!(rejected.is_err(), "unused alias admitted: {alias}");
        }
        let unused = expand_text(
            "struct Tag<life a>{n:i32}type Unused=for<life a>fn(&'a Tag<'a>)->i32;fn main(){}",
        )
        .unwrap();
        assert_eq!(unused.life_instances.len(), 1);
    }

    #[test]
    fn rich_bfp_grouped_aliases_and_record_fields_keep_complete_contracts() {
        let core=expand_text("struct Tag<life a>{n:i32}type Code=(for<life a>fn(&'a Tag<'a>)->i32);struct Box{f:Code}fn pass(f:Code)->Code{return f}fn main(){}").unwrap();
        let field = &core
            .aggregates
            .iter()
            .find(|a| a.name == "Box")
            .unwrap()
            .fields[0]
            .ty;
        let function = core.functions.iter().find(|f| f.name == "pass").unwrap();
        assert_eq!(field, &function.parameters[0].ty);
        assert_eq!(field, &function.return_type);
        let Type::BorrowedFunctionPointer {
            parameters, life, ..
        } = field
        else {
            panic!("{field:?}")
        };
        assert_eq!(life.classes.len(), 1);
        assert_eq!(life.classes[0].binder, StorageBinder::Invocation(0));
        assert!(
            matches!(&parameters[0],Type::Address{inner,..} if matches!(inner.as_ref(),Type::BorrowedStorage{..}))
        );
        for use_site in [
            "fn bad(f:(Code,i32)){}",
            "struct Bad{f:(Code,i32)}",
            "type Bad=[Code;1];",
        ] {
            let errors=expand_text(&format!("struct Tag<life a>{{n:i32}}type Code=for<life a>fn(&'a Tag<'a>)->i32;{use_site}fn main(){{}}")).unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|e| e.category == zil_base::ResultCategory::Unsupported
                        && e.message.contains("contextual signature adapter")),
                "{errors:?}"
            );
        }
    }

    #[test]
    fn rich_bfp_registered_qualified_names_cannot_use_the_legacy_flat_path() {
        let core =
            expand_text("struct Tag<life s>{n:i32}fn read<life s>(p:&'s Tag<'s>)->i32{return p.n}")
                .unwrap();
        let Type::BorrowedStorage { storage, .. } = &core.life_instances[0].ty else {
            panic!()
        };
        let Type::Record { name, .. } = storage.as_ref() else {
            panic!()
        };
        let mut catalog = Catalog::default();
        catalog.instances.push(Instance {
            declaration: core.life_declarations[0].clone(),
            instance: core.life_instances[0].clone(),
            classes: vec![LexicalLife(0)],
        });
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "raw-legacy-code.zil",
            "fn take(f:for<life a>fn(&'a i32)->i32){}",
        );
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let s::ItemKind::Function(function) = &syntax.items[0].kind else {
            panic!()
        };
        let mut ty = function.params[0].ty.clone();
        let limits = Limits::default();
        assert!(
            catalog
                .pointer_type(&ty, &mut generics::Budget::new(&limits))
                .unwrap()
                .is_none()
        );
        let s::TypeKind::ForLifetime { inner, .. } = &mut ty.kind else {
            panic!()
        };
        let s::TypeKind::Function { params, .. } = &mut inner.kind else {
            panic!()
        };
        let s::TypeKind::Reference { inner, .. } = &mut params[0].kind else {
            panic!()
        };
        inner.kind = s::TypeKind::Named {
            path: vec![name.clone()],
            args: vec![],
        };
        let rejected = catalog
            .pointer_type(&ty, &mut generics::Budget::new(&limits))
            .unwrap_err();
        assert_eq!(rejected.category, zil_base::ResultCategory::Unsupported);
        assert!(rejected.message.contains("actual Fixed context"));
    }

    #[test]
    fn rich_bfp_project_uses_actual_same_spelled_declarations() {
        let sources = [
            SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "moved/main.zil",
                "import a;import b;fn left(f:a.Code){}fn right(f:b.Code){}fn main(){}",
            ),
            SourceUnit::new(
                FileId(1),
                ModuleId(11),
                "moved/a.zil",
                "pub struct Tag<life a>{n:i32}pub type Code=for<life x>fn(&'x Tag<'x>)->i32;",
            ),
            SourceUnit::new(
                FileId(2),
                ModuleId(22),
                "moved/b.zil",
                "pub struct Tag<life a>{n:i32}pub type Code=for<life y>fn(&'y Tag<'y>)->i32;",
            ),
        ];
        let syntax = sources
            .iter()
            .map(|s| zil_syntax::parse(s).into_result().unwrap())
            .collect::<Vec<_>>();
        let imports = [
            vec![
                CoreImport {
                    alias: "a".into(),
                    module: ModuleId(11),
                    span: syntax[0].items[0].span,
                },
                CoreImport {
                    alias: "b".into(),
                    module: ModuleId(22),
                    span: syntax[0].items[1].span,
                },
            ],
            vec![],
            vec![],
        ];
        let aliases = BTreeMap::from([
            (vec!["a".into()], ModuleId(11)),
            (vec!["b".into()], ModuleId(22)),
        ]);
        let expanded = expand_program_with_limits_target_context(
            &sources,
            &syntax.iter().collect::<Vec<_>>(),
            &imports,
            &Limits::default(),
            &zil_base::TargetProfile::default(),
            source_context::SourceIdentityContext::ClosedSnapshot {
                root: ModuleId(0),
                aliases: &aliases,
                declarations: source_context::DeclarationPlacement::Original,
            },
        )
        .unwrap();
        let root = &expanded
            .iter()
            .find(|(m, _)| m.core.module_id == ModuleId(0))
            .unwrap()
            .0
            .core;
        let left = &root
            .functions
            .iter()
            .find(|f| f.name == "left")
            .unwrap()
            .parameters[0]
            .ty;
        let right = &root
            .functions
            .iter()
            .find(|f| f.name == "right")
            .unwrap()
            .parameters[0]
            .ty;
        assert_ne!(left, right);
        let mut declarations = expanded
            .iter()
            .flat_map(|(m, _)| &m.core.life_declarations)
            .map(|d| &d.identity)
            .collect::<Vec<_>>();
        declarations.sort();
        declarations.dedup();
        assert_eq!(declarations.len(), 2);
        assert_eq!(
            declarations
                .iter()
                .map(|d| d.module_path.clone())
                .collect::<Vec<_>>(),
            [vec!["a".to_string()], vec!["b".to_string()]]
        );
    }

    #[test]
    fn rich_bfp_and_grouped_const_domains_prepay_exact_request_limits() {
        for (name,text) in [
            ("rich", "struct Tag<life s>{n:i32}type Code=for<life a>for<life b>fn(&'a Tag<'b>,&'b i32)->i32;fn first(f:Code){}fn second(f:Code){}fn main(){}".to_string()),
            ("const-domain",format!("struct Phantom<life s,#N:{}i32{}>{{n:i32}}fn make<life s>(p:&'s i64)->Phantom<'s,7>{{return Phantom::<'s,7>{{n:42}}}}fn main(){{}}","(".repeat(20),")".repeat(20))),
        ] {
            let source=SourceUnit::new(FileId(0),ModuleId(0),"exact-context.zil",text);
            let syntax=zil_syntax::parse(&source).into_result().unwrap();
            let aliases=BTreeMap::new();
            let run=|work| {
                let limits=Limits{max_analysis_steps:work,..Limits::default()};
                expand_with_map_limits_target_context(&source,&syntax,&limits,&zil_base::TargetProfile::default(),source_context::SourceIdentityContext::ClosedSnapshot{root:ModuleId(0),aliases:&aliases,declarations:source_context::DeclarationPlacement::Original})
            };
            let mut low=0;let mut high=Limits::default().max_analysis_steps;
            assert!(run(high).is_ok());
            while low+1<high {let mid=low+(high-low)/2;if run(mid).is_ok(){high=mid}else{low=mid}}
            assert!(run(high).is_ok());assert!(run(high).is_ok());
            let rejected=run(high-1).unwrap_err();
            assert!(rejected.iter().all(|e|e.category==zil_base::ResultCategory::ResourceLimit),"{rejected:?}");
            eprintln!("{name} exact source work N={high}, N-1 ResourceLimit, repeated exact succeeds");
        }
    }

    #[test]
    fn signature_classes_select_actual_qualified_tuple_boundaries() {
        let core = expand_text("struct Phantom<life a>{n:i32}fn select<life a,life b>(pair:(Phantom<'a>,Phantom<'b>))->Phantom<'b>{return pair.1}").unwrap();
        let contract = core
            .functions
            .iter()
            .find(|function| function.name == "select")
            .unwrap()
            .storage_life
            .as_ref()
            .unwrap();
        assert!(contract.bindings.is_empty());
        assert_eq!(
            (
                contract.fixed_binder_count,
                contract.invocation_binder_count
            ),
            (2, 0)
        );
        assert_eq!(
            contract.classes,
            vec![
                SignatureLifeClassBinding {
                    position: SignatureLifeClass {
                        base: StorageBase::Parameter(0),
                        path: vec![LifeTypeProjection::Place(PlaceProjection::Field(0))],
                        class: 0
                    },
                    binder: StorageBinder::Fixed(0)
                },
                SignatureLifeClassBinding {
                    position: SignatureLifeClass {
                        base: StorageBase::Parameter(0),
                        path: vec![LifeTypeProjection::Place(PlaceProjection::Field(1))],
                        class: 0
                    },
                    binder: StorageBinder::Fixed(1)
                },
                SignatureLifeClassBinding {
                    position: SignatureLifeClass {
                        base: StorageBase::Result,
                        path: vec![],
                        class: 0
                    },
                    binder: StorageBinder::Fixed(1)
                },
            ]
        );
    }

    const RICH_ROLE_HELPER: &str = "struct Tag<life s>{n:i32}fn invoke<life s,F:for<life a>for<life b>Fn(&'a Tag<'b>,&'b i32)->i32>(f:&F,p:&Tag<'s>,q:&'s i32)->i32{return f(p,q)}fn apply<life s>(x:&'s i32)->i32{let v:Tag<'s>=Tag::<'s>{n:*x};let f=(p,q)=>42;return invoke(&f,&v,x)}fn result()->i32{let n=21;return apply(&n)}fn main()->i32{return result()-42}";
    #[test]
    fn rich_callable_role_preserves_helper_and_invocation_domains() {
        let core = expand_text(RICH_ROLE_HELPER).unwrap();
        let helper = core.functions.iter().find(|f| f.name == "invoke").unwrap();
        let contract = helper.storage_life.as_ref().unwrap();
        assert_eq!(
            (
                contract.fixed_binder_count,
                contract.invocation_binder_count
            ),
            (1, 2)
        );
        assert_eq!(
            contract
                .bindings
                .iter()
                .map(|r| r.binder)
                .collect::<Vec<_>>(),
            [
                StorageBinder::Invocation(0),
                StorageBinder::Invocation(1),
                StorageBinder::Fixed(0)
            ]
        );
        assert_eq!(contract.classes[0].binder, StorageBinder::Fixed(0));
        assert_eq!(contract.classes[0].position.base, StorageBase::Parameter(1));
        let role = &helper.callable_parameters[0];
        let relation = role.life.as_ref().unwrap();
        assert_eq!(
            (
                relation.fixed_binder_count,
                relation.invocation_binder_count
            ),
            (0, 2)
        );
        assert_eq!(
            relation
                .bindings
                .iter()
                .map(|r| r.binder)
                .collect::<Vec<_>>(),
            [StorageBinder::Invocation(0), StorageBinder::Invocation(1)]
        );
        assert_eq!(relation.classes[0].binder, StorageBinder::Invocation(1));
        assert_eq!(
            relation.classes[0].position.path,
            [LifeTypeProjection::Pointee]
        );
        assert!(
            matches!(&role.parameters[0],Type::Address {inner,..} if matches!(inner.as_ref(),Type::BorrowedStorage {..}))
        );
    }
    #[test]
    fn unused_rich_role_definitions_check_rigid_classes_before_instances() {
        for (tag, q, expected) in [("b", "b", true), ("a", "b", false), ("a", "a", true)] {
            let text = format!(
                "struct Tag<life s>{{n:i32}}fn unused<life a,life b,F:for<life x>for<life y>Fn(&'x Tag<'y>,&'y i32)->i32>(f:&F,p:&'a Tag<'{tag}>,q:&'{q} i32)->i32{{return f(p,q)}}fn main(){{}}"
            );
            let result = expand_text(&text);
            if expected {
                result.unwrap();
            } else {
                let errors = result.unwrap_err();
                assert!(
                    errors
                        .iter()
                        .any(|e| e.message.contains("distinct declared lifetime classes")),
                    "{errors:?}"
                );
            }
        }
    }

    #[test]
    fn outer_only_callable_role_uses_the_common_invocation_contract() {
        let core = expand_text("fn apply<F:for<life a>Fn(&'a i64)->&'a i64>(f:&F,p:&i64)->&i64{return f(p)}fn main(){}").unwrap();
        let role = &core
            .functions
            .iter()
            .find(|function| function.name == "apply")
            .unwrap()
            .callable_parameters[0];
        let life = role.life.as_ref().unwrap();
        assert_eq!(
            (life.fixed_binder_count, life.invocation_binder_count),
            (0, 1)
        );
        assert!(life.classes.is_empty());
        assert_eq!(
            life.bindings,
            vec![
                StorageLifeBinding {
                    position: StoragePosition {
                        base: StorageBase::Parameter(0),
                        path: vec![]
                    },
                    binder: StorageBinder::Invocation(0)
                },
                StorageLifeBinding {
                    position: StoragePosition {
                        base: StorageBase::Result,
                        path: vec![]
                    },
                    binder: StorageBinder::Invocation(0)
                },
            ]
        );
    }

    #[test]
    fn joint_nominal_fields_select_original_type_formals() {
        let core = expand_text("struct Phantom<life a>{n:i32}struct Wrap<T:Copy,U:Copy>{left:T;right:(U)}fn make<life a,life b>(p:&'a i32,q:&'b i32)->Wrap<Phantom<'a>,Phantom<'b>>{return Wrap::<Phantom<'a>,Phantom<'b>>{left:Phantom::<'a>{n:1},right:Phantom::<'b>{n:2}}}").unwrap();
        let declaration = core
            .life_declarations
            .iter()
            .find(|d| {
                d.identity
                    .declaration_path
                    .last()
                    .is_some_and(|name| name == "Wrap")
            })
            .unwrap();
        let instance = core
            .life_instances
            .iter()
            .find(|i| i.declaration == declaration.id)
            .unwrap();
        assert_eq!(
            instance.arguments[0], instance.arguments[1],
            "the two actuals deliberately share their child-local shape"
        );
        let Type::BorrowedStorage { storage, life } = &instance.ty else {
            panic!("{instance:?}")
        };
        assert_eq!(life.binder_count, 2);
        assert_eq!(
            life.retained,
            vec![
                StorageRetainedBinding {
                    path: vec![LifeTypeProjection::Place(PlaceProjection::Field(0))],
                    class: 0,
                    binder: 0
                },
                StorageRetainedBinding {
                    path: vec![LifeTypeProjection::Place(PlaceProjection::Field(1))],
                    class: 0,
                    binder: 1
                },
            ]
        );
        let Type::Record { fields, .. } = storage.as_ref() else {
            panic!()
        };
        assert!(
            fields
                .iter()
                .all(|(_, ty)| matches!(ty, Type::BorrowedStorage { .. }))
        );
    }

    #[test]
    fn stored_instances_and_header_maps_pay_common_prefix_searches() {
        let prefix = "long_stored_prefix_".repeat(12);
        let nominal = format!("{prefix}View");
        let mut text = format!("struct {nominal}<life a, T> {{ p: &'a i64 }}\n");
        for index in 1..=12 {
            text.push_str(&format!("fn {prefix}make{index}<life a>(p: &'a i64) -> {nominal}<'a, [i64; {index}]> {{ return {nominal}::<'a, [i64; {index}]> {{ p: p }}; }}\n"));
        }
        let source = SourceUnit::new(FileId(0), ModuleId(0), "stored-budget.zil", &text);
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let aliases = BTreeMap::new();
        let run = |work| {
            let limits = Limits {
                max_analysis_steps: work,
                ..Limits::default()
            };
            expand_with_map_limits_target_context(
                &source,
                &syntax,
                &limits,
                &zil_base::TargetProfile::default(),
                source_context::SourceIdentityContext::ClosedSnapshot {
                    root: source.module_id,
                    aliases: &aliases,
                    declarations: source_context::DeclarationPlacement::Original,
                },
            )
            .map(|expanded| expanded.core)
        };
        let reference = run(Limits::default().max_analysis_steps).unwrap();
        assert_eq!(reference.life_instances.len(), 12);
        let (mut lo, mut hi) = (1, Limits::default().max_analysis_steps);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            match run(mid) {
                Ok(_) => hi = mid,
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .any(|error| error.category == zil_base::ResultCategory::ResourceLimit)
                    );
                    lo = mid + 1;
                }
            }
        }
        assert_eq!(run(lo).unwrap(), reference);
        assert!(
            run(lo - 1)
                .unwrap_err()
                .iter()
                .any(|error| error.category == zil_base::ResultCategory::ResourceLimit)
        );
        assert_eq!(run(lo).unwrap(), reference);
        eprintln!("stored source common-prefix work boundary N={lo}");
    }
}
