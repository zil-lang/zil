//! A deliberately closed parametric admission check. Unconstrained T is not
//! presumed numeric, Copy, or equipped with members because one instance is.
use super::*;
use loop_control::BlockFlow;
#[path = "generic_defer.rs"]
mod deferred;
#[path = "generic_factory.rs"]
mod factory;

pub(super) struct ParametricTables<'a, 'l> {
    pub nominals: &'a BTreeMap<String, &'l Item>,
    pub enums: &'a BTreeMap<String, &'l EnumDecl>,
    pub signatures: &'a BTreeMap<String, Cow<'l, Function>>,
    pub foreign_signatures: &'a BTreeMap<&'l str, (&'l str, &'l Function)>,
    pub pattern_types: &'a BTreeMap<String, TypeRef>,
    pub traits: &'a trait_bounds::Index<'l>,
}

pub(super) fn validate_parametric<'l>(
    function: &Function,
    tables: ParametricTables<'_, 'l>,
    dependencies: &mut helper_graph::Dependencies,
    budget: &mut Budget<'l>,
) -> R<()> {
    let ParametricTables {
        nominals,
        enums,
        signatures,
        foreign_signatures,
        pattern_types,
        traits,
    } = tables;
    // Validate every written signature even if the template remains unused.
    // Type keys reject unresolved lifetime/const forms rather than erasing them.
    let mut parameter_names = BTreeSet::new();
    let variables = function.generics.iter().map(|p| p.name.clone()).collect();
    let callable = callable::template(function)?;
    let owned_factory = callable.as_ref().is_some_and(|bounds| bounds.owned_factory);
    let owned_dispatch = function.generics.len() == 1 && callable.as_ref().is_some_and(|bounds| {
        !bounds.owned_state && !bounds.owned_factory && bounds.state_bindings.is_empty()
            && matches!(bounds.roles.as_slice(), [role]
                if (role.kind == zil_ir::CallableKind::FnOnce || bounds.forwarded)
                    && callable::parameter_mode(&function.params[role.index].ty, &role.generic_name) == Some(None))
    });
    for p in &function.params {
        if super::super::primitive_name(&p.name) {
            return Err(super::super::primitive_name_error(&p.name, p.span));
        }
        if !parameter_names.insert(&p.name) {
            return Err(error(p.span, "duplicate generic function parameter"));
        }
        type_key(&p.ty, budget)?;
        super::super::foreign::callback_type_tree(&p.ty, &variables, budget, 0)?;
    }
    if let Some(t) = &function.return_type {
        if owned_factory {
            budget.ty(t)?;
        } else {
            type_key(t, budget)?;
        }
        super::super::foreign::callback_type_tree(t, &variables, budget, 0)?;
    }
    if callable.is_some() {
        dependencies.register(&function.name, function.span, budget)?;
    }
    let mut check = Check {
        variables,
        callable,
        owned_dispatch,
        definition: &function.name,
        dependencies,
        bounds: traits.bounds(function, budget)?,
        traits,
        next_local: function.params.len(),
        enum_resource_flow: false,
        resource_desc_roots: false,
        resource_root_owners: Vec::new(),
        package_slots: BTreeMap::new(),
        loops: Vec::new(),
        aliases: state_aliases::Tracker::default(),
        factory_scopes: Vec::new(),
        factory_values: BTreeMap::new(),
        factory_return_identity: None,
        nominals,
        enums,
        signatures,
        foreign_signatures,
        unsafe_scope: false,
        defer_depth: 0,
        defer_body: false,
        defer_captures: None,
        pattern_types,
        result_mode: stores::result_mode(function)?,
        result: function
            .return_type
            .clone()
            .unwrap_or_else(|| unit(function.span)),
        budget,
    };
    // Mixed Copy-T templates can disappear without a concrete instantiation.
    // Check the new data boundary here rather than relying on a later Core
    // instance. Ordinary single-F and existing State paths keep their fees.
    if function.generics.iter().any(|parameter| matches!(&parameter.kind,
        GenericParamKind::Type(bounds) if matches!(bounds.as_slice(), [bound] if copy::is_bound(bound))))
        && check
            .callable
            .as_ref()
            .is_some_and(|b| !b.owned_state && !b.owned_factory)
    {
        for generic in &function.generics {
            let GenericParamKind::Type(bounds) = &generic.kind else {
                continue;
            };
            let [bound] = bounds.as_slice() else { continue };
            let TypeKind::Named { path, args } = &bound.kind else {
                continue;
            };
            if !matches!(path.as_slice(), [name] if matches!(name.as_str(), "Fn" | "FnMut" | "FnOnce"))
            {
                continue;
            }
            let [GenericArg::Type(parameters), GenericArg::Type(result)] = args.as_slice() else {
                continue;
            };
            let TypeKind::Tuple(parameters) = &parameters.kind else {
                continue;
            };
            for ty in parameters.iter().chain(std::iter::once(result)).chain(function.return_type.iter()) {
                if matches!(group(ty).kind, TypeKind::Reference { .. })
                    || trait_bounds::plain(ty).is_some_and(|name| check.bounds.get(name)
                        .is_some_and(|bounds| bounds.iter().any(|bound| bound == copy::BOUND)))
                    || known_copy(ty, &check.variables, check.nominals) == Some(true)
                {
                    continue;
                }
                if !check.known_copy(ty)? {
                    return Err(unsupported(
                        ty.span,
                        "owned callback signatures require one callable type and no ordinary type parameters",
                    ));
                }
            }
        }
    }
    if check
        .callable
        .as_ref()
        .is_some_and(|bounds| bounds.owned_factory)
    {
        check.factory_header(function)?;
    }
    if let Some(profile) = check.callable.as_ref().and_then(|b| b.state_replacement) {
        check.state_replacement_header(function, profile)?;
    }
    if let Some(bounds) = check.callable.as_ref().filter(|b| b.dual_names().is_some()) {
        for binding in bounds.bindings() {
            copy::require(
                &binding.result,
                check.nominals,
                &check.bounds,
                &check.variables,
                check.budget,
            )?;
        }
    }
    let mut state = State {
        locals: function
            .params
            .iter()
            .enumerate()
            .map(|(id, p)| {
                (
                    p.name.clone(),
                    Local {
                        id,
                        ty: p.ty.clone(),
                        mutable: false,
                        owned_resource: false,
                        declaration_resource: false,
                        description_roots: None,
                        description_reference_depth: None,
                    },
                )
            })
            .collect(),
        moved: BTreeSet::new(),
        store_owners: BTreeSet::new(),
        store_results: BTreeSet::new(),
        store_origins: BTreeMap::new(),
        invalidated_stores: BTreeSet::new(),
        fresh_stores: BTreeSet::new(),
        aliases: None,
        defer_holds: BTreeSet::new(),
    };
    for (id, parameter) in function.params.iter().enumerate() {
        check.traits.store_types(
            &parameter.ty,
            &check.bounds,
            &check.variables,
            check.budget,
            check.callable.as_ref().map_or(
                callable::StoreCallbacks::None,
                callable::CallableTemplates::store_callbacks,
            ),
        )?;
        if stores::element(&parameter.ty).is_some() {
            state.store_origins.insert(id, BTreeSet::from([id]));
            if parameter.give {
                state.store_owners.insert(id);
            }
        } else if let Some((error, _)) = stores::success(&parameter.ty) {
            stores::copy_error(error, check.nominals, check.budget)?;
            if !parameter.give {
                return Err(unsupported(
                    parameter.span,
                    "generic Store success parameter requires an explicit give payload mode",
                ));
            }
            state.store_results.insert(id);
        } else if parameter.give && !check.concrete_helper_resource(&parameter.ty) {
            return Err(unsupported(
                parameter.span,
                "generic give parameter requires a direct Store carrier or a concrete resource in a callable helper",
            ));
        } else if parameter.give
            && (check.two_input_state_constructor()
                || check
                    .callable
                    .as_ref()
                    .is_some_and(|bounds| bounds.owned_factory))
        {
            state
                .locals
                .get_mut(&parameter.name)
                .expect("parameter bound")
                .owned_resource = true;
        }
    }
    check.traits.store_types(
        &check.result,
        &check.bounds,
        &check.variables,
        check.budget,
        check.callable.as_ref().map_or(
            callable::StoreCallbacks::None,
            callable::CallableTemplates::store_callbacks,
        ),
    )?;
    if let Some((error, _)) = stores::success(&check.result) {
        stores::copy_error(error, check.nominals, check.budget)?;
    }
    if stores::carrier(&check.result).is_some() && check.result_mode == zil_ir::PassingMode::Infer {
        return Err(unsupported(
            function.span,
            "generic Store result requires result(create/give)",
        ));
    }
    match &function.body {
        FunctionBody::Block(body) => {
            if check.block(body, &mut state)?.fallthrough {
                same(&unit(body.span), &check.result, body.span, check.budget)?;
            }
        }
        FunctionBody::Expression(value) => {
            let result = check.result.clone();
            let actual = check.return_expression(value, &mut state)?;
            if owned_factory {
                check.factory_same_result(&actual, &result, value.span)?;
            } else if check.callable.is_some()
                || matches!(group(&result).kind, TypeKind::Error { .. })
            {
                same_return(&actual, &result, value.span, check.budget)?;
            } else {
                same(&actual, &result, value.span, check.budget)?;
            }
        }
        FunctionBody::Declaration => {
            return Err(unsupported(
                function.span,
                "generic declaration without body",
            ));
        }
    }
    Ok(())
}

pub(super) fn unit(span: Span) -> TypeRef {
    TypeRef {
        span,
        kind: TypeKind::Tuple(vec![]),
    }
}
fn same(actual: &TypeRef, expected: &TypeRef, span: Span, budget: &mut Budget<'_>) -> R<()> {
    if type_key(actual, budget)? == type_key(expected, budget)? {
        Ok(())
    } else {
        Err(error(
            span,
            "generic body types must agree before concrete instantiation",
        ))
    }
}
fn same_return(actual: &TypeRef, expected: &TypeRef, span: Span, budget: &mut Budget<'_>) -> R<()> {
    let actual_key = type_key(actual, budget)?;
    if actual_key == type_key(expected, budget)? {
        return Ok(());
    }
    if let TypeKind::Error {
        error: error_type,
        result,
    } = &group(expected).kind
    {
        let error_match = actual_key == type_key(error_type, budget)?;
        let result_match = actual_key == type_key(result, budget)?;
        if error_match != result_match {
            return Ok(());
        }
        if error_match {
            return Err(error(span, "error union payload matches both sides"));
        }
    }
    same(actual, expected, span, budget)
}
fn substituted_type(
    ty: &TypeRef,
    substitutions: &BTreeMap<String, TypeRef>,
    budget: &mut Budget<'_>,
) -> R<TypeRef> {
    budget.substituted_type(ty, substitutions)?;
    let mut result = ty.clone();
    Substitution { substitutions }.ty(&mut result);
    Ok(result)
}
pub(super) fn integer(ty: &TypeRef) -> bool {
    matches!(&group(ty).kind, TypeKind::Named { path, args } if args.is_empty() && matches!(path.as_slice(), [name] if matches!(name.as_str(), "u8" | "u16" | "i32" | "i64")))
}
// An unused template cannot erase an out-of-range literal or an explicit
// suffix mismatch. Concrete instances still enter ordinary Types and Verify.
fn checked_integer(
    text: &str,
    expected: Option<&TypeRef>,
    negative: bool,
    span: Span,
) -> R<(TypeRef, i64)> {
    let clean = text.replace('_', "");
    let suffix = ["u8", "u16", "i32", "i64"]
        .into_iter()
        .find(|name| clean.ends_with(name));
    let expected_name = expected.and_then(|ty| match &group(ty).kind {
        TypeKind::Named { path, args } if args.is_empty() && path.len() == 1 => {
            Some(path[0].as_str())
        }
        _ => None,
    });
    let name = suffix.unwrap_or(match expected_name {
        Some("u8") => "u8",
        Some("u16") => "u16",
        Some("i64") => "i64",
        _ => "i32",
    });
    let digits = suffix.map_or(clean.as_str(), |suffix| clean.strip_suffix(suffix).unwrap());
    let (radix, digits) = if let Some(v) = digits.strip_prefix("0x") {
        (16, v)
    } else if let Some(v) = digits.strip_prefix("0b") {
        (2, v)
    } else if let Some(v) = digits.strip_prefix("0o") {
        (8, v)
    } else {
        (10, digits)
    };
    let value = i128::from_str_radix(digits, radix)
        .map_err(|_| error(span, "generic integer literal is invalid or too large"))?;
    let value = if negative { -value } else { value };
    let value = match name {
        "u8" => u8::try_from(value)
            .map(i64::from)
            .map_err(|_| error(span, "generic integer literal does not fit u8"))?,
        "u16" => u16::try_from(value)
            .map(i64::from)
            .map_err(|_| error(span, "generic integer literal does not fit u16"))?,
        "i32" => i32::try_from(value)
            .map(i64::from)
            .map_err(|_| error(span, "generic integer literal does not fit i32"))?,
        _ => i64::try_from(value)
            .map_err(|_| error(span, "generic integer literal does not fit i64"))?,
    };
    Ok((named(name, span), value))
}
fn boolean(ty: &TypeRef) -> bool {
    matches!(&group(ty).kind, TypeKind::Named { path, args } if args.is_empty() && path.as_slice() == ["bool"])
}
fn group(ty: &TypeRef) -> &TypeRef {
    match &ty.kind {
        TypeKind::Group(t) => group(t),
        _ => ty,
    }
}
pub(super) fn dependent(ty: &TypeRef, vars: &BTreeSet<String>) -> bool {
    match &ty.kind {
        TypeKind::Named { path, args } => {
            path.len() == 1 && vars.contains(&path[0])
                || args
                    .iter()
                    .any(|a| matches!(a, GenericArg::Type(t) if dependent(t, vars)))
        }
        TypeKind::Reference { inner, .. } | TypeKind::Group(inner) | TypeKind::Optional(inner) => {
            dependent(inner, vars)
        }
        TypeKind::Tuple(ts) => ts.iter().any(|t| dependent(t, vars)),
        TypeKind::Array { element, .. } => dependent(element, vars),
        TypeKind::Error { error, result } => dependent(error, vars) || dependent(result, vars),
        TypeKind::Function { params, result }
        | TypeKind::ForeignFunction { params, result, .. } => {
            params.iter().any(|p| dependent(p, vars)) || dependent(result, vars)
        }
        _ => false,
    }
}
fn known_copy(
    ty: &TypeRef,
    vars: &BTreeSet<String>,
    nominals: &BTreeMap<String, &Item>,
) -> Option<bool> {
    Some(match &group(ty).kind {
        // Source Store values are descriptions. The real carrier can only
        // leave an owning position through an explicitly checked give.
        TypeKind::Named { path, .. } if path.as_slice() == ["Store"] => true,
        TypeKind::Reference { mutable: false, .. }
        | TypeKind::Function { .. }
        | TypeKind::ForeignFunction { .. } => true,
        TypeKind::Reference { mutable: true, .. } => false,
        // Opaque callable bounds expose an invocation interface, not Copy.
        TypeKind::Impl(_) => false,
        TypeKind::Tuple(ts) => {
            for t in ts {
                if !known_copy(t, vars, nominals)? {
                    return Some(false);
                }
            }
            true
        }
        TypeKind::Array { element, .. } => known_copy(element, vars, nominals)?,
        TypeKind::Error { result, .. } if stores::element(result).is_some() => false,
        TypeKind::Error { error, result } => {
            known_copy(error, vars, nominals)? && known_copy(result, vars, nominals)?
        }
        TypeKind::Named { path, .. } => match nominals.get(&path.join(".")) {
            Some(Item {
                kind: ItemKind::Aggregate(a),
                ..
            }) if a.kind == AggregateKind::Package => false,
            // A concrete Enum can now own a complete Product or another Sum.
            // Its spelling cannot establish Copy. Request the paid finite
            // structural proof, including nested and non-manages payloads.
            Some(Item {
                kind: ItemKind::Enum(_),
                ..
            }) => return None,
            _ => !dependent(ty, vars),
        },
        _ => !dependent(ty, vars),
    })
}

#[derive(Clone)]
struct Local {
    id: usize,
    ty: TypeRef,
    mutable: bool,
    // Actual managed Enum fields and the established State/factory producers
    // record ownership. A description never inherits it from a nominal type.
    owned_resource: bool,
    // The type came from a concrete declaration field, outside this helper's
    // binder scope, including through bare descriptions and actual give results.
    declaration_resource: bool,
    // Immutable root sets clone only their reference in branch snapshots.
    description_roots: Option<std::rc::Rc<[usize]>>,
    description_reference_depth: Option<usize>,
}
#[derive(Clone)]
struct State {
    locals: BTreeMap<String, Local>,
    moved: BTreeSet<usize>,
    store_owners: BTreeSet<usize>,
    // These locals hold an ordinary affine union, not a direct Managed Store.
    // Only explicit success modes can establish this fact.
    store_results: BTreeSet<usize>,
    store_origins: BTreeMap<usize, BTreeSet<usize>>,
    invalidated_stores: BTreeSet<usize>,
    // A new symbolic position is not necessarily a newly created resource.
    // Written create producers and transfers of their direct locals or union
    // success payloads carry this fact. Give parameters, give calls and
    // Package field takes do not.
    fresh_stores: BTreeSet<usize>,
    aliases: Option<state_aliases::AliasState>,
    defer_holds: BTreeSet<usize>,
}
struct Check<'s, 'b, 'l> {
    variables: BTreeSet<String>,
    callable: Option<callable::CallableTemplates>,
    owned_dispatch: bool,
    definition: &'s str,
    dependencies: &'b mut helper_graph::Dependencies,
    bounds: trait_bounds::Bounds,
    traits: &'s trait_bounds::Index<'l>,
    next_local: usize,
    enum_resource_flow: bool,
    resource_desc_roots: bool,
    resource_root_owners: Vec<(usize, usize)>,
    package_slots: BTreeMap<(usize, String), usize>,
    loops: Vec<loop_control::LoopFrame>,
    aliases: state_aliases::Tracker,
    factory_scopes: Vec<factory::Scope>,
    factory_values: BTreeMap<usize, Span>,
    factory_return_identity: Option<Span>,
    nominals: &'s BTreeMap<String, &'l Item>,
    enums: &'s BTreeMap<String, &'l EnumDecl>,
    signatures: &'s BTreeMap<String, Cow<'l, Function>>,
    foreign_signatures: &'s BTreeMap<&'l str, (&'l str, &'l Function)>,
    unsafe_scope: bool,
    defer_depth: usize,
    defer_body: bool,
    defer_captures: Option<deferred::Captures>,
    pattern_types: &'s BTreeMap<String, TypeRef>,
    result: TypeRef,
    result_mode: zil_ir::PassingMode,
    budget: &'b mut Budget<'l>,
}
impl Check<'_, '_, '_> {
    /// Ordinary Copy parameters can leave this definition uninstantiated and
    /// absent from Core. Check the complete mixed header here as well as at
    /// the later selector-producing boundary; no concrete instance grants it.
    fn state_replacement_header(
        &mut self,
        function: &Function,
        profile: callable::StateReplacement,
    ) -> R<()> {
        let owner = &function.params[profile.owner].ty;
        let result = function
            .return_type
            .as_ref()
            .expect("mixed complete result");
        same(owner, result, result.span, self.budget)?;
        let TypeKind::Named { path, args } = &group(owner).kind else {
            return Err(unsupported(
                owner.span,
                "State replacement requires a complete Package",
            ));
        };
        self.budget.ty(owner)?;
        let Some(Item {
            kind: ItemKind::Aggregate(aggregate),
            ..
        }) = self.nominals.get(&path.join("."))
        else {
            return Err(unsupported(
                owner.span,
                "State replacement requires a declared Package",
            ));
        };
        if aggregate.kind != AggregateKind::Package || aggregate.generics.len() != args.len() {
            return Err(unsupported(
                owner.span,
                "State replacement requires the complete declared Package type",
            ));
        }
        let mut substitutions = BTreeMap::new();
        for (parameter, argument) in aggregate.generics.iter().zip(args) {
            let GenericArg::Type(ty) = argument else {
                return Err(unsupported(
                    owner.span,
                    "State replacement constant type argument",
                ));
            };
            self.budget.node(ty.span, 0)?;
            self.budget.text(&parameter.name, ty.span)?;
            self.budget.ty(ty)?;
            substitutions.insert(parameter.name.clone(), ty.clone());
        }
        let bounds = self.callable.as_ref().expect("mixed callable scope");
        let bound = bounds
            .bindings()
            .next()
            .expect("mixed callable declaration");
        copy::require(
            &bound.result,
            self.nominals,
            &self.bounds,
            &self.variables,
            self.budget,
        )?;
        let mut callbacks = 0usize;
        for field in &aggregate.fields {
            self.budget.text(&field.name, field.span)?;
            let ty = substituted_type(&field.ty, &substitutions, self.budget)?;
            if callable::parameter_mode(&ty, &bound.generic_name) == Some(None) && !field.manages {
                callbacks += 1;
            } else if callable::refers(&ty, &bound.generic_name) {
                return Err(unsupported(
                    field.span,
                    "State replacement requires direct callback fields",
                ));
            } else if !field.manages {
                copy::require(
                    &ty,
                    self.nominals,
                    &self.bounds,
                    &self.variables,
                    self.budget,
                )?;
            }
        }
        if !(1..=2).contains(&callbacks) {
            return Err(unsupported(
                owner.span,
                "State replacement requires one or two direct fields of its single callable type",
            ));
        }
        for (index, parameter) in function.params.iter().enumerate() {
            if index != profile.owner && index != profile.replacement {
                copy::require(
                    &parameter.ty,
                    self.nominals,
                    &self.bounds,
                    &self.variables,
                    self.budget,
                )?;
            }
        }
        Ok(())
    }
    fn literal_context<'a>(&self, expected: Option<&'a TypeRef>) -> Option<&'a TypeRef> {
        if self
            .callable
            .as_ref()
            .is_some_and(|b| b.dual_names().is_some())
            && let Some(ty) = expected
            && let TypeKind::Error { error, result } = &group(ty).kind
            && integer(error) != integer(result)
        {
            // An integer payload keeps its concrete width even when the other
            // alternative is a complete symbolic State. same_return still
            // validates the original entire union and rejects either-side
            // mismatch; no generic numeric capability is inferred from a call.
            return Some(if integer(error) { error } else { result });
        }
        expected
    }
    fn known_copy(&mut self, ty: &TypeRef) -> R<bool> {
        match known_copy(ty, &self.variables, self.nominals) {
            Some(true) => return Ok(true),
            Some(false) if !dependent(ty, &self.variables) => return Ok(false),
            Some(false) | None => {}
        }
        match copy::require(
            ty,
            self.nominals,
            &self.bounds,
            &self.variables,
            self.budget,
        ) {
            Ok(()) => Ok(true),
            Err(diagnostic) if diagnostic.category == zil_base::ResultCategory::ResourceLimit => {
                Err(diagnostic)
            }
            Err(_) => Ok(false),
        }
    }
    fn concrete_helper_resource(&self, ty: &TypeRef) -> bool {
        if self.callable.is_none() || dependent(ty, &self.variables) {
            return false;
        }
        self.declared_resource(ty)
    }
    fn declared_resource(&self, ty: &TypeRef) -> bool {
        match &group(ty).kind {
            TypeKind::Named { path, args } if path.as_slice() == ["ByteBuffer"] => args.is_empty(),
            TypeKind::Named { path, args } => matches!(self.nominals.get(&path.join(".")),
                Some(Item { kind: ItemKind::Aggregate(resource), .. })
                if resource.kind == AggregateKind::Resource
                    && resource.generics.len() == args.len()
                    && args.iter().all(|arg| matches!(arg, GenericArg::Type(_)))),
            _ => false,
        }
    }
    fn two_input_state_constructor(&self) -> bool {
        self.callable.as_ref().is_some_and(|bounds| {
            bounds.owned_state && bounds.roles.len() == 2 && bounds.state_replacement.is_none()
        })
    }
    fn owned_resource_expression(&mut self, expression: &Expr, state: &State) -> R<bool> {
        self.tick(expression.span)?;
        Ok(match &expression.kind {
            ExprKind::Group(inner) => self.owned_resource_expression(inner, state)?,
            // The expression's actual concrete resource type is checked after
            // evaluation; ordinary records and Package values cannot set it.
            ExprKind::Record {
                ty: Some(_),
                base: None,
                ..
            } => true,
            ExprKind::Unary {
                op: UnaryOp::Give,
                operand,
            } => {
                let mut operand = operand.as_ref();
                while let ExprKind::Group(inner) = &operand.kind {
                    self.tick(operand.span)?;
                    operand = inner;
                }
                match &operand.kind {
                    ExprKind::Name(name) => state
                        .locals
                        .get(name)
                        .is_some_and(|local| local.owned_resource),
                    // managed_field_transfer checks the declared real field.
                    ExprKind::Member { .. } => true,
                    _ => false,
                }
            }
            ExprKind::Call { callee, .. } => stores::call_name(callee).is_some_and(|name| {
                name == "bytes_new"
                    || self.signatures.get(name).is_some_and(|function| {
                        stores::result_mode(function).is_ok_and(|mode| {
                            matches!(
                                mode,
                                zil_ir::PassingMode::Create | zil_ir::PassingMode::Give
                            )
                        })
                    })
            }),
            ExprKind::If {
                then_value,
                else_value,
                ..
            } => {
                self.owned_resource_expression(then_value, state)?
                    && self.owned_resource_expression(else_value, state)?
            }
            _ => false,
        })
    }
    fn tick(&mut self, span: Span) -> R<()> {
        // The borrowed preflight checked full AST depth before any cloning.
        self.budget.node(span, 0)
    }
    fn bind_local(&mut self, state: &mut State, name: String, ty: TypeRef) -> R<()> {
        if super::super::primitive_name(&name) {
            return Err(super::super::primitive_name_error(&name, ty.span));
        }
        let id = self.next_local;
        self.next_local += 1;
        state.locals.insert(
            name,
            Local {
                id,
                ty,
                mutable: false,
                owned_resource: false,
                declaration_resource: false,
                description_roots: None,
                description_reference_depth: None,
            },
        );
        Ok(())
    }
    fn owning_store_expression(&self, expression: &Expr, state: &State) -> bool {
        match &expression.kind {
            ExprKind::Group(inner) => self.owning_store_expression(inner, state),
            ExprKind::Name(name) => state
                .locals
                .get(name)
                .is_some_and(|local| state.store_results.contains(&local.id)),
            ExprKind::Try(inner) => self.owning_store_expression(inner, state),
            ExprKind::Unary {
                op: UnaryOp::Give,
                operand,
            } => match &operand.kind {
                ExprKind::Name(name) => state
                    .locals
                    .get(name)
                    .is_some_and(|local| state.store_owners.contains(&local.id)),
                ExprKind::Member { .. } => true, // The give expression separately checks the actual field.
                ExprKind::Group(inner) => self.owning_store_expression(
                    &Expr {
                        span: expression.span,
                        kind: ExprKind::Unary {
                            op: UnaryOp::Give,
                            operand: inner.clone(),
                        },
                    },
                    state,
                ),
                _ => false,
            },
            ExprKind::Call { callee, .. } => {
                let Some(name) = stores::call_name(callee) else {
                    return false;
                };
                if name == "store_new" {
                    return true;
                }
                self.signatures.get(name).is_some_and(|function| {
                    stores::result_mode(function).is_ok_and(|mode| {
                        matches!(
                            mode,
                            zil_ir::PassingMode::Create | zil_ir::PassingMode::Give
                        )
                    })
                })
            }
            ExprKind::If {
                then_value,
                else_value,
                ..
            } => {
                self.owning_store_expression(then_value, state)
                    && self.owning_store_expression(else_value, state)
            }
            _ => false,
        }
    }
    fn fresh_store_expression(&mut self, expression: &Expr, state: &State) -> R<bool> {
        self.tick(expression.span)?;
        match &expression.kind {
            ExprKind::Group(inner) => self.fresh_store_expression(inner, state),
            ExprKind::Name(name) => Ok(state.locals.get(name).is_some_and(|local| {
                state.store_results.contains(&local.id) && state.fresh_stores.contains(&local.id)
            })),
            ExprKind::Try(inner) => self.fresh_store_expression(inner, state),
            ExprKind::Unary {
                op: UnaryOp::Give,
                operand,
            } => {
                let mut operand = operand.as_ref();
                while let ExprKind::Group(inner) = &operand.kind {
                    self.tick(operand.span)?;
                    operand = inner;
                }
                Ok(matches!(&operand.kind, ExprKind::Name(name)
                    if state.locals.get(name).is_some_and(|local|
                        state.fresh_stores.contains(&local.id))))
            }
            ExprKind::Call { callee, .. } => {
                let Some(name) = stores::call_name(callee) else {
                    return Ok(false);
                };
                Ok(name == "store_new"
                    || self.signatures.get(name).is_some_and(|function| {
                        stores::result_mode(function)
                            .is_ok_and(|mode| mode == zil_ir::PassingMode::Create)
                    }))
            }
            ExprKind::If {
                then_value,
                else_value,
                ..
            } => Ok(self.fresh_store_expression(then_value, state)?
                && self.fresh_store_expression(else_value, state)?),
            _ => Ok(false),
        }
    }
    fn return_expression(&mut self, expression: &Expr, state: &mut State) -> R<TypeRef> {
        // Capture the ownership facts before expression evaluation consumes
        // its source. Looking only at the returned syntax type erases the
        // distinction between a Store carrier and its Copy description.
        let owns = self.owning_store_expression(expression, state);
        let fresh = owns && self.fresh_store_expression(expression, state)?;
        let expected = self.result.clone();
        let actual = self.expr(expression, state, Some(&expected), false)?;
        if self.factory_scopes.is_empty()
            && self
                .callable
                .as_ref()
                .is_some_and(|bounds| bounds.owned_factory)
        {
            self.factory_result_identity(expression, state)?;
        }
        if let Some((error_type, _)) = stores::success(&expected)
            && type_key(&actual, self.budget)? == type_key(error_type, self.budget)?
        {
            // This return injects the Copy error and owns zero success Stores.
            return Ok(actual);
        }
        if stores::carrier(&expected).is_some() && !owns {
            return Err(error(
                expression.span,
                "generic Store result requires an actual create/give value, not a descriptor",
            ));
        }
        if stores::carrier(&expected).is_some()
            && self.result_mode == zil_ir::PassingMode::Create
            && !fresh
        {
            return Err(error(
                expression.span,
                "generic Store create result requires a fresh source on every returning path",
            ));
        }
        Ok(actual)
    }
    fn store_origins(&mut self, expression: &Expr, state: &State) -> R<BTreeSet<usize>> {
        match &expression.kind {
            ExprKind::Name(name) => Ok(state
                .locals
                .get(name)
                .and_then(|local| state.store_origins.get(&local.id))
                .cloned()
                .unwrap_or_default()),
            ExprKind::Group(inner) => self.store_origins(inner, state),
            ExprKind::Member {
                object,
                member: MemberName::Name(member),
            } => {
                if let Some(owner) = self.package_local(object, state) {
                    let slot = self.field_slot(owner, member, expression.span)?;
                    Ok(state
                        .store_origins
                        .get(&slot)
                        .cloned()
                        .unwrap_or_else(|| BTreeSet::from([slot])))
                } else {
                    Ok(BTreeSet::new())
                }
            }
            ExprKind::If {
                then_value,
                else_value,
                ..
            } => {
                let mut origins = self.store_origins(then_value, state)?;
                origins.extend(self.store_origins(else_value, state)?);
                Ok(origins)
            }
            _ => Ok(BTreeSet::new()),
        }
    }
    fn package_local(&self, expression: &Expr, state: &State) -> Option<usize> {
        if let ExprKind::Group(inner) = &expression.kind {
            return self.package_local(inner, state);
        }
        let ExprKind::Name(name) = &expression.kind else {
            return None;
        };
        let local = state.locals.get(name)?;
        let TypeKind::Named { path, .. } = &group(&local.ty).kind else {
            return None;
        };
        matches!(self.nominals.get(&path.join(".")), Some(Item { kind: ItemKind::Aggregate(a), .. })
            if a.kind == AggregateKind::Package)
        .then_some(local.id)
    }
    fn field_slot(&mut self, local: usize, member: &str, span: Span) -> R<usize> {
        self.budget.text(member, span)?;
        let key = (local, member.to_owned());
        if let Some(id) = self.package_slots.get(&key) {
            return Ok(*id);
        }
        let id = self.next_local;
        self.next_local += 1;
        self.package_slots.insert(key, id);
        Ok(id)
    }
    fn existing_field_slot(&mut self, expression: &Expr, state: &State) -> R<Option<usize>> {
        let ExprKind::Member {
            object,
            member: MemberName::Name(member),
        } = &expression.kind
        else {
            return Ok(None);
        };
        let Some(local) = self.package_local(object, state) else {
            return Ok(None);
        };
        self.budget.text(member, expression.span)?;
        Ok(self.package_slots.get(&(local, member.clone())).copied())
    }
    fn managed_field_transfer(&mut self, operand: &Expr, state: &mut State) -> R<TypeRef> {
        let mut direct = operand;
        while let ExprKind::Group(inner) = &direct.kind {
            direct = inner;
        }
        if let ExprKind::Name(name) = &direct.kind
            && let Some(local) = state.locals.get(name)
            && (local.declaration_resource || self.concrete_helper_resource(&local.ty))
            && (local.owned_resource
                || self.two_input_state_constructor()
                || self
                    .callable
                    .as_ref()
                    .is_some_and(|bounds| bounds.owned_factory))
        {
            if !local.owned_resource {
                return Err(error(
                    operand.span,
                    "give requires an actual owned resource; descriptions do not own",
                ));
            }
            self.budget.ty(&local.ty)?;
            let (id, ty, roots) = (local.id, local.ty.clone(), local.description_roots.clone());
            if roots.is_some() {
                self.defer_use(id, true, operand.span)?;
                self.defer_held(state, id, true, operand.span)?;
            }
            self.resource_description_use(roots.as_deref(), state, operand.span)?;
            self.resource_description_invalidate(roots.as_deref(), state, operand.span)?;
            self.factory_use(id, false, true, operand.span)?;
            if !state.moved.insert(id) {
                return Err(error(operand.span, "owned resource used after give"));
            }
            return Ok(ty);
        }
        if let ExprKind::Name(name) = &direct.kind
            && let Some(local) = state.locals.get(name)
            && stores::element(&local.ty).is_some()
        {
            let id = local.id;
            let ty = local.ty.clone();
            self.factory_use(id, false, true, operand.span)?;
            if !state.store_owners.contains(&id) {
                return Err(error(
                    operand.span,
                    "give requires an actual Store carrier; descriptors do not own",
                ));
            }
            if !state.moved.insert(id) {
                return Err(error(operand.span, "Store carrier used after give"));
            }
            if let Some(origins) = state.store_origins.get(&id) {
                state.invalidated_stores.extend(origins);
            }
            self.budget.ty(&ty)?;
            return Ok(ty);
        }
        let ExprKind::Member {
            object,
            member: MemberName::Name(member),
        } = &operand.kind
        else {
            return Err(unsupported(
                operand.span,
                "generic managed give requires a direct Package manages field",
            ));
        };
        let Some(owner) = self.package_local(object, state) else {
            return Err(unsupported(
                operand.span,
                "generic managed give requires a local owning Package",
            ));
        };
        let mut object = object.as_ref();
        while let ExprKind::Group(inner) = &object.kind {
            self.budget.node(object.span, 0)?;
            object = inner;
        }
        let ExprKind::Name(name) = &object.kind else {
            unreachable!()
        };
        let TypeKind::Named { path, .. } = &group(&state.locals[name].ty).kind else {
            unreachable!()
        };
        let Some(Item {
            kind: ItemKind::Aggregate(aggregate),
            ..
        }) = self.nominals.get(&path.join("."))
        else {
            unreachable!()
        };
        for field in &aggregate.fields {
            self.budget.node(field.span, 0)?;
        }
        let declared = aggregate
            .fields
            .iter()
            .find(|field| &field.name == member)
            .ok_or_else(|| error(operand.span, "unknown generic Package field"))?;
        if !declared.manages {
            return Err(error(
                operand.span,
                "generic give requires a declared manages field",
            ));
        }
        // Place resolution checks the real nominal declaration and privacy.
        // It does not grant ownership to a descriptor or a reference carrier.
        let (ty, _) = self.place(operand, state)?;
        if let Some(element) = stores::element(&ty) {
            self.traits.store_element(
                element,
                &self.bounds,
                &self.variables,
                self.budget,
                self.callable.as_ref().map_or(
                    callable::StoreCallbacks::None,
                    callable::CallableTemplates::store_callbacks,
                ),
            )?;
        } else if !self
            .callable
            .as_ref()
            .is_some_and(|bound| bound.owned_state || bound.owned_factory || self.owned_dispatch)
        {
            return Err(unsupported(
                operand.span,
                "generic managed give requires an owning State profile",
            ));
        } else if dependent(&ty, &self.variables) {
            return Err(unsupported(
                operand.span,
                "generic manages field give requires a concrete resource type",
            ));
        }
        let concrete = match &group(&ty).kind {
            TypeKind::Named { path, args } if path.as_slice() == ["Store"] => {
                matches!(args.as_slice(), [GenericArg::Type(_)])
            }
            TypeKind::Named { path, args } if path.as_slice() == ["ByteBuffer"] => args.is_empty(),
            TypeKind::Named { path, args } => matches!(self.nominals.get(&path.join(".")),
                Some(Item { kind: ItemKind::Aggregate(resource), .. })
                if resource.kind == AggregateKind::Resource && resource.generics.len() == args.len()),
            _ => false,
        };
        if !concrete {
            return Err(unsupported(
                operand.span,
                "generic manages field give requires a concrete resource type",
            ));
        }
        let slot = self.field_slot(owner, member, operand.span)?;
        self.factory_field_use(owner, slot, operand.span)?;
        if !state.moved.insert(slot) {
            return Err(error(operand.span, "managed Package field used after give"));
        }
        if stores::element(&ty).is_some() {
            state.invalidated_stores.extend(
                state
                    .store_origins
                    .get(&slot)
                    .cloned()
                    .unwrap_or_else(|| BTreeSet::from([slot])),
            );
        }
        Ok(ty)
    }
    fn clone_state(&mut self, state: &State) -> R<State> {
        self.alias_clone_fee(state, self.result.span)?;
        for local in state.locals.values() {
            self.budget.ty(&local.ty)?;
        }
        for roots in state.store_origins.values() {
            for _ in roots {
                self.budget.node(self.result.span, 0)?;
            }
        }
        for _ in &state.invalidated_stores {
            self.budget.node(self.result.span, 0)?;
        }
        for _ in &state.fresh_stores {
            self.budget.node(self.result.span, 0)?;
        }
        for _ in &state.store_results {
            self.budget.node(self.result.span, 0)?;
        }
        for _ in &state.defer_holds {
            self.budget.node(self.result.span, 0)?;
        }
        if self.resource_desc_roots {
            // Fresh root activations also live in moved; reserve their snapshot
            // before cloning the set. No old profile enters this extra walk.
            for _ in &state.moved {
                self.budget.node(self.result.span, 0)?;
            }
        }
        Ok(state.clone())
    }
    fn join_states(&mut self, state: &mut State, branches: &[&State], span: Span) -> R<()> {
        self.alias_join(state, branches, span)?;
        self.resource_description_join(state, branches, span)?;
        state.moved.clear();
        state.invalidated_stores.clear();
        state.store_origins.clear();
        state.fresh_stores.clear();
        if let Some(first) = branches.first() {
            for id in &first.fresh_stores {
                self.budget.node(span, 0)?;
                let mut fresh_on_every_path = true;
                for branch in &branches[1..] {
                    self.budget.node(span, 0)?;
                    fresh_on_every_path &= branch.fresh_stores.contains(id);
                }
                if fresh_on_every_path {
                    state.fresh_stores.insert(*id);
                }
            }
        }
        for branch in branches {
            for id in &branch.moved {
                self.budget.node(span, 0)?;
                state.moved.insert(*id);
            }
            for id in &branch.invalidated_stores {
                self.budget.node(span, 0)?;
                state.invalidated_stores.insert(*id);
            }
            for (local, roots) in &branch.store_origins {
                for root in roots {
                    self.budget.node(span, 0)?;
                    state.store_origins.entry(*local).or_default().insert(*root);
                }
            }
        }
        Ok(())
    }
    fn replace_store_origin(&mut self, state: &mut State, slot: usize, span: Span) -> R<()> {
        self.budget.node(span, 0)?;
        state.invalidated_stores.extend(
            state
                .store_origins
                .get(&slot)
                .cloned()
                .unwrap_or_else(|| BTreeSet::from([slot])),
        );
        let fresh = self.next_local;
        self.next_local += 1;
        state.store_origins.insert(slot, BTreeSet::from([fresh]));
        Ok(())
    }
    fn loop_invariant(&mut self, before: &State, after: &State, span: Span) -> R<()> {
        for local in before.locals.values() {
            self.budget.node(span, 0)?;
            if before.moved.contains(&local.id) != after.moved.contains(&local.id)
                || before.store_origins.get(&local.id) != after.store_origins.get(&local.id)
                || before.fresh_stores.contains(&local.id) != after.fresh_stores.contains(&local.id)
            {
                return Err(unsupported(
                    span,
                    "generic loop must preserve outer carrier and non-Copy consumption state",
                ));
            }
        }
        for ((owner, _), field) in &self.package_slots {
            self.budget.node(span, 0)?;
            if before.locals.values().any(|local| local.id == *owner)
                && (before.moved.contains(field) != after.moved.contains(field)
                    || before.store_origins.get(field) != after.store_origins.get(field)
                    || after.invalidated_stores.contains(field)
                        && !before.invalidated_stores.contains(field))
            {
                return Err(unsupported(
                    span,
                    "generic loop cannot move or replace an outer managed field",
                ));
            }
        }
        Ok(())
    }

    // This lookup supplies only contextual integer widths. It never evaluates
    // an endpoint, consumes a local or constructs a symbolic field position.
    // The ordinary expression check below still validates every operation.
    fn range_hint(&mut self, expression: &Expr, state: &State, depth: usize) -> R<Option<TypeRef>> {
        self.budget.node(expression.span, depth)?;
        let span = expression.span;
        match &expression.kind {
            ExprKind::Name(name) => {
                let Some(local) = state.locals.get(name) else {
                    return Ok(None);
                };
                self.budget.ty(&local.ty)?;
                Ok(Some(local.ty.clone()))
            }
            ExprKind::Literal(Literal::Integer(text)) => {
                self.budget.text(text, span)?;
                Ok(["u8", "u16", "i32", "i64"]
                    .into_iter()
                    .find(|suffix| text.ends_with(suffix))
                    .map(|suffix| named(suffix, span)))
            }
            ExprKind::Literal(Literal::Bool(_)) => Ok(Some(named("bool", span))),
            ExprKind::Group(inner) | ExprKind::MetaEval(inner) => {
                self.range_hint(inner, state, depth + 1)
            }
            ExprKind::Unary {
                op: UnaryOp::Not, ..
            } => Ok(Some(named("bool", span))),
            ExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => {
                let Some(ty) = self.range_hint(operand, state, depth + 1)? else {
                    return Ok(None);
                };
                let TypeKind::Reference { inner, .. } = &group(&ty).kind else {
                    return Ok(None);
                };
                self.budget.ty(inner)?;
                Ok(Some(*inner.clone()))
            }
            ExprKind::Unary { operand, .. } => self.range_hint(operand, state, depth + 1),
            ExprKind::Binary {
                op:
                    BinaryOp::Equal
                    | BinaryOp::NotEqual
                    | BinaryOp::Less
                    | BinaryOp::LessEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual
                    | BinaryOp::And
                    | BinaryOp::Or,
                ..
            } => Ok(Some(named("bool", span))),
            ExprKind::Binary { left, right, .. } => {
                if let Some(ty) = self.range_hint(left, state, depth + 1)? {
                    Ok(Some(ty))
                } else {
                    self.range_hint(right, state, depth + 1)
                }
            }
            ExprKind::If {
                then_value,
                else_value,
                ..
            } => {
                if let Some(ty) = self.range_hint(then_value, state, depth + 1)? {
                    Ok(Some(ty))
                } else {
                    self.range_hint(else_value, state, depth + 1)
                }
            }
            ExprKind::Try(operand) => {
                let Some(ty) = self.range_hint(operand, state, depth + 1)? else {
                    return Ok(None);
                };
                let TypeKind::Error { result, .. } = &group(&ty).kind else {
                    return Ok(None);
                };
                self.budget.ty(result)?;
                Ok(Some(*result.clone()))
            }
            ExprKind::Index { object, .. } | ExprKind::Member { object, .. } => {
                let Some(ty) = self.range_hint(object, state, depth + 1)? else {
                    return Ok(None);
                };
                let base = match &group(&ty).kind {
                    TypeKind::Reference { inner, .. } => group(inner),
                    _ => group(&ty),
                };
                let field = match (&expression.kind, &base.kind) {
                    (ExprKind::Index { .. }, TypeKind::Array { element, .. }) => {
                        self.budget.ty(element)?;
                        return Ok(Some(*element.clone()));
                    }
                    (
                        ExprKind::Member {
                            member: MemberName::TupleIndex(index),
                            ..
                        },
                        TypeKind::Tuple(fields),
                    ) => fields.get(*index as usize),
                    (
                        ExprKind::Member {
                            member: MemberName::Name(member),
                            ..
                        },
                        TypeKind::Named { path, args },
                    ) => {
                        for part in path {
                            self.budget.text(part, span)?;
                        }
                        let Some(Item {
                            kind: ItemKind::Aggregate(aggregate),
                            ..
                        }) = self.nominals.get(&path.join("."))
                        else {
                            return Ok(None);
                        };
                        for field in &aggregate.fields {
                            self.budget.text(&field.name, span)?;
                        }
                        let Some(field) = aggregate.fields.iter().find(|f| &f.name == member)
                        else {
                            return Ok(None);
                        };
                        if aggregate.generics.len() != args.len() {
                            return Ok(None);
                        }
                        let mut substitutions = BTreeMap::new();
                        for (parameter, argument) in aggregate.generics.iter().zip(args) {
                            let GenericArg::Type(ty) = argument else {
                                return Ok(None);
                            };
                            self.budget.text(&parameter.name, span)?;
                            self.budget.ty(ty)?;
                            substitutions.insert(parameter.name.clone(), ty.clone());
                        }
                        return substituted_type(&field.ty, &substitutions, self.budget).map(Some);
                    }
                    _ => None,
                };
                if let Some(field) = field {
                    self.budget.ty(field)?;
                }
                Ok(field.cloned())
            }
            ExprKind::Call { callee, arguments } => {
                if let Some(name) = super::super::integer_conversion_name(callee) {
                    return Ok(Some(named(name, span)));
                }
                if let ExprKind::Member {
                    object,
                    member: MemberName::Name(name),
                } = &callee.kind
                {
                    if name == "len"
                        && self.range_hint(object, state, depth + 1)?.is_some_and(|ty|
                            matches!(&group(&ty).kind, TypeKind::Reference { inner, .. }
                                if matches!(group(inner).kind, TypeKind::Array { length: None, .. })))
                    {
                        return Ok(Some(named("i64", span)));
                    }
                    return Ok(None);
                }
                let (target, explicit) = match &callee.kind {
                    ExprKind::Name(name) => (name, None),
                    ExprKind::TypeApply { callee, arguments } => match &callee.kind {
                        ExprKind::Name(name) => (name, Some(arguments)),
                        _ => return Ok(None),
                    },
                    _ => return Ok(None),
                };
                if matches!(
                    target.as_str(),
                    "bytes_len" | "bytes_capacity" | "store_len" | "store_capacity"
                ) {
                    return Ok(Some(named("i64", span)));
                }
                if let Some(local) = state.locals.get(target) {
                    if local.description_reference_depth.is_some() {
                        return Ok(None);
                    }
                    self.budget.ty(&local.ty)?;
                    let result = if let Some(bound) = self.callable.as_ref().and_then(|bounds| {
                        bounds.bindings().find(|bound| {
                            callable::parameter_mode(&local.ty, &bound.generic_name).is_some()
                        })
                    }) {
                        Some(&bound.result)
                    } else {
                        callable::opaque_signature(&local.ty).map(|(_, _, result)| result)
                    };
                    if let Some(result) = result {
                        self.budget.ty(result)?;
                    }
                    return Ok(result.cloned());
                }
                let signatures = self.signatures;
                let Some(function) = signatures.get(target) else {
                    return Ok(None);
                };
                let Some(result) = &function.return_type else {
                    return Ok(Some(unit(span)));
                };
                let mut variables = BTreeSet::new();
                for parameter in &function.generics {
                    self.budget.text(&parameter.name, span)?;
                    variables.insert(parameter.name.clone());
                }
                self.budget.ty(result)?;
                if !dependent(result, &variables) {
                    self.budget.ty(result)?;
                    return Ok(Some(result.clone()));
                }
                let mut found = BTreeMap::new();
                if let Some(explicit) = explicit {
                    if explicit.len() != function.generics.len() {
                        return Ok(None);
                    }
                    for (parameter, argument) in function.generics.iter().zip(explicit) {
                        let GenericArg::Type(ty) = argument else {
                            return Ok(None);
                        };
                        self.budget.text(&parameter.name, span)?;
                        self.budget.ty(ty)?;
                        found.insert(parameter.name.clone(), ty.clone());
                    }
                }
                for (parameter, argument) in function.params.iter().zip(arguments) {
                    if let Some(actual) = self.range_hint(&argument.value, state, depth + 1)? {
                        unify(
                            &parameter.ty,
                            &actual,
                            &variables,
                            &mut found,
                            span,
                            self.budget,
                        )?;
                    }
                }
                let result = substituted_type(result, &found, self.budget)?;
                Ok((!dependent(&result, &variables)).then_some(result))
            }
            _ => Ok(None),
        }
    }

    fn for_range(
        &mut self,
        pattern: &Pattern,
        iterable: &Expr,
        body: &Block,
        state: &mut State,
        span: Span,
    ) -> R<BlockFlow> {
        let PatternKind::Name(name) = &pattern.kind else {
            return Err(unsupported(
                pattern.span,
                "generic for requires a named integer index",
            ));
        };
        let mut range = iterable;
        while let ExprKind::Group(inner) = &range.kind {
            self.tick(range.span)?;
            range = inner;
        }
        let ExprKind::Binary {
            op: BinaryOp::Range,
            left,
            right,
        } = &range.kind
        else {
            return Err(unsupported(
                iterable.span,
                "generic for requires an integer range",
            ));
        };
        let hint = if let Some(ty) = self.range_hint(left, state, 0)? {
            Some(ty)
        } else {
            self.range_hint(right, state, 0)?
        };
        // Endpoints belong to the outer scope, execute once in source order,
        // and may consume an outer value once before the first loop entry.
        let ty = self.expr(left, state, hint.as_ref(), false)?;
        if dependent(&ty, &self.variables) {
            return Err(unsupported(
                left.span,
                "generic range endpoint requires a known integer type",
            ));
        }
        if !integer(&ty) {
            return Err(error(
                left.span,
                "integer range endpoints require u8, u16, i32 or i64",
            ));
        }
        let end = self.expr(right, state, Some(&ty), false)?;
        same(&end, &ty, right.span, self.budget)?;

        let mut iteration = self.clone_loop_state(state, span)?;
        self.budget.text(name, pattern.span)?;
        self.bind_local(&mut iteration, name.clone(), ty)?;
        // The concrete For continue edge goes through its increment block.
        // Here each such edge must retain the same outer symbolic obligations.
        let aliases = self.alias_mark_loop();
        self.loop_body(body, state, iteration, span, aliases)
    }
    // The concrete argument checker reborrows address parameters. Mirror only
    // that written reference shape here; an unconstrained T still consumes its
    // argument and gains no Copy or address capability from an instance.
    fn argument(
        &mut self,
        expression: &Expr,
        state: &mut State,
        parameter: &TypeRef,
        expected: Option<&TypeRef>,
    ) -> R<TypeRef> {
        let TypeKind::Reference { mutable, .. } = &group(parameter).kind else {
            return self.expr(expression, state, expected, false);
        };
        let mut actual = self.expr(expression, state, expected, true)?;
        let TypeKind::Reference {
            mutable: available, ..
        } = &group(&actual).kind
        else {
            return Err(error(
                expression.span,
                "reference parameter requires a reference",
            ));
        };
        if *mutable && !available {
            return Err(error(
                expression.span,
                "shared reference cannot grant exclusive access",
            ));
        }
        // Coercion changes only permission; later unification still compares
        // the actual referent with the complete symbolic parameter shape.
        self.budget.ty(&actual)?;
        actual = group(&actual).clone();
        let TypeKind::Reference { mutable: mode, .. } = &mut actual.kind else {
            unreachable!()
        };
        *mode = *mutable;
        self.alias_hold_result(state, *mutable, expression.span)?;
        Ok(actual)
    }

    fn bound_method(
        &mut self,
        object: &Expr,
        actual: &TypeRef,
        name: &str,
        arguments: &[Argument],
        state: &mut State,
    ) -> R<TypeRef> {
        let host = match &group(actual).kind {
            TypeKind::Reference { inner, .. } => inner.as_ref(),
            _ => actual,
        };
        let variable = trait_bounds::plain(host)
            .filter(|name| self.variables.contains(*name))
            .ok_or_else(|| {
                unsupported(
                    object.span,
                    "parametric method receiver requires a declared type-parameter bound",
                )
            })?;
        let method = self
            .traits
            .method(&self.bounds, variable, name, self.budget, object.span)?;
        let Some(receiver) = method.params.first().filter(|p| p.name == "self") else {
            return Err(error(
                object.span,
                "static trait method requires a type-qualified call",
            ));
        };
        if receiver.give || method.params.iter().any(|p| p.give) {
            return Err(unsupported(
                object.span,
                "trait-bound resource passing modes require their category solver",
            ));
        }
        if arguments.len() + 1 != method.params.len() {
            return Err(error(
                object.span,
                "trait-bound method argument count differs",
            ));
        }
        let substitutions = BTreeMap::from([("Self".to_owned(), host.clone())]);
        let expected = substituted_type(&receiver.ty, &substitutions, self.budget)?;
        match &group(&expected).kind {
            TypeKind::Reference { mutable, .. } => {
                if matches!(group(actual).kind, TypeKind::Reference { .. }) {
                    let actual = self.argument(object, state, &expected, Some(&expected))?;
                    same(&actual, &expected, object.span, self.budget)?;
                } else {
                    let (place, writable) = self.place(object, state)?;
                    if *mutable && !writable {
                        return Err(error(
                            object.span,
                            "mutable trait receiver requires a writable place",
                        ));
                    }
                    same(&place, host, object.span, self.budget)?;
                }
            }
            _ => {
                if matches!(group(actual).kind, TypeKind::Reference { .. }) {
                    return Err(error(
                        object.span,
                        "cannot move an unknown non-Copy trait receiver through a reference",
                    ));
                }
                let consumed = self.expr(object, state, Some(&expected), false)?;
                same(&consumed, &expected, object.span, self.budget)?;
            }
        }
        for (argument, parameter) in arguments.iter().zip(&method.params[1..]) {
            if argument.spread {
                return Err(unsupported(
                    argument.span,
                    "trait-bound method tuple spread",
                ));
            }
            let expected = substituted_type(&parameter.ty, &substitutions, self.budget)?;
            let actual = self.argument(&argument.value, state, &expected, Some(&expected))?;
            same(&actual, &expected, argument.span, self.budget)?;
        }
        match &method.return_type {
            Some(result) => substituted_type(result, &substitutions, self.budget),
            None => Ok(unit(object.span)),
        }
    }

    fn expr(
        &mut self,
        e: &Expr,
        state: &mut State,
        expected: Option<&TypeRef>,
        borrowing: bool,
    ) -> R<TypeRef> {
        self.aliases.result.clear();
        let base = self.alias_call_start();
        let ty = self.expr_inner(e, state, expected, borrowing)?;
        match &e.kind {
            ExprKind::Call { .. } => self.alias_call_end(state, base, &ty, e.span)?,
            ExprKind::Name(_)
            | ExprKind::Group(_)
            | ExprKind::MetaEval(_)
            | ExprKind::If { .. }
            | ExprKind::Unary {
                op: UnaryOp::Borrow | UnaryOp::BorrowMut,
                ..
            } => {}
            _ => self.aliases.result.clear(),
        }
        Ok(ty)
    }

    fn expr_inner(
        &mut self,
        e: &Expr,
        state: &mut State,
        expected: Option<&TypeRef>,
        borrowing: bool,
    ) -> R<TypeRef> {
        self.tick(e.span)?;
        let enum_context = expected.map(|ty| stores::success(ty).map_or(ty, |(error, _)| error));
        if let Some(ty) = self.enum_expression(e, state, enum_context)? {
            return Ok(ty);
        }
        let needs_bound = || {
            unsupported(
                e.span,
                "operation on an unconstrained type parameter requires a declared bound",
            )
        };
        match &e.kind {
            ExprKind::Name(name) => {
                let ty = state.locals.get(name).ok_or_else(|| {
                    error(e.span, format!("unknown value {name} in generic body"))
                })?;
                self.resource_description_use(ty.description_roots.as_deref(), state, e.span)?;
                let id = ty.id;
                let declaration_resource = ty.declaration_resource;
                self.budget.ty(&ty.ty)?;
                let ty = ty.ty.clone();
                let defer_consumes = if self.defer_captures.is_some() {
                    !borrowing && !declaration_resource && !self.known_copy(&ty)?
                } else {
                    false
                };
                self.defer_use(id, defer_consumes, e.span)?;
                if !self.factory_scopes.is_empty() {
                    let consumes = !borrowing && !declaration_resource && !self.known_copy(&ty)?;
                    self.factory_use(id, false, consumes, e.span)?;
                }
                if state.moved.contains(&id) {
                    return Err(error(
                        e.span,
                        "generic value used after move; Copy cannot be inferred from a concrete instance",
                    ));
                }
                if stores::element(&ty).is_some()
                    && state.store_origins.get(&id).is_some_and(|origins| {
                        origins
                            .iter()
                            .any(|root| state.invalidated_stores.contains(root))
                    })
                {
                    return Err(error(
                        e.span,
                        "Store descriptor depends on a carrier or field that was moved",
                    ));
                }
                self.alias_use_local(state, id, e.span)?;
                let owning_package = self
                    .callable
                    .as_ref()
                    .is_some_and(|bound| bound.owned_state)
                    && self.package_local(e, state).is_some();
                if !borrowing
                    && ((!declaration_resource && !self.known_copy(&ty)?) || owning_package)
                {
                    self.defer_held(state, id, true, e.span)?;
                    self.alias_invalidate(state, id, None, e.span)?;
                    for ((owner, _), field) in &self.package_slots {
                        self.budget.node(e.span, 0)?;
                        if *owner == id && state.moved.contains(field) {
                            return Err(error(
                                e.span,
                                "a partial callable State must be refilled before whole transfer",
                            ));
                        }
                        if *owner == id {
                            state.invalidated_stores.extend(
                                state
                                    .store_origins
                                    .get(field)
                                    .cloned()
                                    .unwrap_or_else(|| BTreeSet::from([*field])),
                            );
                        }
                    }
                    state.moved.insert(id);
                }
                Ok(ty)
            }
            ExprKind::Literal(Literal::Integer(text)) => {
                let expected = self.literal_context(expected);
                if expected.is_some_and(|t| dependent(t, &self.variables)) {
                    return Err(needs_bound());
                }
                Ok(checked_integer(text, expected, false, e.span)?.0)
            }
            ExprKind::Literal(Literal::Bool(_)) => Ok(named("bool", e.span)),
            ExprKind::Group(inner) | ExprKind::MetaEval(inner) => {
                self.expr(inner, state, expected, borrowing)
            }
            ExprKind::Tuple(values) => {
                let hints = expected.and_then(|t| match &group(t).kind {
                    TypeKind::Tuple(ts) => Some(ts),
                    _ => None,
                });
                let mut fields = vec![];
                for (i, v) in values.iter().enumerate() {
                    fields.push(self.expr(v, state, hints.and_then(|ts| ts.get(i)), borrowing)?);
                }
                Ok(TypeRef {
                    span: e.span,
                    kind: TypeKind::Tuple(fields),
                })
            }
            ExprKind::Record {
                ty: Some(constructor),
                base: None,
                fields,
            } => {
                let (name, args) = match &constructor.kind {
                    ExprKind::Name(name) => (name.clone(), vec![]),
                    ExprKind::TypeApply { callee, arguments } => {
                        let ExprKind::Name(name) = &callee.kind else {
                            return Err(unsupported(
                                e.span,
                                "generic record constructor needs a nominal type",
                            ));
                        };
                        (name.clone(), arguments.clone())
                    }
                    _ => {
                        return Err(unsupported(
                            e.span,
                            "generic record constructor needs a nominal type",
                        ));
                    }
                };
                let Some(Item {
                    kind: ItemKind::Aggregate(declaration),
                    ..
                }) = self.nominals.get(&name)
                else {
                    return Err(error(e.span, "unknown nominal record constructor"));
                };
                if declaration.generics.len() != args.len() {
                    return Err(error(e.span, "generic record constructor arity mismatch"));
                }
                let mut substitutions = BTreeMap::new();
                for (parameter, argument) in declaration.generics.iter().zip(&args) {
                    let GenericArg::Type(ty) = argument else {
                        return Err(unsupported(e.span, "constant generic record constructor"));
                    };
                    self.budget.ty(ty)?;
                    substitutions.insert(parameter.name.clone(), ty.clone());
                }
                let mut seen = BTreeSet::new();
                for field in fields {
                    if !seen.insert(&field.name) {
                        return Err(error(field.span, "duplicate generic record field"));
                    }
                    let declared = declaration
                        .fields
                        .iter()
                        .find(|f| f.name == field.name)
                        .ok_or_else(|| error(field.span, "unknown generic record field"))?;
                    if !declared.public && declared.span.file != field.span.file {
                        return Err(Diagnostic::error(
                            Phase::Expand,
                            "E-FIELD-PRIVATE",
                            Some(field.span),
                            "record field is private to its declaring module",
                        ));
                    }
                    let want = substituted_type(&declared.ty, &substitutions, self.budget)?;
                    let actual = if let Some(value) = &field.value {
                        if declared.manages
                            && stores::element(&want).is_some()
                            && !self.owning_store_expression(value, state)
                        {
                            return Err(error(
                                value.span,
                                "managed Store field requires an actual create/give value",
                            ));
                        }
                        self.expr(value, state, Some(&want), false)?
                    } else {
                        if declared.manages && stores::element(&want).is_some() {
                            return Err(error(
                                field.span,
                                "managed Store field shorthand cannot transfer a descriptor; write give",
                            ));
                        }
                        let local = Expr {
                            span: field.span,
                            kind: ExprKind::Name(field.name.clone()),
                        };
                        self.expr(&local, state, Some(&want), false)?
                    };
                    same(&actual, &want, field.span, self.budget)?;
                }
                if seen.len() != declaration.fields.len() {
                    return Err(error(e.span, "missing generic record field"));
                }
                let result = TypeRef {
                    span: e.span,
                    kind: TypeKind::Named {
                        path: vec![name],
                        args,
                    },
                };
                self.traits.store_types(
                    &result,
                    &self.bounds,
                    &self.variables,
                    self.budget,
                    self.callable.as_ref().map_or(
                        callable::StoreCallbacks::None,
                        callable::CallableTemplates::store_callbacks,
                    ),
                )?;
                Ok(result)
            }
            ExprKind::Array(values) => {
                let mut element = expected
                    .and_then(|ty| match &group(ty).kind {
                        TypeKind::Array { element, .. } => Some(element.as_ref()),
                        _ => None,
                    })
                    .cloned();
                for value in values {
                    let actual = self.expr(value, state, element.as_ref(), borrowing)?;
                    if let Some(element) = &element {
                        same(&actual, element, value.span, self.budget)?;
                    } else {
                        element = Some(actual);
                    }
                }
                array_type(
                    element.ok_or_else(|| {
                        error(e.span, "empty generic array requires element type context")
                    })?,
                    values.len(),
                    e.span,
                )
            }
            ExprKind::Index { object, .. } => {
                let (ty, _) = self.place(e, state)?;
                let actual_description = self.resource_desc_roots
                    && self.transferred_resource_expression(e, state)?.declaration;
                if !borrowing && !actual_description && !self.known_copy(&ty)? {
                    // Existing concrete owning arrays transfer complete Package
                    // elements, and Copy-member reads project without a move.
                    // A reference to such an array grants neither operation by
                    // copying its Package value into a local carrier.
                    let (base, _) = self.place(object, state)?;
                    let concrete_package = !dependent(&ty, &self.variables)
                        && matches!(&group(&ty).kind, TypeKind::Named { path, .. }
                            if matches!(self.nominals.get(&path.join(".")),
                                Some(Item { kind: ItemKind::Aggregate(a), .. })
                                if a.kind == AggregateKind::Package));
                    if self
                        .callable
                        .as_ref()
                        .is_some_and(|bound| bound.owned_state)
                        || !concrete_package
                        || !matches!(
                            &group(&base).kind,
                            TypeKind::Array {
                                length: Some(_),
                                ..
                            }
                        )
                    {
                        return Err(needs_bound());
                    }
                }
                Ok(ty)
            }
            ExprKind::Slice { .. } => {
                if !borrowing {
                    return Err(unsupported(
                        e.span,
                        "slice ranges require an explicit borrow",
                    ));
                }
                Ok(self.place(e, state)?.0)
            }
            ExprKind::Unary {
                op: UnaryOp::Borrow | UnaryOp::BorrowMut,
                operand,
            } => {
                let mutable = matches!(
                    e.kind,
                    ExprKind::Unary {
                        op: UnaryOp::BorrowMut,
                        ..
                    }
                );
                self.factory_place(operand, state, mutable)?;
                let (inner, writable) = self.place(operand, state)?;
                if let Some(slot) = self.existing_field_slot(operand, state)?
                    && state.moved.contains(&slot)
                {
                    return Err(error(e.span, "cannot borrow a moved callback field"));
                }
                if mutable && !writable {
                    return Err(error(e.span, "exclusive borrow requires a writable place"));
                }
                self.alias_borrow(operand, state, mutable)?;
                Ok(TypeRef {
                    span: e.span,
                    kind: TypeKind::Reference {
                        mutable,
                        lifetime: None,
                        inner: Box::new(inner),
                    },
                })
            }
            ExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => {
                let ty = self.expr(operand, state, None, true)?;
                let TypeKind::Reference { inner, .. } = &group(&ty).kind else {
                    return Err(error(e.span, "dereference requires an address"));
                };
                let actual_description = self.resource_desc_roots
                    && self.transferred_resource_expression(e, state)?.declaration;
                if !borrowing && !actual_description && !self.known_copy(inner)? {
                    return Err(needs_bound());
                }
                self.alias_read_place(e, state)?;
                Ok(*inner.clone())
            }
            ExprKind::Unary {
                op: UnaryOp::Negate,
                operand,
            } if matches!(operand.kind, ExprKind::Literal(Literal::Integer(_))) => {
                let expected = self.literal_context(expected);
                if expected.is_some_and(|t| dependent(t, &self.variables)) {
                    return Err(needs_bound());
                }
                let ExprKind::Literal(Literal::Integer(text)) = &operand.kind else {
                    unreachable!()
                };
                Ok(checked_integer(text, expected, true, e.span)?.0)
            }
            ExprKind::Unary {
                op: UnaryOp::Give,
                operand,
            } => self.managed_field_transfer(operand, state),
            ExprKind::Unary { op, operand } => {
                let ty = self.expr(operand, state, expected, false)?;
                if dependent(&ty, &self.variables) {
                    return Err(needs_bound());
                }
                let valid = match op {
                    UnaryOp::Negate | UnaryOp::BitNot => integer(&ty),
                    UnaryOp::Not => boolean(&ty),
                    UnaryOp::BorrowMut | UnaryOp::Give => {
                        return Err(unsupported(
                            e.span,
                            "generic mutable borrow or managed give requires capability checking",
                        ));
                    }
                    _ => unreachable!(),
                };
                if !valid {
                    return Err(error(
                        e.span,
                        "invalid concrete unary operand type in generic body",
                    ));
                }
                Ok(ty)
            }
            ExprKind::Binary { op, left, right } => {
                // A comparison produces bool; that result hint is not the
                // operand type of an ordinary helper call on its left.
                let operand_hint = if matches!(
                    op,
                    BinaryOp::Equal
                        | BinaryOp::NotEqual
                        | BinaryOp::Less
                        | BinaryOp::LessEqual
                        | BinaryOp::Greater
                        | BinaryOp::GreaterEqual
                ) {
                    None
                } else {
                    expected
                };
                let lhs = self.expr(left, state, operand_hint, false)?;
                let rhs = self.expr(right, state, Some(&lhs), false)?;
                if dependent(&lhs, &self.variables) || dependent(&rhs, &self.variables) {
                    return Err(needs_bound());
                }
                same(&rhs, &lhs, e.span, self.budget)?;
                let valid = match op {
                    BinaryOp::And | BinaryOp::Or => boolean(&lhs),
                    BinaryOp::Equal | BinaryOp::NotEqual => {
                        integer(&lhs)
                            || boolean(&lhs)
                            || matches!(&group(&lhs).kind, TypeKind::Tuple(ts) if ts.is_empty())
                    }
                    BinaryOp::Range => return Err(unsupported(e.span, "generic range value")),
                    _ => integer(&lhs),
                };
                if !valid {
                    return Err(error(
                        e.span,
                        "invalid concrete binary operand types in generic body",
                    ));
                }
                if matches!(
                    op,
                    BinaryOp::Equal
                        | BinaryOp::NotEqual
                        | BinaryOp::Less
                        | BinaryOp::LessEqual
                        | BinaryOp::Greater
                        | BinaryOp::GreaterEqual
                        | BinaryOp::And
                        | BinaryOp::Or
                ) {
                    Ok(named("bool", e.span))
                } else {
                    Ok(lhs)
                }
            }
            ExprKind::Call { callee, arguments } => {
                if let Some(name) =
                    super::super::foreign::bare_name(callee).filter(|name| stores::primitive(name))
                {
                    if arguments.len() != stores::arity(name)
                        || arguments.iter().any(|arg| arg.spread)
                    {
                        return Err(error(
                            e.span,
                            "Store primitive argument count or shape differs",
                        ));
                    }
                    let element = if name == "store_new" {
                        expected
                            .and_then(stores::carrier)
                            .and_then(stores::element)
                            .cloned()
                            .ok_or_else(|| {
                                error(
                                    e.span,
                                    "store_new requires an explicit Store element context",
                                )
                            })?
                    } else {
                        let store = self.expr(&arguments[0].value, state, None, true)?;
                        stores::element(&store).cloned().ok_or_else(|| {
                            error(e.span, "Store primitive requires a Store description")
                        })?
                    };
                    self.traits.store_element(
                        &element,
                        &self.bounds,
                        &self.variables,
                        self.budget,
                        self.callable.as_ref().map_or(
                            callable::StoreCallbacks::None,
                            callable::CallableTemplates::store_callbacks,
                        ),
                    )?;
                    if arguments.len() == 2 {
                        let wanted = if name == "store_push" {
                            element.clone()
                        } else {
                            named("i64", e.span)
                        };
                        let actual = self.expr(&arguments[1].value, state, Some(&wanted), false)?;
                        same(&actual, &wanted, arguments[1].span, self.budget)?;
                    }
                    return Ok(stores::result(name, &element, e.span));
                }
                if let Some((parameters, result)) = super::super::foreign::bare_name(callee)
                    .and_then(|name| bytes::signature(name, e.span))
                {
                    if arguments.len() != parameters.len() || arguments.iter().any(|arg| arg.spread)
                    {
                        return Err(error(
                            e.span,
                            "byte primitive argument count or shape differs",
                        ));
                    }
                    for (index, (argument, parameter)) in
                        arguments.iter().zip(&parameters).enumerate()
                    {
                        // Ordinary ByteBuffer observation takes a descriptor;
                        // it does not consume the carrier in this expression.
                        let actual =
                            self.expr(&argument.value, state, Some(parameter), index == 0)?;
                        same(&actual, parameter, argument.span, self.budget)?;
                    }
                    self.budget.ty(&result)?;
                    return Ok(result);
                }
                if let ExprKind::Member {
                    object,
                    member: MemberName::Name(name),
                } = &callee.kind
                {
                    if let Some(result) = self.state_field_call(callee, arguments, state)? {
                        return Ok(result);
                    }
                    let ty = self.expr(object, state, None, true)?;
                    if name == "len"
                        && matches!(&group(&ty).kind, TypeKind::Reference { inner, .. } if matches!(group(inner).kind, TypeKind::Array { length: None, .. }))
                    {
                        if !arguments.is_empty() {
                            return Err(error(e.span, "slice len requires no arguments"));
                        }
                        return Ok(named("i64", e.span));
                    }
                    return self.bound_method(object, &ty, name, arguments, state);
                }

                if let Some(value) =
                    super::super::foreign::literal_constructor(callee, arguments, e.span)
                {
                    let value = value?;
                    if let zil_ir::Constant::WideString(units) = &value {
                        self.budget.wide_units(units, e.span)?;
                    }
                    return Ok(named(&value.ty().name(), e.span));
                }
                if super::super::foreign::bare_name(callee) == Some("c_callback") {
                    let [argument] = arguments.as_slice() else {
                        return Err(super::super::foreign::callback_error(
                            e.span,
                            "c_callback requires one static symbol",
                        ));
                    };
                    if argument.spread {
                        return Err(super::super::foreign::callback_error(
                            e.span,
                            "C callback constructor does not accept spread arguments",
                        ));
                    }
                    let Some(target) = super::super::foreign::bare_name(&argument.value)
                        .filter(|name| !state.locals.contains_key(*name))
                    else {
                        return Err(super::super::foreign::callback_error(
                            e.span,
                            "c_callback requires one static symbol",
                        ));
                    };
                    let Some(function) = self.signatures.get(target) else {
                        return Err(super::super::foreign::callback_error(
                            e.span,
                            "unknown callback symbol",
                        ));
                    };
                    for parameter in &function.params {
                        self.budget.ty(&parameter.ty)?;
                    }
                    if let Some(result) = &function.return_type {
                        self.budget.ty(result)?;
                    }
                    return super::super::foreign::callback_syntax_type(function, e.span);
                }
                if let Some(name) = super::super::integer_conversion_name(callee) {
                    if arguments.len() != 1 || arguments[0].spread {
                        return Err(super::super::conversion_error(
                            e.span,
                            "integer conversion requires exactly one non-spread argument",
                        ));
                    }
                    let actual = self.expr(&arguments[0].value, state, None, false)?;
                    if !integer(&actual) {
                        return Err(super::super::conversion_error(
                            e.span,
                            "integer conversion requires a known integer operand",
                        ));
                    }
                    return Ok(named(name, e.span));
                }
                let (target, explicit) = match &callee.kind {
                    ExprKind::Name(n) => (n, None),
                    ExprKind::TypeApply { callee, arguments } => match &callee.kind {
                        ExprKind::Name(n) => (n, Some(arguments)),
                        _ => {
                            return Err(unsupported(
                                e.span,
                                "parametric qualified callable resolution",
                            ));
                        }
                    },
                    _ => {
                        return Err(unsupported(
                            e.span,
                            "parametric indirect callable resolution",
                        ));
                    }
                };
                if let Some(local) = state.locals.get(target) {
                    if local.description_reference_depth.is_some() {
                        return Err(error(
                            e.span,
                            "a declared resource description is not a callable parameter",
                        ));
                    }
                    // A factory's visible signature may itself be large. Pay
                    // its full type before cloning the symbolic local.
                    self.budget.ty(&local.ty)?;
                    let local = local.clone();
                    let Some(bounds) = self.callable.as_ref() else {
                        return Err(unsupported(
                            e.span,
                            "generic callable parameter requires a Fn bound",
                        ));
                    };
                    let owned_state =
                        bounds.owned_state || bounds.owned_factory || bounds.forwarded;
                    let bound = bounds.bindings().find(|bound| {
                        callable::parameter_mode(&local.ty, &bound.generic_name).is_some()
                    });
                    let (kind, mode, parameters, result, factory_local) = if let Some(bound) = bound
                    {
                        for parameter in &bound.parameters {
                            self.budget.ty(parameter)?;
                        }
                        self.budget.ty(&bound.result)?;
                        (
                            bound.kind,
                            callable::parameter_mode(&local.ty, &bound.generic_name).unwrap(),
                            bound.parameters.clone(),
                            bound.result.clone(),
                            false,
                        )
                    } else if let Some((kind, parameters, result)) =
                        callable::opaque_signature(&local.ty)
                    {
                        for parameter in parameters {
                            self.budget.ty(parameter)?;
                        }
                        self.budget.ty(result)?;
                        (kind, None, parameters.to_vec(), result.clone(), true)
                    } else {
                        return Err(error(
                            e.span,
                            "called generic local is neither the callable parameter nor a direct opaque factory result",
                        ));
                    };
                    if explicit.is_some() || arguments.len() != parameters.len() {
                        return Err(error(e.span, "callable bound argument count differs"));
                    }
                    if state.moved.contains(&local.id) {
                        return Err(error(
                            e.span,
                            "generic callable used after its environment was consumed",
                        ));
                    }
                    self.defer_use(local.id, kind == zil_ir::CallableKind::FnOnce, callee.span)?;
                    self.defer_held(
                        state,
                        local.id,
                        kind != zil_ir::CallableKind::Fn,
                        callee.span,
                    )?;
                    self.factory_use(
                        local.id,
                        kind == zil_ir::CallableKind::FnMut,
                        kind == zil_ir::CallableKind::FnOnce,
                        callee.span,
                    )?;
                    self.alias_use_local(state, local.id, callee.span)?;
                    match kind {
                        zil_ir::CallableKind::Fn => {}
                        zil_ir::CallableKind::FnMut if mode == Some(true) => {}
                        zil_ir::CallableKind::FnMut
                            if local.mutable && (factory_local || owned_state) => {}
                        zil_ir::CallableKind::FnMut if factory_local => {
                            return Err(error(
                                e.span,
                                "local FnMut call requires a mutable binding",
                            ));
                        }
                        zil_ir::CallableKind::FnOnce if mode.is_none() => {
                            self.alias_invalidate(state, local.id, None, callee.span)?;
                            state.moved.insert(local.id);
                        }
                        _ => {
                            return Err(unsupported(
                                e.span,
                                "callable bound requires its matching environment passing mode",
                            ));
                        }
                    }
                    if kind != zil_ir::CallableKind::FnOnce {
                        if mode.is_none() {
                            self.alias_borrow(callee, state, kind == zil_ir::CallableKind::FnMut)?;
                        }
                        self.alias_hold_result(
                            state,
                            kind == zil_ir::CallableKind::FnMut,
                            callee.span,
                        )?;
                    }
                    for (argument, parameter) in arguments.iter().zip(&parameters) {
                        if argument.spread {
                            return Err(unsupported(argument.span, "callable helper tuple spread"));
                        }
                        let actual =
                            self.argument(&argument.value, state, parameter, Some(parameter))?;
                        same(&actual, parameter, argument.span, self.budget)?;
                    }
                    return Ok(result);
                }
                if let Some((abi, function)) = self.foreign_signatures.get(target.as_str()) {
                    if !self.defer_body
                        && !self
                            .callable
                            .as_ref()
                            .is_some_and(|b| b.dual_names().is_some())
                    {
                        return Err(unsupported(
                            e.span,
                            "foreign calls in this parametric profile are not implemented",
                        ));
                    }
                    if !self.unsafe_scope {
                        return Err(error(
                            e.span,
                            "foreign call requires an explicit unsafe block",
                        ));
                    }
                    let scalar = |ty: &TypeRef, result: bool| match &group(ty).kind {
                        TypeKind::Named { path, args } if args.is_empty() => {
                            matches!(path.as_slice(), [name] if matches!(name.as_str(), "i32" | "i64" | "c_handle")
                                || result && name == "void")
                        }
                        TypeKind::Tuple(fields) if result => fields.is_empty(),
                        _ => false,
                    };
                    self.budget.node(e.span, 0)?;
                    if *abi != "\"C\""
                        || explicit.is_some()
                        || !function.generics.is_empty()
                        || !function.contract.is_empty()
                        || !function.where_clause.is_empty()
                        || function
                            .params
                            .iter()
                            .any(|p| p.give || !scalar(&p.ty, false))
                        || function
                            .return_type
                            .as_ref()
                            .is_some_and(|ty| !scalar(ty, true))
                    {
                        return Err(unsupported(
                            e.span,
                            "dual State marker calls require a direct concrete C scalar signature",
                        ));
                    }
                    if arguments.len() != function.params.len()
                        || arguments.iter().any(|a| a.spread)
                    {
                        return Err(error(
                            e.span,
                            "foreign call argument count or shape differs",
                        ));
                    }
                    for (argument, parameter) in arguments.iter().zip(&function.params) {
                        self.budget.ty(&parameter.ty)?;
                        let actual = self.argument(
                            &argument.value,
                            state,
                            &parameter.ty,
                            Some(&parameter.ty),
                        )?;
                        same(&actual, &parameter.ty, argument.span, self.budget)?;
                    }
                    return if let Some(result) = &function.return_type {
                        self.budget.ty(result)?;
                        Ok(result.clone())
                    } else {
                        Ok(unit(e.span))
                    };
                }
                let function = self
                    .signatures
                    .get(target)
                    .ok_or_else(|| error(e.span, "unknown function in parametric body"))?;
                if self.callable.is_some() && callable::template(function)?.is_some() {
                    self.dependencies.edge(
                        self.definition,
                        &function.name,
                        function.span,
                        self.budget,
                    )?;
                }
                if arguments.len() != function.params.len() {
                    return Err(error(e.span, "parametric call arity mismatch"));
                }
                let variables: BTreeSet<_> =
                    function.generics.iter().map(|p| p.name.clone()).collect();
                let mut found = BTreeMap::new();
                if let Some(args) = explicit {
                    if args.len() != function.generics.len() {
                        return Err(error(e.span, "parametric type argument arity mismatch"));
                    }
                    for (param, arg) in function.generics.iter().zip(args) {
                        let GenericArg::Type(ty) = arg else {
                            return Err(unsupported(e.span, "const generic argument"));
                        };
                        found.insert(param.name.clone(), ty.clone());
                    }
                }
                if let (Some(result), Some(expected)) = (&function.return_type, expected)
                    && !self.callable.as_ref().is_some_and(|bounds| {
                        bounds.owned_factory && callable::opaque_signature(result).is_some()
                    })
                    && let Some(expected) = stores::call_context(result, expected)
                {
                    unify(
                        result,
                        expected,
                        &variables,
                        &mut found,
                        e.span,
                        self.budget,
                    )?;
                }
                for (parameter, argument) in function.params.iter().zip(arguments) {
                    if argument.spread {
                        return Err(unsupported(argument.span, "parametric tuple spread"));
                    }
                    let hint = substituted_type(&parameter.ty, &found, self.budget)?;
                    if parameter.give
                        && stores::carrier(&hint).is_some()
                        && !self.owning_store_expression(&argument.value, state)
                    {
                        return Err(error(
                            argument.span,
                            "generic give parameter requires an actual Store create/give value",
                        ));
                    }
                    let actual = self.argument(
                        &argument.value,
                        state,
                        &hint,
                        (!dependent(&hint, &variables)).then_some(&hint),
                    )?;
                    unify(
                        &parameter.ty,
                        &actual,
                        &variables,
                        &mut found,
                        e.span,
                        self.budget,
                    )?;
                }
                if variables.iter().any(|name| !found.contains_key(name)) {
                    return Err(error(e.span, "unconstrained nested generic argument"));
                }
                let callable = callable::template(function)?;
                if let Some(callees) = &callable {
                    for callee in callees.bindings() {
                        self.budget.node(e.span, 0)?;
                        let actual = &found[&callee.generic_name];
                        let caller = self.callable.as_ref().and_then(|bounds| {
                            bounds.bindings().find(|caller| {
                                callable::parameter_mode(actual, &caller.generic_name) == Some(None)
                            })
                        });
                        let (kind, parameters, result) = if let Some(caller) = caller {
                            if !callees.owned_state
                                && !callees.forwarded
                                && !self.callable.as_ref().unwrap().owned_state
                                && caller.kind != callee.kind
                            {
                                return Err(unsupported(
                                    e.span,
                                    "nested synchronous helpers require the same declared callable kind for each forwarded role",
                                ));
                            }
                            (caller.kind, caller.parameters.as_slice(), &caller.result)
                        } else if let Some(signature) = callable::opaque_signature(actual) {
                            signature
                        } else {
                            return Err(error(
                                e.span,
                                "nested callable State helper requires an established callable bound",
                            ));
                        };
                        if !matches!(
                            (kind, callee.kind),
                            (zil_ir::CallableKind::Fn, _)
                                | (
                                    zil_ir::CallableKind::FnMut,
                                    zil_ir::CallableKind::FnMut | zil_ir::CallableKind::FnOnce
                                )
                                | (zil_ir::CallableKind::FnOnce, zil_ir::CallableKind::FnOnce)
                        ) || parameters.len() != callee.parameters.len()
                        {
                            return Err(error(
                                e.span,
                                "nested callable State helper requires a compatible callable capability",
                            ));
                        }
                        for (actual, expected) in parameters.iter().zip(&callee.parameters) {
                            let expected = substituted_type(expected, &found, self.budget)?;
                            same(actual, &expected, e.span, self.budget)?;
                        }
                        let expected = substituted_type(&callee.result, &found, self.budget)?;
                        same(result, &expected, e.span, self.budget)?;
                    }
                }
                self.traits.check_arguments(
                    &function.generics,
                    &found,
                    (
                        &self.bounds,
                        &self.variables,
                        self.callable.as_ref().map_or(
                            callable::StoreCallbacks::None,
                            callable::CallableTemplates::store_callbacks,
                        ),
                    ),
                    self.budget,
                    e.span,
                    callable.as_ref(),
                )?;
                match &function.return_type {
                    Some(result) => substituted_type(result, &found, self.budget),
                    None => Ok(unit(e.span)),
                }
            }
            ExprKind::Try(operand) => {
                if self.defer_body {
                    return Err(Self::defer_control(
                        e.span,
                        "a defer body cannot propagate an error outside its cleanup boundary",
                    ));
                }
                let union = self.expr(operand, state, None, false)?;
                let TypeKind::Error {
                    error: actual_error,
                    result,
                } = &group(&union).kind
                else {
                    return Err(error(e.span, "propagation requires an error union"));
                };
                let TypeKind::Error {
                    error: expected_error,
                    ..
                } = &group(&self.result).kind
                else {
                    return Err(error(
                        e.span,
                        "propagation requires an error-returning helper",
                    ));
                };
                same(actual_error, expected_error, e.span, self.budget)?;
                Ok(result.as_ref().clone())
            }
            ExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                let condition_type =
                    self.expr(condition, state, Some(&named("bool", e.span)), false)?;
                same(&condition_type, &named("bool", e.span), e.span, self.budget)?;
                let mut a = self.clone_state(state)?;
                let mut b = self.clone_state(state)?;
                let lhs = self.expr(then_value, &mut a, expected, borrowing)?;
                let aliases_a = self.alias_result_copy(e.span)?;
                let rhs = self.expr(else_value, &mut b, Some(&lhs), borrowing)?;
                let aliases_b = self.alias_result_copy(e.span)?;
                if self
                    .callable
                    .as_ref()
                    .is_some_and(|bounds| bounds.owned_factory)
                    && callable::opaque_signature(&lhs).is_some()
                    && callable::opaque_signature(&rhs).is_some()
                {
                    self.factory_same_result(&lhs, &rhs, e.span)?;
                } else {
                    same(&lhs, &rhs, e.span, self.budget)?;
                }
                self.join_states(state, &[&a, &b], e.span)?;
                self.alias_result_join(aliases_a, aliases_b, e.span)?;
                Ok(lhs)
            }
            ExprKind::Member {
                object,
                member: MemberName::TupleIndex(i),
            } => {
                let value = self.expr(object, state, None, borrowing)?;
                let TypeKind::Tuple(fields) = &group(&value).kind else {
                    return Err(needs_bound());
                };
                fields
                    .get(*i as usize)
                    .cloned()
                    .ok_or_else(|| error(e.span, "generic tuple index outside its static shape"))
            }
            ExprKind::Member {
                object,
                member: MemberName::Name(member),
            } => {
                let package = self.package_local(object, state);
                // A known Address local is accessed, not consumed, to project a
                // Copy child. Ordinary symbolic Values retain their previous
                // consumption rule: Holder<T>.value cannot silently copy T.
                let mut base_expression = object.as_ref();
                while let ExprKind::Group(inner) = &base_expression.kind {
                    self.tick(base_expression.span)?;
                    base_expression = inner;
                }
                let address_local = matches!(&base_expression.kind, ExprKind::Name(name)
                    if state.locals.get(name).is_some_and(|local|
                        matches!(group(&local.ty).kind, TypeKind::Reference { .. })));
                let declaration_resource = if self.resource_desc_roots {
                    let flow = self.transferred_resource_expression(object, state)?;
                    flow.declaration || flow.reference_depth == Some(1)
                } else {
                    false
                };
                let value = self.expr(
                    object,
                    state,
                    None,
                    borrowing || package.is_some() || address_local || declaration_resource,
                )?;
                let (base, through_address) = match &group(&value).kind {
                    TypeKind::Reference { inner, .. } => (group(inner), true),
                    _ => (group(&value), false),
                };
                let TypeKind::Named { path, args } = &base.kind else {
                    return Err(needs_bound());
                };
                if !declaration_resource && path.len() == 1 && self.variables.contains(&path[0]) {
                    return Err(needs_bound());
                }
                let Some(Item {
                    kind: ItemKind::Aggregate(aggregate),
                    ..
                }) = self.nominals.get(&path.join("."))
                else {
                    return Err(needs_bound());
                };
                if aggregate.generics.len() != args.len() {
                    return Err(error(e.span, "generic record argument arity mismatch"));
                }
                let field = aggregate
                    .fields
                    .iter()
                    .find(|field| &field.name == member)
                    .ok_or_else(|| error(e.span, "unknown generic record field"))?;
                if !field.public && field.span.file != e.span.file {
                    return Err(Diagnostic::error(
                        Phase::Expand,
                        "E-FIELD-PRIVATE",
                        Some(e.span),
                        "record field is private to its declaring module",
                    ));
                }
                let mut substitutions = BTreeMap::new();
                for (p, arg) in aggregate.generics.iter().zip(args) {
                    let GenericArg::Type(ty) = arg else {
                        return Err(unsupported(e.span, "generic constant field shape"));
                    };
                    substitutions.insert(p.name.clone(), ty.clone());
                }
                let ty = substituted_type(&field.ty, &substitutions, self.budget)?;
                if through_address && (field.manages || !self.known_copy(&ty)?) {
                    return Err(unsupported(
                        e.span,
                        "generic address field reads require a proven Copy projection",
                    ));
                }
                if field.manages
                    && self
                        .existing_field_slot(e, state)?
                        .is_some_and(|slot| state.moved.contains(&slot))
                {
                    return Err(error(e.span, "managed Package field used after give"));
                }
                self.alias_read_place(e, state)?;
                if let Some(local) = package
                    && self.callable.as_ref().is_some_and(|bounds| {
                        bounds.bindings().any(|bound| {
                            callable::parameter_mode(&ty, &bound.generic_name) == Some(None)
                        })
                    })
                {
                    let slot = self.field_slot(local, member, e.span)?;
                    if state.moved.contains(&slot) {
                        return Err(error(
                            e.span,
                            "callback field used after its environment was moved",
                        ));
                    }
                    if !borrowing {
                        self.alias_invalidate(state, local, Some(slot), e.span)?;
                        state.moved.insert(slot);
                    }
                }
                Ok(ty)
            }
            ExprKind::Closure {
                move_capture,
                params,
                return_type,
                body,
            } if self
                .callable
                .as_ref()
                .is_some_and(|bounds| bounds.owned_factory) =>
            {
                let saved_cleanup = std::mem::replace(&mut self.defer_body, false);
                let checked = self.factory_closure(
                    *move_capture,
                    params,
                    return_type.as_ref(),
                    body,
                    expected,
                    state,
                    e.span,
                );
                self.defer_body = saved_cleanup;
                checked
            }
            _ => Err(unsupported(
                e.span,
                "this generic expression requires additional symbolic checking",
            )),
        }
    }
    /// A direct callable field is a value invocation, not a trait method.
    /// Resolve the actual Package place and its declared field before using
    /// the existing symbolic F bound; the concrete pipeline still proves its
    /// environment, alias windows and cleanup obligations independently.
    fn state_field_call(
        &mut self,
        callee: &Expr,
        arguments: &[Argument],
        state: &mut State,
    ) -> R<Option<TypeRef>> {
        let Some(bounds) = self.callable.as_ref().filter(|bounds| bounds.owned_state) else {
            return Ok(None);
        };
        let dual = bounds.dual_names().is_some();
        let ExprKind::Member {
            object,
            member: MemberName::Name(member),
        } = &callee.kind
        else {
            return Ok(None);
        };
        let owner = self.package_local(object, state);
        if owner.is_none() {
            let mut base = object.as_ref();
            while let ExprKind::Group(inner) = &base.kind {
                self.budget.node(base.span, 0)?;
                base = inner;
            }
            let window = match &base.kind {
                ExprKind::Name(name) => state.locals.get(name).is_some_and(|local| {
                    matches!(&group(&local.ty).kind, TypeKind::Reference { inner, .. }
                        if bounds.store_callbacks().active(inner))
                }),
                ExprKind::Call { callee, .. } => {
                    matches!(stores::call_name(callee), Some("store_at" | "store_at_mut"))
                }
                _ => false,
            };
            if !window {
                return Ok(None);
            }
        }
        // Only the new two-root path selects from the actual resolved field.
        // One representative ClosureId or first bound cannot supply its mode.
        let place = if dual {
            Some(self.place(callee, state)?)
        } else {
            None
        };
        let bounds = self.callable.as_ref().expect("owning callable scope");
        let bound = if let Some((ty, _)) = &place {
            self.budget.node(callee.span, 0)?;
            let Some(bound) = bounds
                .bindings()
                .find(|bound| callable::parameter_mode(ty, &bound.generic_name) == Some(None))
            else {
                return Ok(None);
            };
            self.budget.text(&bound.generic_name, callee.span)?;
            bound
        } else {
            &bounds.roles[0]
        };
        for parameter in &bound.parameters {
            self.budget.ty(parameter)?;
        }
        self.budget.ty(&bound.result)?;
        let bound = bound.clone();
        let (ty, mutable) = match place {
            Some(place) => place,
            None => self.place(callee, state)?,
        };
        if callable::parameter_mode(&ty, &bound.generic_name) != Some(None) {
            return Ok(None);
        }
        let slot = owner
            .map(|owner| self.field_slot(owner, member, callee.span))
            .transpose()?;
        if slot.is_some_and(|slot| state.moved.contains(&slot)) {
            return Err(error(
                callee.span,
                "callback field used after its environment was moved",
            ));
        }
        match bound.kind {
            zil_ir::CallableKind::Fn => {}
            zil_ir::CallableKind::FnMut if mutable => {}
            zil_ir::CallableKind::FnOnce if slot.is_some() => {
                self.alias_invalidate(state, owner.expect("owning State"), slot, callee.span)?;
                state.moved.insert(slot.expect("owning field"));
            }
            zil_ir::CallableKind::FnOnce => {
                return Err(error(
                    callee.span,
                    "FnOnce requires an owned Store element obtained by pop",
                ));
            }
            _ => {
                return Err(error(
                    callee.span,
                    "mutable callback field requires a mutable owning State",
                ));
            }
        }
        if arguments.len() != bound.parameters.len() {
            return Err(error(callee.span, "callable field argument count differs"));
        }
        if bound.kind != zil_ir::CallableKind::FnOnce {
            self.alias_borrow(callee, state, bound.kind == zil_ir::CallableKind::FnMut)?;
            self.alias_hold_result(
                state,
                bound.kind == zil_ir::CallableKind::FnMut,
                callee.span,
            )?;
        }
        for (argument, parameter) in arguments.iter().zip(&bound.parameters) {
            if argument.spread {
                return Err(unsupported(argument.span, "callable State tuple spread"));
            }
            let actual = self.argument(&argument.value, state, parameter, Some(parameter))?;
            same(&actual, parameter, argument.span, self.budget)?;
        }
        Ok(Some(bound.result))
    }
    // Return true when every path through this block has returned. Branches
    // keep separate move states, so a returning arm cannot consume the value
    // on a distinct fallthrough path.
    fn block(&mut self, block: &Block, state: &mut State) -> R<BlockFlow> {
        self.tick(block.span)?;
        for _ in &state.defer_holds {
            self.budget.node(block.span, 0)?;
        }
        let first_resource_local = self.next_local;
        let outer_holds = state.defer_holds.clone();
        let mut flow = BlockFlow::NEXT;
        for statement in &block.statements {
            self.tick(statement.span)?;
            if !flow.fallthrough {
                return Err(unsupported(
                    statement.span,
                    "unreachable-statement type checking",
                ));
            }
            match &statement.kind {
                StmtKind::Binding {
                    pattern:
                        Pattern {
                            kind: PatternKind::Name(n),
                            ..
                        },
                    ty,
                    value,
                    mutable,
                } => {
                    if let Some(ty) = ty {
                        if stores::success(ty).is_some() {
                            return Err(unsupported(
                                ty.span,
                                "generic owned Store union locals require a producer-inferred success mode",
                            ));
                        }
                        self.traits.store_types(
                            ty,
                            &self.bounds,
                            &self.variables,
                            self.budget,
                            self.callable.as_ref().map_or(
                                callable::StoreCallbacks::None,
                                callable::CallableTemplates::store_callbacks,
                            ),
                        )?;
                    }
                    let owns_store = self.owning_store_expression(value, state);
                    let fresh_store = owns_store && self.fresh_store_expression(value, state)?;
                    let mut resource_flow = if self.enum_resource_flow {
                        self.transferred_resource_expression(value, state)?
                    } else {
                        ResourceFlow::default()
                    };
                    if self.two_input_state_constructor()
                        || self
                            .callable
                            .as_ref()
                            .is_some_and(|bounds| bounds.owned_factory)
                    {
                        resource_flow.owns = self.owned_resource_expression(value, state)?;
                    }
                    if resource_flow.declaration
                        && ty.as_ref().is_some_and(|ty| dependent(ty, &self.variables))
                    {
                        return Err(error(
                            statement.span,
                            "a declaration resource cannot be annotated as a helper type parameter",
                        ));
                    }
                    let origins = self.store_origins(value, state)?;
                    let actual = self.expr(value, state, ty.as_ref(), false)?;
                    self.traits.store_types(
                        &actual,
                        &self.bounds,
                        &self.variables,
                        self.budget,
                        self.callable.as_ref().map_or(
                            callable::StoreCallbacks::None,
                            callable::CallableTemplates::store_callbacks,
                        ),
                    )?;
                    if let Some(ty) = ty {
                        same(&actual, ty, statement.span, self.budget)?;
                    }
                    self.bind_local(state, n.clone(), ty.clone().unwrap_or(actual))?;
                    self.factory_bind_value(state.locals[n].id, value, state)?;
                    self.alias_bind(state, state.locals[n].id, statement.span)?;
                    let owns_resource = resource_flow.owns;
                    let declaration_resource = resource_flow.declaration;
                    let roots = if declaration_resource && owns_resource {
                        Some(self.resource_description_root(state.locals[n].id, statement.span)?)
                    } else {
                        resource_flow.roots
                    };
                    let local = state.locals.get_mut(n).expect("just bound");
                    local.mutable = *mutable;
                    local.declaration_resource = declaration_resource;
                    local.description_roots = roots;
                    local.description_reference_depth = resource_flow.reference_depth;
                    local.owned_resource = owns_resource
                        && (declaration_resource || self.concrete_helper_resource(&local.ty));
                    if stores::element(&local.ty).is_some() {
                        if owns_store {
                            state.store_owners.insert(local.id);
                            if fresh_store {
                                state.fresh_stores.insert(local.id);
                            }
                            state
                                .store_origins
                                .insert(local.id, BTreeSet::from([local.id]));
                        } else {
                            state.store_origins.insert(local.id, origins);
                        }
                    } else if let Some((error_type, _)) = stores::success(&local.ty) {
                        stores::copy_error(error_type, self.nominals, self.budget)?;
                        if !owns_store {
                            return Err(error(
                                value.span,
                                "generic Store success local requires an actual owning union",
                            ));
                        }
                        state.store_results.insert(local.id);
                        if fresh_store {
                            state.fresh_stores.insert(local.id);
                        }
                    }
                }
                StmtKind::Assign { target, op, value } => {
                    self.factory_place(target, state, true)?;
                    let (ty, writable) = self.assignment_place(target, state)?;
                    if stores::success(&ty).is_some() {
                        return Err(unsupported(
                            target.span,
                            "generic owning Store union replacement is not implemented",
                        ));
                    }
                    if !writable {
                        return Err(error(target.span, "assignment requires a writable place"));
                    }
                    if *op != AssignOp::Set && (!integer(&ty) || dependent(&ty, &self.variables)) {
                        return Err(unsupported(
                            target.span,
                            "compound assignment requires a known integer place",
                        ));
                    }
                    let mut whole = target;
                    while let ExprKind::Group(inner) = &whole.kind {
                        whole = inner;
                    }
                    if let ExprKind::Name(name) = &whole.kind
                        && let Some(local) = state.locals.get(name)
                    {
                        self.defer_use(local.id, false, target.span)?;
                        self.defer_held(state, local.id, true, target.span)?;
                    }
                    let store_target = if stores::element(&ty).is_some() {
                        let slot = match &whole.kind {
                            ExprKind::Name(name) => Some((
                                state.locals[name].id,
                                state.store_owners.contains(&state.locals[name].id),
                            )),
                            ExprKind::Member {
                                object,
                                member: MemberName::Name(member),
                            } => self
                                .package_local(object, state)
                                .map(|owner| self.field_slot(owner, member, target.span))
                                .transpose()?
                                .map(|slot| (slot, true)),
                            _ => None,
                        };
                        let Some((slot, owns)) = slot else {
                            return Err(unsupported(
                                target.span,
                                "generic Store replacement requires a real local or Package field",
                            ));
                        };
                        let supplied = self.owning_store_expression(value, state);
                        if owns != supplied {
                            return Err(error(
                                value.span,
                                "Store replacement must preserve the declared carrier/description mode",
                            ));
                        }
                        let fresh = owns
                            && matches!(whole.kind, ExprKind::Name(_))
                            && self.fresh_store_expression(value, state)?;
                        Some((slot, owns, self.store_origins(value, state)?, fresh))
                    } else {
                        None
                    };
                    let resource_replacement =
                        self.resource_description_replacement(whole, value, state)?;
                    let actual = self.expr(value, state, Some(&ty), false)?;
                    same(&actual, &ty, value.span, self.budget)?;
                    if let Some((name, flow)) = resource_replacement {
                        self.resource_description_replace(&name, flow, state, target.span)?;
                    }
                    self.alias_replace_place(target, state)?;
                    let mut whole = target;
                    while let ExprKind::Group(inner) = &whole.kind {
                        self.budget.node(whole.span, 0)?;
                        whole = inner;
                    }
                    if let ExprKind::Name(name) = &whole.kind {
                        let local = state.locals[name].id;
                        self.alias_bind(state, local, target.span)?;
                        // Restore only after the complete RHS succeeded. A
                        // move from this very target during RHS evaluation is
                        // still checked, including repeated argument uses.
                        state.moved.remove(&local);
                        for ((owner, _), slot) in &self.package_slots {
                            self.budget.node(target.span, 0)?;
                            if *owner == local {
                                state.moved.remove(slot);
                            }
                        }
                    } else if let Some(slot) = self.existing_field_slot(target, state)? {
                        state.moved.remove(&slot);
                    }
                    if let Some((slot, owns, origins, fresh)) = store_target {
                        if owns {
                            self.replace_store_origin(state, slot, target.span)?;
                            if fresh {
                                state.fresh_stores.insert(slot);
                            } else {
                                state.fresh_stores.remove(&slot);
                            }
                        } else {
                            state.store_origins.insert(slot, origins);
                        }
                    } else if let ExprKind::Name(name) = &whole.kind {
                        let local = state.locals[name].id;
                        let fields: Vec<_> = self
                            .package_slots
                            .iter()
                            .filter_map(|((owner, _), slot)| (*owner == local).then_some(*slot))
                            .collect();
                        for field in fields {
                            self.replace_store_origin(state, field, target.span)?;
                        }
                    }
                }
                StmtKind::Defer(body) => self.defer_statement(body, state, statement.span)?,
                StmtKind::Return(value) => {
                    if self.defer_body {
                        return Err(Self::defer_control(
                            statement.span,
                            "return cannot leave a defer cleanup body",
                        ));
                    }
                    if !self.factory_scopes.is_empty() {
                        self.factory_body_return(value.as_ref(), state, statement.span)?;
                        flow.fallthrough = false;
                        flow.returns = true;
                        continue;
                    }
                    let expected = self.result.clone();
                    let actual = match value {
                        Some(e) => self.return_expression(e, state)?,
                        None => unit(statement.span),
                    };
                    if self
                        .callable
                        .as_ref()
                        .is_some_and(|bounds| bounds.owned_factory)
                    {
                        self.factory_same_result(&actual, &expected, statement.span)?;
                    } else if self.callable.is_some()
                        || matches!(group(&expected).kind, TypeKind::Error { .. })
                    {
                        same_return(&actual, &expected, statement.span, self.budget)?;
                    } else {
                        same(&actual, &expected, statement.span, self.budget)?;
                    }
                    flow.fallthrough = false;
                    flow.returns = true;
                }
                StmtKind::Expression(e) => {
                    self.expr(e, state, None, false)?;
                }
                StmtKind::If {
                    condition: Condition::Expression(condition),
                    then_body,
                    else_body,
                } => {
                    let actual = self.expr(
                        condition,
                        state,
                        Some(&named("bool", condition.span)),
                        false,
                    )?;
                    same(
                        &actual,
                        &named("bool", condition.span),
                        condition.span,
                        self.budget,
                    )?;
                    let mut a = self.clone_state(state)?;
                    let mut b = self.clone_state(state)?;
                    let flow_a = self.block(then_body, &mut a)?;
                    let flow_b = match else_body {
                        Some(ElseBranch::Block(block)) => self.block(block, &mut b)?,
                        None => BlockFlow::NEXT,
                        Some(ElseBranch::If(statement)) => self.block(
                            &Block {
                                span: statement.span,
                                statements: vec![*statement.clone()],
                            },
                            &mut b,
                        )?,
                    };
                    flow.returns |= flow_a.returns || flow_b.returns;
                    flow.fallthrough = flow_a.fallthrough || flow_b.fallthrough;
                    match (flow_a.fallthrough, flow_b.fallthrough) {
                        (false, false) => {}
                        (false, true) => self.join_states(state, &[&b], statement.span)?,
                        (true, false) => self.join_states(state, &[&a], statement.span)?,
                        (true, true) => {
                            self.join_states(state, &[&a, &b], statement.span)?;
                        }
                    }
                }
                StmtKind::Match { value, arms } => {
                    let matched = self.enum_match(value, arms, state)?;
                    flow.fallthrough = matched.fallthrough;
                    flow.returns |= matched.returns;
                }
                StmtKind::While {
                    condition: Condition::Expression(condition),
                    body,
                } => {
                    let aliases = self.alias_mark_loop();
                    let entry = self.clone_loop_state(state, condition.span)?;
                    let actual = self.expr(
                        condition,
                        state,
                        Some(&named("bool", condition.span)),
                        false,
                    )?;
                    same(
                        &actual,
                        &named("bool", condition.span),
                        condition.span,
                        self.budget,
                    )?;
                    self.loop_invariant(&entry, state, condition.span)?;
                    let iteration = self.clone_loop_state(state, body.span)?;
                    let iteration_flow =
                        self.loop_body(body, state, iteration, statement.span, aliases)?;
                    flow.returns |= iteration_flow.returns;
                }
                StmtKind::For {
                    pattern,
                    iterable,
                    body,
                } => {
                    let iteration_flow =
                        self.for_range(pattern, iterable, body, state, statement.span)?;
                    flow.returns |= iteration_flow.returns;
                }
                StmtKind::Unsafe(body)
                    if self
                        .callable
                        .as_ref()
                        .is_some_and(|b| b.dual_names().is_some())
                        || self.defer_body =>
                {
                    let mut inner = self.clone_state(state)?;
                    let saved = self.unsafe_scope;
                    self.unsafe_scope = true;
                    let inner_flow = self.block(body, &mut inner)?;
                    self.unsafe_scope = saved;
                    self.join_states(state, &[&inner], statement.span)?;
                    flow.fallthrough = inner_flow.fallthrough;
                    flow.returns |= inner_flow.returns;
                }
                StmtKind::Break | StmtKind::Continue => {
                    if self.defer_body && self.loops.is_empty() {
                        return Err(Self::defer_control(
                            statement.span,
                            "loop control cannot cross a defer cleanup boundary",
                        ));
                    }
                    self.loop_control(
                        state,
                        matches!(statement.kind, StmtKind::Continue),
                        statement.span,
                    )?;
                    flow.fallthrough = false;
                }
                _ => {
                    return Err(unsupported(
                        statement.span,
                        "this parametric statement requires additional symbolic checking",
                    ));
                }
            }
        }
        self.resource_description_scope_exit(state, first_resource_local, block.span)?;
        state.defer_holds = outer_holds;
        Ok(flow)
    }
}

#[path = "generic_check_enum.rs"]
mod enums;

#[path = "generic_loop_control.rs"]
mod loop_control;

#[path = "generic_places.rs"]
mod places;

#[path = "generic_aliases.rs"]
mod state_aliases;

#[path = "generic_resource_desc.rs"]
mod resource_desc;
use resource_desc::ResourceFlow;
