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
}

impl Catalog {
    pub(super) fn publish(
        &self,
        core: &mut CoreModule,
        budget: &mut generics::Budget<'_>,
    ) -> R<()> {
        if self.instances.is_empty() && self.functions.is_empty() {
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
            if let Some(contract) = self.functions.get(&function.name) {
                for binding in &contract.bindings {
                    spend(
                        budget,
                        function.span,
                        binding.position.path.len().saturating_add(1),
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
    pub(super) position: StoragePosition,
    pub(super) life: LexicalLife,
    /// True only for a class of an actual qualified INPUT carrier. A result
    /// occurrence cannot introduce a fixed upper or revive a dead target.
    pub(super) fixed_input: bool,
}

/// Form the complete retained relation from source occurrences before type
/// labels disappear. The same lexical name has one class across all positions;
/// a pre-existing input class remains Fixed even on an ordinary result address.
pub(super) fn contract(
    occurrences: Vec<Occurrence>,
    budget: &mut generics::Budget<'_>,
    span: Span,
) -> R<StorageLifeContract> {
    let mut fixed = Vec::new();
    for occurrence in &occurrences {
        spend(
            budget,
            span,
            occurrence.position.path.len().saturating_add(1),
        )?;
        if occurrence.fixed_input {
            if !matches!(
                occurrence.position.base,
                StorageBase::Parameter(_) | StorageBase::Capture(_)
            ) {
                return Err(invalid(span, "a result cannot introduce a fixed lifetime"));
            }
            spend(budget, span, fixed.len().saturating_add(1))?;
            if !fixed.contains(&occurrence.life) {
                fixed.push(occurrence.life);
            }
        }
    }
    let mut invocation = Vec::new();
    let mut bindings: Vec<StorageLifeBinding> = Vec::new();
    for occurrence in occurrences {
        // Every comparison includes all actual path segments, before storing
        // any relation row. An equal physical position cannot carry two labels.
        for prior in &bindings {
            spend(
                budget,
                span,
                prior
                    .position
                    .path
                    .len()
                    .saturating_add(occurrence.position.path.len())
                    .saturating_add(1),
            )?;
            if prior.position == occurrence.position {
                return Err(invalid(span, "duplicate stored-lifetime contract position"));
            }
        }
        spend(
            budget,
            span,
            fixed
                .len()
                .saturating_add(invocation.len())
                .saturating_add(1),
        )?;
        let binder = if let Some(index) = fixed.iter().position(|life| *life == occurrence.life) {
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
        bindings.push(StorageLifeBinding {
            position: occurrence.position,
            binder,
        });
    }
    Ok(StorageLifeContract {
        fixed_binder_count: u32::try_from(fixed.len())
            .map_err(|_| invalid(span, "fixed binder overflow"))?,
        invocation_binder_count: u32::try_from(invocation.len())
            .map_err(|_| invalid(span, "invocation binder overflow"))?,
        bindings,
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

    #[test]
    fn source_view_factory_and_helper_keep_scoped_relations() {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "stored-view.zil", VIEW);
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let core = expand(&source, &syntax).unwrap();
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
        assert_eq!(contract.bindings.len(), 2);
        assert_eq!(contract.bindings[0].binder, contract.bindings[1].binder);
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
    }
}
