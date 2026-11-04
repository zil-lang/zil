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
    project_functions: Vec<(zil_base::ModuleId, String, StorageLifeContract)>,
    pub(super) pointers: BTreeMap<String, Type>,
    pub(super) borrowed_signatures: bool,
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
                        && let Type::Record { name, .. } = storage.as_ref()
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
            let Type::Record { name, fields } = storage.as_ref() else {
                return Err(invalid(
                    Span::default(),
                    "stored producer lost record layout",
                ));
            };
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
            // The source field lowering retained labels until this point; the
            // complete scoped shape, rather than a bare Address, is published
            // together with each actual physical field type.
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
            pay_instance(produced, budget, aggregate.span)?;
            spend(budget, aggregate.span, core.life_declarations.len())?;
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

fn pay_type(ty: &Type, budget: &mut generics::Budget<'_>, span: Span, depth: usize) -> R<()> {
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
        Type::Record { name, fields } => {
            budget.text(name, span)?;
            for (name, ty) in fields {
                budget.text(name, span)?;
                pay_type(ty, budget, span, depth + 1)?;
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
        | Type::U64
        | Type::Usize
        | Type::I32
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
    let mut grounded = Vec::new();
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
    Ok(StorageLifeContract {
        fixed_binder_count: u32::try_from(fixed.len())
            .map_err(|_| invalid(span, "fixed binder overflow"))?,
        invocation_binder_count: u32::try_from(invocation.len())
            .map_err(|_| invalid(span, "invocation binder overflow"))?,
        bindings,
        classes,
    })
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

struct Shape<'a, 'l> {
    budget: &'a mut generics::Budget<'l>,
    formals: &'a [(String, LexicalLife)],
    classes: Vec<LexicalLife>,
    slots: Vec<StorageLifeSlot>,
}

impl Shape<'_, '_> {
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
        path: &mut Vec<PlaceProjection>,
        depth: usize,
    ) -> R<Type> {
        self.budget.node(source.span, depth)?;
        match &source.kind {
            s::TypeKind::Group(inner) => self.field(inner, path, depth + 1),
            s::TypeKind::Reference {
                lifetime,
                mutable,
                inner,
            } => {
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
                    let ty = self.field(field, path, depth + 1)?;
                    path.pop();
                    types.push(ty);
                }
                Ok(Type::Tuple(types))
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

/// `aggregate` is the actual substituted declaration and `identity` comes from
/// the original SourceIndex/Origins chain. Names are used only for lookup and
/// storage spelling, never parsed to reconstruct declaration identity.
pub(super) fn instantiate(
    aggregate: &s::Aggregate,
    declaration: LifeNominalDeclaration,
    arguments: Vec<LifeNominalActual>,
    lifetime_arguments: &[(String, LexicalLife)],
    budget: &mut generics::Budget<'_>,
    span: Span,
) -> R<Instance> {
    if aggregate.kind != s::AggregateKind::Struct || declaration.kind != LifeNominalKind::Record {
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
    let mut shape = Shape {
        budget,
        formals: lifetime_arguments,
        classes: vec![],
        slots: vec![],
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
    let mut fields = Vec::new();
    for (index, field) in aggregate.fields.iter().enumerate() {
        shape.budget.text(&field.name, field.span)?;
        let index =
            u32::try_from(index).map_err(|_| invalid(field.span, "field ordinal overflow"))?;
        let ty = shape.field(&field.ty, &mut vec![PlaceProjection::Field(index)], 0)?;
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
    shape.budget.text(&aggregate.name, span)?;
    let binder_count =
        u32::try_from(shape.classes.len()).map_err(|_| invalid(span, "life class overflow"))?;
    let ty = Type::BorrowedStorage {
        storage: Box::new(Type::Record {
            name: aggregate.name.clone(),
            fields,
        }),
        life: StorageLifeShape {
            binder_count,
            slots: shape.slots,
            retained: vec![],
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
        let mut target = zil_base::TargetProfile::default();
        target.pointer_width = 32;
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
