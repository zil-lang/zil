//! Definition-time Store shapes and modes. These are syntax obligations, not
//! ownership certificates: every instance enters Types and independent Verify.
use super::*;

pub(super) const BOUND: &str = "StoreElement";

pub(super) fn is_bound(ty: &TypeRef) -> bool {
    matches!(&grouped(ty).kind, TypeKind::Named { path, args }
        if path.as_slice() == [BOUND] && args.is_empty())
}

pub(super) fn element(ty: &TypeRef) -> Option<&TypeRef> {
    match &grouped(ty).kind {
        TypeKind::Named { path, args } if path.as_slice() == ["Store"] => match args.as_slice() {
            [GenericArg::Type(element)] => Some(element),
            _ => None,
        },
        _ => None,
    }
}

/// A primitive may describe an actual explicit owner. This unwraps only the
/// written Managed<Store<T>> head; no enclosing Store promotes a bare resource.
pub(super) fn described_element(ty: &TypeRef) -> Option<&TypeRef> {
    element(ty).or_else(|| match &grouped(ty).kind {
        TypeKind::Named { path, args } if path.as_slice() == ["Managed"] => match args.as_slice() {
            [GenericArg::Type(store)] => element(store),
            _ => None,
        },
        _ => None,
    })
}

/// Syntax classification only: an explicit mode must separately establish
/// ownership of this success payload. The union itself remains a Value.
pub(super) fn success(ty: &TypeRef) -> Option<(&TypeRef, &TypeRef)> {
    match &grouped(ty).kind {
        TypeKind::Error { error, result } if element(result).is_some() => Some((error, result)),
        _ => None,
    }
}

pub(super) fn carrier(ty: &TypeRef) -> Option<&TypeRef> {
    if element(ty).is_some() {
        Some(ty)
    } else {
        success(ty).map(|(_, result)| result)
    }
}

/// A union context cannot constrain an arbitrary result variable. Only the
/// written direct Store / Store-success shape determines which context fits.
pub(super) fn call_context<'a>(result: &TypeRef, expected: &'a TypeRef) -> Option<&'a TypeRef> {
    if let Some((_, payload)) = success(expected) {
        if success(result).is_some() {
            Some(expected)
        } else if element(result).is_some() {
            Some(payload)
        } else {
            None
        }
    } else if matches!(grouped(expected).kind, TypeKind::Error { .. })
        && !matches!(grouped(result).kind, TypeKind::Error { .. })
    {
        // The caller may return either payload or the complete union. A
        // concrete payload result, or an unconstrained result variable, must
        // not be unified with the whole union before real arguments bind it.
        // Definition checking still validates the actual returned payload.
        None
    } else {
        Some(expected)
    }
}

pub(super) fn call_name(callee: &Expr) -> Option<&str> {
    match &callee.kind {
        ExprKind::Group(inner) | ExprKind::TypeApply { callee: inner, .. } => call_name(inner),
        _ => super::super::foreign::bare_name(callee),
    }
}

pub(super) fn copy_error(
    ty: &TypeRef,
    nominals: &BTreeMap<String, &Item>,
    budget: &mut Budget<'_>,
) -> R<()> {
    copy::concrete_storage(ty, nominals, budget).map(|_| ())
}

pub(super) fn primitive(name: &str) -> bool {
    matches!(
        name,
        "store_new"
            | "store_reserve"
            | "store_push"
            | "store_pop"
            | "store_len"
            | "store_capacity"
            | "store_at"
            | "store_at_mut"
    )
}

pub(super) fn arity(name: &str) -> usize {
    match name {
        "store_new" => 0,
        "store_reserve" | "store_push" | "store_at" | "store_at_mut" => 2,
        _ => 1,
    }
}

pub(super) fn result(name: &str, element: &TypeRef, span: Span) -> TypeRef {
    let kind = match name {
        "store_new" => TypeKind::Named {
            path: vec!["Store".into()],
            args: vec![GenericArg::Type(element.clone())],
        },
        "store_reserve" => return named("bool", span),
        "store_len" | "store_capacity" => return named("i64", span),
        "store_push" => TypeKind::Error {
            error: Box::new(element.clone()),
            result: Box::new(check::unit(span)),
        },
        "store_pop" => TypeKind::Error {
            error: Box::new(check::unit(span)),
            result: Box::new(element.clone()),
        },
        "store_at" | "store_at_mut" => TypeKind::Reference {
            mutable: name == "store_at_mut",
            lifetime: None,
            inner: Box::new(element.clone()),
        },
        _ => unreachable!("known Store primitive"),
    };
    TypeRef { kind, span }
}

/// Source result modes are deliberately finite. No arbitrary contract is
/// accepted merely because the function happens to mention a Store.
pub(super) fn explicit_task_result(ty: &TypeRef) -> bool {
    matches!(&grouped(ty).kind,
        TypeKind::Named { path, args }
            if path.as_slice() == ["Task"]
                && matches!(args.as_slice(), [GenericArg::Type(_), GenericArg::Type(_)]))
}
pub(super) fn result_mode(function: &Function) -> R<zil_ir::PassingMode> {
    use zil_ir::PassingMode;
    let mut mode = PassingMode::Infer;
    for clause in &function.contract {
        if matches!(clause.name.as_str(), "requires" | "provides" | "resume")
            && matches!(function.generics.as_slice(),[g] if function.params.iter().any(|p|callable::borrowed_registration(&p.ty,&g.name) || (p.give && callable::owned_registration(&p.ty,&g.name))))
        {
            // The resource contract parser checks the complete DSL later;
            // this pass only determines the independent result passing mode.
            continue;
        }
        let selected = match clause.terms.as_slice() {
            [term]
                if clause.name == "result"
                    && term.type_args.is_empty()
                    && term.arguments.is_empty()
                    && term.index.is_none() =>
            {
                match term.path.as_slice() {
                    [name] if name == "create" => Some(PassingMode::Create),
                    [name] if name == "give" => Some(PassingMode::Give),
                    _ => None,
                }
            }
            _ => None,
        };
        let Some(selected) = selected else {
            return Err(unsupported(
                clause.span,
                "generic contract requires a direct Store result(create/give)",
            ));
        };
        if mode != PassingMode::Infer {
            return Err(error(
                clause.span,
                "generic Store result modes are mutually exclusive",
            ));
        }
        let explicit_task = function
            .return_type
            .as_ref()
            .is_some_and(explicit_task_result);
        if function.return_type.as_ref().and_then(carrier).is_none()
            && function
                .return_type
                .as_ref()
                .and_then(callable::registration_environment)
                .is_none()
            && !explicit_task
        {
            return Err(unsupported(
                clause.span,
                "generic resource result mode requires a direct Store result or an explicit Store success result",
            ));
        }
        mode = selected;
    }
    Ok(mode)
}

pub(super) fn concrete_element(
    ty: &TypeRef,
    nominals: &BTreeMap<String, &Item>,
    budget: &mut Budget<'_>,
    context: (
        &trait_bounds::Bounds,
        &BTreeSet<String>,
        callable::StoreCallbacks<'_>,
    ),
) -> R<()> {
    concrete_element_at(ty, nominals, budget, context, 0)
}

fn concrete_element_at(
    ty: &TypeRef,
    nominals: &BTreeMap<String, &Item>,
    budget: &mut Budget<'_>,
    context: (
        &trait_bounds::Bounds,
        &BTreeSet<String>,
        callable::StoreCallbacks<'_>,
    ),
    depth: usize,
) -> R<()> {
    if depth > 0 {
        budget.node(ty.span, depth)?;
    }
    let mut shape = Shape { nominals, budget };
    let (available, variables, callback) = context;
    let symbolic_candidate = match &grouped(ty).kind {
        TypeKind::Named { path, .. } => nominals.get(&path.join(".")).is_some_and(|item| {
            matches!(&item.kind, ItemKind::Aggregate(aggregate)
                if aggregate.kind == AggregateKind::Struct)
        }),
        TypeKind::Tuple(_) | TypeKind::Array { .. } => true,
        _ => false,
    };
    if check::dependent(ty, variables) && symbolic_candidate {
        if !symbolic_pod(
            &mut shape,
            ty,
            available,
            variables,
            &mut BTreeSet::new(),
            0,
        )? {
            return Err(error(
                ty.span,
                "POD Store elements require nonzero inline layout",
            ));
        }
        return Ok(());
    }
    if matches!(
        grouped(ty).kind,
        TypeKind::Tuple(_) | TypeKind::Array { .. }
    ) {
        let (size, _) = shape.pod(ty, &mut BTreeSet::new(), 0)?;
        if size == 0 {
            return Err(error(
                ty.span,
                "POD Store elements require nonzero inline layout",
            ));
        }
        return Ok(());
    }
    if let TypeKind::Named { path, args } = &grouped(ty).kind
        && path.as_slice() == ["Managed"]
    {
        shape.budget.ty(ty)?;
        let [GenericArg::Type(resource)] = args.as_slice() else {
            return Err(error(
                ty.span,
                "Managed requires one canonical resource argument",
            ));
        };
        if !shape.resource(resource, available, variables, callback, depth + 1)? {
            return Err(error(
                ty.span,
                "StoreElement requires nonzero actual resource storage",
            ));
        }
        return Ok(());
    }
    if let TypeKind::Named { path, args } = &grouped(ty).kind
        && args.is_empty()
        && path.len() == 1
        && matches!(
            path[0].as_str(),
            "bool"
                | "u8"
                | "u16"
                | "u32"
                | "i8"
                | "i16"
                | "isize"
                | "i32"
                | "i64"
                | "u64"
                | "usize"
                | "f32"
                | "f64"
        )
    {
        // The paid POD layout walk also authenticates the written primitive.
        // No symbolic T or same-sized nominal acquires StoreElement here.
        let (size, _) = shape.pod(ty, &mut BTreeSet::new(), 0)?;
        if size == 0 {
            return Err(error(
                ty.span,
                "Store elements require nonzero inline layout",
            ));
        }
        return Ok(());
    }
    let (aggregate, substitutions) = shape.aggregate(ty, AggregateKind::Package, true)?;
    // Reuse the already paid nominal lookup; legacy Package fees stay exact.
    // Only the disjoint concrete Record family enters the additional walk.
    if aggregate.kind == AggregateKind::Struct {
        let (size, _) = shape.pod(ty, &mut BTreeSet::new(), 0)?;
        if size == 0 {
            return Err(error(
                ty.span,
                "POD Store elements require nonzero inline layout",
            ));
        }
        return Ok(());
    }
    // aggregate already charged the complete application before this bounded
    // syntactic classification. F grants neither Copy nor StoreElement: only
    // its direct occurrences in the existing owning State family are admitted.
    let callback = if callback.active(ty) {
        callback
    } else {
        callable::StoreCallbacks::None
    };
    let active = !matches!(callback, callable::StoreCallbacks::None);
    if active {
        shape.budget.ty(ty)?;
    }
    let mut callbacks = 0usize;
    let mut storage = false;
    for field in &aggregate.fields {
        shape.budget.node(field.span, 0)?;
        let field_type = shape.substitute(&field.ty, &substitutions)?;
        if field.manages {
            storage |= shape.resource(&field_type, available, variables, callback, depth + 1)?;
        } else if callback.field(&field_type) {
            shape.budget.ty(&field_type)?;
            callbacks += 1;
            // Concrete environment layout and nonzero size are checked again
            // after F is resolved; a source name is not a layout certificate.
            storage = true;
        } else if active && check::dependent(&field_type, variables) {
            copy::require(&field_type, nominals, available, variables, shape.budget)?;
        } else if !active
            && trait_bounds::plain(&field_type).is_some_and(|name| {
                variables.contains(name)
                    && available
                        .get(name)
                        .is_some_and(|bounds| bounds.iter().any(|bound| bound == BOUND))
            })
        {
            // The declaration holds a complete opaque Value. Its concrete
            // application decides whether that Value has any Drop debt.
            storage = true;
        } else if !active
            && explicit_managed_value_at(
                &field_type,
                shape.nominals,
                shape.budget,
                available,
                variables,
                callback,
                depth + 1,
            )?
        {
            storage = true;
        } else if !active && shape.package_name(&field_type) {
            // The actual nominal application was bounded by substitute. A
            // child-only Product still carries its complete Value.
            storage |= shape.product(&field_type, available, variables, callback, depth + 1)?;
        } else {
            storage |= shape.copy(&field_type)?;
        }
    }
    if active && !(1..=2).contains(&callbacks) {
        return Err(error(
            ty.span,
            "callback Store elements require one or two direct fields of declared callable types",
        ));
    }
    if !storage {
        return Err(error(
            ty.span,
            "StoreElement requires a nonzero complete Package Value",
        ));
    }
    Ok(())
}

fn symbolic_pod(
    shape: &mut Shape<'_, '_, '_, '_>,
    ty: &TypeRef,
    available: &trait_bounds::Bounds,
    variables: &BTreeSet<String>,
    active: &mut BTreeSet<String>,
    depth: usize,
) -> R<bool> {
    shape.budget.node(ty.span, depth)?;
    shape.budget.ty(ty)?;
    if let Some(name) = trait_bounds::plain(ty).filter(|name| variables.contains(*name)) {
        shape.budget.text(name, ty.span)?;
        let bounds = available.get(name).ok_or_else(|| {
            error(
                ty.span,
                "symbolic POD Store field needs Copy and StoreElement bounds",
            )
        })?;
        if !bounds.iter().any(|bound| bound == copy::BOUND)
            || !bounds.iter().any(|bound| bound == BOUND)
        {
            return Err(error(
                ty.span,
                "symbolic POD Store field needs Copy and StoreElement bounds",
            ));
        }
        return Ok(true);
    }
    if !check::dependent(ty, variables) {
        return shape.pod(ty, active, depth).map(|(size, _)| size != 0);
    }
    match &grouped(ty).kind {
        TypeKind::Tuple(fields) => {
            let mut nonzero = false;
            for field in fields {
                nonzero |= symbolic_pod(shape, field, available, variables, active, depth + 1)?;
            }
            Ok(nonzero)
        }
        TypeKind::Array {
            element,
            length: Some(length),
        } => {
            let count = array_length(length)?;
            let nonzero = symbolic_pod(shape, element, available, variables, active, depth + 1)?;
            Ok(count != 0 && nonzero)
        }
        TypeKind::Group(inner) => {
            symbolic_pod(shape, inner, available, variables, active, depth + 1)
        }
        TypeKind::Named { .. } => {
            let key = type_key(ty, shape.budget)?;
            shape.budget.identity_work(
                key.len()
                    .saturating_add(1)
                    .saturating_mul(active.len().saturating_add(2)),
                ty.span,
            )?;
            if !active.insert(key.clone()) {
                return Err(error(
                    ty.span,
                    "recursive symbolic POD Store element layout",
                ));
            }
            let (aggregate, substitutions) = shape.aggregate(ty, AggregateKind::Struct, false)?;
            let mut nonzero = false;
            for field in &aggregate.fields {
                shape.budget.node(field.span, depth + 1)?;
                if field.manages {
                    return Err(error(field.span, "POD Store field cannot own resources"));
                }
                let actual = shape.substitute(&field.ty, &substitutions)?;
                nonzero |= symbolic_pod(shape, &actual, available, variables, active, depth + 1)?;
            }
            shape
                .budget
                .identity_work(key.len().saturating_add(1), ty.span)?;
            active.remove(&key);
            Ok(nonzero)
        }
        _ => Err(error(
            ty.span,
            "symbolic POD Store element requires finite numeric Records, tuples or arrays",
        )),
    }
}

pub(super) fn explicit_managed_value(
    ty: &TypeRef,
    nominals: &BTreeMap<String, &Item>,
    budget: &mut Budget<'_>,
) -> R<bool> {
    explicit_managed_value_at(
        ty,
        nominals,
        budget,
        &trait_bounds::Bounds::new(),
        &BTreeSet::new(),
        callable::StoreCallbacks::None,
        0,
    )
}

#[allow(clippy::too_many_arguments)]
fn explicit_managed_value_at(
    ty: &TypeRef,
    nominals: &BTreeMap<String, &Item>,
    budget: &mut Budget<'_>,
    available: &trait_bounds::Bounds,
    variables: &BTreeSet<String>,
    callback: callable::StoreCallbacks<'_>,
    depth: usize,
) -> R<bool> {
    let TypeKind::Named { path, args } = &grouped(ty).kind else {
        return Ok(false);
    };
    if path.as_slice() != ["Managed"] {
        return Ok(false);
    }
    budget.ty(ty)?;
    let [GenericArg::Type(resource)] = args.as_slice() else {
        return Err(error(
            ty.span,
            "Managed requires one canonical resource argument",
        ));
    };
    let mut shape = Shape { nominals, budget };
    // The resource category and actual finite payload are checked; zero-size
    // owners still carry their real Drop responsibility in ordinary Enums.
    shape.resource(resource, available, variables, callback, depth + 1)?;
    Ok(true)
}

struct Shape<'s, 'b, 'n, 'l> {
    nominals: &'s BTreeMap<String, &'n Item>,
    budget: &'b mut Budget<'l>,
}

impl<'n> Shape<'_, '_, 'n, '_> {
    fn package_name(&self, ty: &TypeRef) -> bool {
        matches!(&grouped(ty).kind, TypeKind::Named { path, .. }
            if matches!(self.nominals.get(&path.join(".")),
                Some(Item { kind: ItemKind::Aggregate(a), .. }) if a.kind == AggregateKind::Package))
    }

    fn product(
        &mut self,
        ty: &TypeRef,
        available: &trait_bounds::Bounds,
        variables: &BTreeSet<String>,
        callback: callable::StoreCallbacks<'_>,
        depth: usize,
    ) -> R<bool> {
        // A recursive by-value application eventually reaches the same depth
        // bound as the source type resolver; no unbounded host recursion.
        self.budget.node(ty.span, depth)?;
        let (aggregate, substitutions) = self.aggregate(ty, AggregateKind::Package, false)?;
        let mut storage = false;
        for field in &aggregate.fields {
            self.budget.node(field.span, depth)?;
            let field_type = self.substitute(&field.ty, &substitutions)?;
            if field.manages {
                storage |= self.resource(&field_type, available, variables, callback, depth + 1)?;
            } else if trait_bounds::plain(&field_type).is_some_and(|name| {
                variables.contains(name)
                    && available
                        .get(name)
                        .is_some_and(|bounds| bounds.iter().any(|bound| bound == BOUND))
            }) || explicit_managed_value_at(
                &field_type,
                self.nominals,
                self.budget,
                available,
                variables,
                callback,
                depth + 1,
            )? {
                storage = true;
            } else if self.package_name(&field_type) {
                storage |= self.product(&field_type, available, variables, callback, depth + 1)?;
            } else {
                storage |= self.copy(&field_type)?;
            }
        }
        Ok(storage)
    }

    fn substitute(
        &mut self,
        ty: &TypeRef,
        substitutions: &BTreeMap<String, TypeRef>,
    ) -> R<TypeRef> {
        self.budget.substituted_type(ty, substitutions)?;
        let mut result = ty.clone();
        Substitution { substitutions }.ty(&mut result);
        Ok(result)
    }

    fn aggregate(
        &mut self,
        ty: &TypeRef,
        kind: AggregateKind,
        allow_pod: bool,
    ) -> R<(&'n Aggregate, BTreeMap<String, TypeRef>)> {
        self.budget.ty(ty)?;
        let TypeKind::Named { path, args } = &grouped(ty).kind else {
            return Err(error(
                ty.span,
                "StoreElement requires a complete nominal Package",
            ));
        };
        let Some(Item {
            kind: ItemKind::Aggregate(aggregate),
            ..
        }) = self.nominals.get(&path.join("."))
        else {
            return Err(error(
                ty.span,
                "StoreElement requires a complete nominal Package or declared resource",
            ));
        };
        if (aggregate.kind != kind && !(allow_pod && aggregate.kind == AggregateKind::Struct))
            || aggregate.generics.len() != args.len()
        {
            return Err(error(
                ty.span,
                "StoreElement requires the declared Package/resource category and type arguments",
            ));
        }
        let mut substitutions = BTreeMap::new();
        for (parameter, argument) in aggregate.generics.iter().zip(args) {
            let GenericArg::Type(ty) = argument else {
                return Err(unsupported(ty.span, "StoreElement constant type argument"));
            };
            self.budget.ty(ty)?;
            substitutions.insert(parameter.name.clone(), ty.clone());
        }
        Ok((aggregate, substitutions))
    }

    fn resource(
        &mut self,
        ty: &TypeRef,
        available: &trait_bounds::Bounds,
        variables: &BTreeSet<String>,
        callback: callable::StoreCallbacks<'_>,
        depth: usize,
    ) -> R<bool> {
        if let Some(child) = element(ty) {
            self.budget.node(ty.span, depth)?;
            if trait_bounds::plain(child).is_some_and(|name| {
                variables.contains(name)
                    && available
                        .get(name)
                        .is_some_and(|bounds| bounds.iter().any(|bound| bound == BOUND))
            }) {
                self.budget.ty(child)?;
            } else {
                concrete_element_at(
                    child,
                    self.nominals,
                    self.budget,
                    (available, variables, callback),
                    depth + 1,
                )?;
            }
            return Ok(true);
        }
        if trait_bounds::plain(ty) == Some("ByteBuffer") {
            self.budget.ty(ty)?;
            return Ok(true);
        }
        let (aggregate, substitutions) = self.aggregate(ty, AggregateKind::Resource, false)?;
        let mut storage = false;
        for field in &aggregate.fields {
            if field.manages {
                return Err(error(
                    field.span,
                    "StoreElement resource payload must be finite Copy",
                ));
            }
            let ty = self.substitute(&field.ty, &substitutions)?;
            storage |= self.copy(&ty)?;
        }
        Ok(storage)
    }

    /// Definition-time POD shape and x64 layout. Both supported Store targets
    /// have 64-bit usize; the independent kernels recheck the actual target.
    fn pod(&mut self, ty: &TypeRef, active: &mut BTreeSet<String>, depth: usize) -> R<(u64, u64)> {
        self.budget.node(ty.span, depth)?;
        self.budget.ty(ty)?;
        let invalid = || {
            error(
                ty.span,
                "POD Store fields require finite numeric Records, tuples or arrays",
            )
        };
        let bounded = |n: Option<u64>| {
            n.filter(|n| *n <= i64::MAX as u64)
                .ok_or_else(|| error(ty.span, "POD Store layout exceeds target address range"))
        };
        let align = |n: u64, a: u64| bounded(n.checked_add(a - 1)).map(|n| n / a * a);
        let fields =
            |this: &mut Self, fields: &[TypeRef], active: &mut BTreeSet<String>| -> R<(u64, u64)> {
                let (mut size, mut maximum) = (0u64, 1u64);
                for field in fields {
                    let (bytes, alignment) = this.pod(field, active, depth + 1)?;
                    size = bounded(align(size, alignment)?.checked_add(bytes))?;
                    maximum = maximum.max(alignment);
                }
                Ok((align(size, maximum)?, maximum))
            };
        match &grouped(ty).kind {
            TypeKind::Tuple(ts) => fields(self, ts, active),
            TypeKind::Array {
                element,
                length: Some(length),
            } => {
                let count = array_length(length)?;
                let (size, alignment) = self.pod(element, active, depth + 1)?;
                Ok((bounded(size.checked_mul(u64::from(count)))?, alignment))
            }
            TypeKind::Named { path, args } => {
                if args.is_empty() && path.len() == 1 {
                    match path[0].as_str() {
                        "void" => return Ok((0, 1)),
                        "bool" | "u8" => return Ok((1, 1)),
                        "u16" => return Ok((2, 2)),
                        "i32" | "f32" => return Ok((4, 4)),
                        "i64" | "u64" | "usize" | "f64" => return Ok((8, 8)),
                        _ => {}
                    }
                }
                let key = type_key(ty, self.budget)?;
                // The ordered set comparison reads the complete key. Reserve
                // that work and the key copy before insertion.
                self.budget.identity_work(
                    key.len()
                        .saturating_add(1)
                        .saturating_mul(active.len().saturating_add(2)),
                    ty.span,
                )?;
                if !active.insert(key.clone()) {
                    return Err(error(ty.span, "recursive POD Store element layout"));
                }
                let (aggregate, substitutions) =
                    self.aggregate(ty, AggregateKind::Struct, false)?;
                let (mut size, mut maximum) = (0u64, 1u64);
                for field in &aggregate.fields {
                    self.budget.node(field.span, depth + 1)?;
                    if field.manages {
                        return Err(invalid());
                    }
                    let actual = self.substitute(&field.ty, &substitutions)?;
                    let (bytes, alignment) = self.pod(&actual, active, depth + 1)?;
                    size = bounded(align(size, alignment)?.checked_add(bytes))?;
                    maximum = maximum.max(alignment);
                }
                self.budget.identity_work(
                    key.len()
                        .saturating_add(1)
                        .saturating_mul(active.len().saturating_add(1)),
                    ty.span,
                )?;
                active.remove(&key);
                Ok((align(size, maximum)?, maximum))
            }
            _ => Err(invalid()),
        }
    }

    fn copy(&mut self, ty: &TypeRef) -> R<bool> {
        copy::concrete_storage(ty, self.nominals, self.budget)
    }
}
