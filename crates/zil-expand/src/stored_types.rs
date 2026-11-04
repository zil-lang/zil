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
}

impl Catalog {
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
    fn rich_bfp_source_classes_stay_closed_until_their_actual_producer() {
        for signature in [
            "for<life a> fn(&'a Tag<'a>)->i32",
            "for<life a> for<life b> fn(&'a Tag<'b>,&'b i32)->i32",
            "for<life a> for<life b> fn(&'a Tag<'a>,&'b i32)->i32",
        ] {
            let errors = expand_text(&format!("struct Tag<life s>{{n:i32}}type Handler={signature};fn use_handler(f:Handler){{}}fn main(){{}}"))
                .expect_err("an outer-only relation cannot erase a qualified signature class");
            assert!(
                errors.iter().all(|error| matches!(
                    error.category,
                    zil_base::ResultCategory::Unsupported | zil_base::ResultCategory::SafeReject
                )),
                "{errors:?}"
            );
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
