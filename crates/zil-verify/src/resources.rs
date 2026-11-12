//! Independent finite resource responsibility and description checking.
//!
//! Descriptions carry only a source position, never ownership or an address
//! loan. Every resource operation is reconstructed from the checked CFG; the
//! candidate's responsibility fields are not trusted or consulted.
use super::*;
pub(super) fn head_address(ty: &Type) -> bool {
    matches!(ty,Type::Address{inner,..} if matches!(inner.as_ref(),Type::Managed(payload) if matches!(payload.as_ref(),Type::ByteBuffer|Type::OwnedStore{..}|Type::AsyncRegistration{..}) || matches!(payload.as_ref(),Type::Record{..}) && payload.is_copy_value()))
}
use std::collections::VecDeque;
pub(crate) mod arrays;
mod desc_liveness;
mod field_fresh;
mod product;
pub(super) mod public_roles;
mod replacements;
pub(super) mod schedule;
mod slot_layout;
pub(crate) mod task_frames;

const EMPTY: u8 = 1;
const LIVE: u8 = 2;
const MOVED: u8 = 4;
const DONE: u8 = 8;
const FRESH: u8 = 1;
const INPUT: u8 = 2;

fn discharged(debt: u8) -> u8 {
    (debt & !LIVE) | if debt & LIVE != 0 { DONE } else { 0 }
}

fn bad(span: Span, message: impl Into<String>) -> Diagnostic {
    reject("E-VERIFY-RESOURCE", Some(span), message)
}

pub(super) fn is_teardown(
    module: &TypedModule,
    function: FunctionId,
    budget: &mut Budget,
) -> Check<bool> {
    budget.spend(module.resources.len())?;
    Ok(module
        .resources
        .iter()
        .any(|r| r.teardown == Some(function)))
}

// All raw type trees have already passed check_type_shape. Charge the repeated
// payload admission and equality work as well; the first shape pass cannot pay
// for every later use of a nominal resource type.
pub(super) fn charge_payload(ty: &Type, budget: &mut Budget) -> Check<()> {
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        budget.spend(2)?;
        tasks::charge_result_context(ty, budget)?;
        tasks::require_closed_context(ty, None)?;
        match ty {
            Type::BorrowedStorage { storage, life } => {
                budget.spend(
                    life.slots
                        .len()
                        .saturating_add(life.retained.len())
                        .saturating_add(life.nominal_arguments.len())
                        .saturating_add(1),
                )?;
                for r in &life.slots {
                    budget.spend(r.path.len())?;
                }
                for r in &life.retained {
                    budget.spend(r.path.len())?;
                }
                for r in &life.nominal_arguments {
                    budget.spend(r.path.len())?;
                }
                pending.push(storage);
            }
            Type::Task {
                result: element, ..
            }
            | Type::Mutex { payload: element }
            | Type::MutexGuard {
                payload: element, ..
            }
            | Type::Array { element, .. }
            | Type::Slice { element }
            | Type::Registration {
                environment: element,
            }
            | Type::AsyncRegistration {
                environment: element,
            }
            | Type::OwnedStore { element }
            | Type::Address { inner: element, .. }
            | Type::Managed(element)
            | Type::Desc(element) => pending.push(element),
            Type::ErrorUnion { error, result } => {
                budget.spend(2)?;
                pending.push(error);
                pending.push(result);
            }
            Type::Tuple(fields) => {
                budget.spend(fields.len().saturating_mul(2))?;
                pending.extend(fields);
            }
            Type::Record { name, fields } | Type::Package { name, fields } => {
                budget.spend(name.len().saturating_add(fields.len()).saturating_mul(2))?;
                for (name, ty) in fields {
                    budget.spend(name.len().saturating_mul(2))?;
                    pending.push(ty);
                }
            }
            Type::Enum { name, variants } => {
                budget.spend(name.len().saturating_add(variants.len()).saturating_mul(2))?;
                for (name, fields) in variants {
                    budget.spend(name.len().saturating_add(fields.len()).saturating_mul(2))?;
                    pending.extend(fields);
                }
            }
            Type::BorrowedFunctionPointer {
                parameters,
                result,
                life,
            } => {
                crate::signature_life::charge(life, budget)?;
                budget.spend(parameters.len().saturating_mul(2))?;
                pending.extend(parameters);
                pending.push(result);
            }
            Type::FunctionPointer { parameters, result }
            | Type::ForeignCallback { parameters, result } => {
                budget.spend(parameters.len().saturating_mul(2))?;
                pending.extend(parameters);
                pending.push(result);
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn check_type_for(
    module: &TypedModule,
    ty: &Type,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    let (Type::Managed(inner) | Type::Desc(inner)) = ty else {
        return Err(unsupported(
            "expected a finite resource carrier or description",
            span,
        ));
    };
    if matches!(inner.as_ref(), Type::OwnedStore { .. }) {
        return dynamic_store::check_carrier(module, ty, span, budget);
    }
    if matches!(
        inner.as_ref(),
        Type::Registration { .. } | Type::AsyncRegistration { .. }
    ) {
        return registration::carrier(module, ty, span, budget);
    }
    if matches!(inner.as_ref(), Type::TaskI32 | Type::Task { .. }) {
        return tasks::carrier(module, ty, span, budget);
    }
    if let Type::Mutex { payload } = inner.as_ref() {
        budget.spend(module.resources.len())?;
        let name = format!("@builtin:Mutex<{}>", payload.fingerprint());
        if module
            .resources
            .iter()
            .filter(|r| r.name == name && r.payload == **inner && r.teardown.is_none())
            .count()
            != 1
        {
            return Err(reject(
                "E-VERIFY-MUTEX",
                span,
                "mutex head lacks one exact sealed resource definition",
            ));
        }
        return Ok(());
    }
    if inner.as_ref() == &Type::ByteBuffer {
        return byte_buffers::check_carrier(module, ty, span, budget);
    }
    let Type::Record { name, .. } = inner.as_ref() else {
        return Err(unsupported(
            "resource payload must be a finite Copy record",
            span,
        ));
    };
    charge_payload(inner, budget)?;
    check_type(inner, span)?;
    // Equal-length string comparison examines at most name.len() bytes. The
    // table has unique names, so at most one full payload comparison follows.
    budget.spend(
        module
            .resources
            .len()
            .saturating_mul(name.len().saturating_add(1)),
    )?;
    if !module
        .resources
        .iter()
        .any(|r| &r.name == name && &r.payload == inner.as_ref())
    {
        return Err(reject(
            "E-VERIFY-RESOURCE",
            span,
            "resource type is absent from its nominal definition table",
        ));
    }
    Ok(())
}

pub(super) fn check_definitions(
    module: &TypedModule,
    limits: &Limits,
    budget: &mut Budget,
) -> Check<()> {
    if module.resources.len() > limits.max_nodes {
        return Err(limit(
            "resource definition count exceeds the selected bound",
        ));
    }
    budget.spend(module.resources.len())?;
    let mut names = BTreeSet::new();
    let mut entries = BTreeSet::new();
    for resource in &module.resources {
        budget.spend(resource.name.len().saturating_add(1))?;
        check_type_shape(&resource.payload, &module.target, limits, budget)?;
        if dynamic_store::check_definition(module, resource, limits, budget)? {
            if !names.insert(&resource.name) {
                return Err(reject(
                    "E-VERIFY-OWNED-STORE",
                    None,
                    "Store definition must be unique",
                ));
            }
            continue;
        }
        if matches!(resource.payload, Type::Mutex { .. })
            || resource.name.starts_with("@builtin:Mutex<")
        {
            if !names.insert(&resource.name) {
                return Err(reject(
                    "E-VERIFY-MUTEX",
                    None,
                    "mutex resource definition repeats",
                ));
            }
            generic_mutex::definition(module, resource, budget)?;
            continue;
        }
        if matches!(
            resource.payload,
            Type::Registration { .. } | Type::AsyncRegistration { .. }
        ) || resource.name.starts_with(registration::PREFIX)
            || resource.name.starts_with(registration::ASYNC_PREFIX)
        {
            if !names.insert(&resource.name) {
                return Err(reject(
                    "E-VERIFY-REGISTRATION",
                    None,
                    "registration definition must be unique",
                ));
            }
            registration::carrier(
                module,
                &Type::Managed(Box::new(resource.payload.clone())),
                None,
                budget,
            )?;
            continue;
        }
        if matches!(resource.payload, Type::Task { .. })
            || resource.name.starts_with(TASK_RESOURCE_NAME_PREFIX)
        {
            if !names.insert(&resource.name) {
                return Err(reject("E-VERIFY-TASK", None, "task definition repeats"));
            }
            tasks::carrier(
                module,
                &Type::Managed(Box::new(resource.payload.clone())),
                None,
                budget,
            )?;
            continue;
        }
        if resource.payload == Type::TaskI32 || resource.name == TASK_I32_RESOURCE_NAME {
            if resource.name != TASK_I32_RESOURCE_NAME
                || resource.payload != Type::TaskI32
                || resource.teardown.is_some()
                || !names.insert(&resource.name)
            {
                return Err(reject(
                    "E-VERIFY-TASK",
                    None,
                    "task definition must be unique and sealed without user teardown",
                ));
            }
            tasks::carrier(
                module,
                &Type::Managed(Box::new(Type::TaskI32)),
                None,
                budget,
            )?;
            continue;
        }
        if resource.payload == Type::ByteBuffer || resource.name == byte_buffers::NAME {
            byte_buffers::target(&module.target, None)?;
            if resource.name != byte_buffers::NAME
                || resource.payload != Type::ByteBuffer
                || resource.teardown.is_some()
                || !names.insert(&resource.name)
            {
                return Err(reject(
                    "E-VERIFY-BYTE-BUFFER",
                    None,
                    "byte storage definition must be unique, sealed and have no user teardown",
                ));
            }
            continue;
        }
        check_type(&resource.payload, None)?;
        if !matches!(&resource.payload, Type::Record { name, .. } if name == &resource.name)
            || !names.insert(&resource.name)
        {
            return Err(reject(
                "E-VERIFY-RESOURCE",
                None,
                "resource definitions require unique matching nominal Copy record payloads",
            ));
        }
        if let Some(id) = resource.teardown {
            let f = module.functions.get(id.0 as usize).ok_or_else(|| {
                reject(
                    "E-VERIFY-RESOURCE",
                    None,
                    "resource teardown function is absent",
                )
            })?;
            if !entries.insert(id)
                || f.foreign.is_some()
                || f.environment.is_some()
                || f.call_kind != CallKind::Direct
                || f.return_type != Type::Unit
                || f.result_mode != PassingMode::Infer
                || f.parameters.len() != 1
                || local_type(f, f.parameters[0], Some(f.span))?
                    != &Type::Desc(Box::new(resource.payload.clone()))
            {
                return Err(bad(
                    f.span,
                    "teardown requires one matching Desc parameter and an ordinary unit body",
                ));
            }
        }
    }
    budget.spend(module.functions.len())?;
    for f in &module.functions {
        match (&f.return_type, f.result_mode) {
            (Type::Managed(_), PassingMode::Create | PassingMode::Give) => {}
            (ty, PassingMode::Create | PassingMode::Give) if errors::managed_success(ty) => {}
            (Type::Desc(_), _) => {
                return Err(unsupported(
                    "resource description returns require exported source and management contracts",
                    Some(f.span),
                ));
            }
            (Type::Managed(_), _) => {
                return Err(bad(
                    f.span,
                    "managed result requires an explicit create or give result mode",
                ));
            }
            (ty, _) if errors::managed_success(ty) => {
                return Err(bad(
                    f.span,
                    "owned success result requires an explicit create or give result mode",
                ));
            }
            (_, PassingMode::Infer) => {}
            _ => {
                return Err(bad(
                    f.span,
                    "non-resource result must not claim a resource result mode",
                ));
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Root {
    Local(LocalId),
    // Field identity is exact even while its package shell is only partly
    // populated. A later instance at the same physical slot cannot revive an
    // earlier live description across a Take/Drop/Store transition.
    PackageField {
        local: LocalId,
        index: u32,
    },
    ArrayElement {
        local: LocalId,
        index: u32,
    },
    Parameter(u32),
    /// Exact checked Some(0) leaf of an Option input, without an owned debt.
    InputPayload(u32),
    OwnedCapture(u32),
    BorrowedCapture(u32),
    StoreParent(StoreParentRoot),
    // Private effect only: an actual Commit ends an old Store field instance.
    // This is never an ordinary write row or a runtime description origin.
    InvalidatedStore(StoreParentRoot),
}

fn option_store_origin(
    f: &TypedFunction,
    source: &crate::stored_flow::Source,
    budget: &mut Budget,
) -> Check<Root> {
    if source.retained_port.is_some() {
        return Err(bad(
            f.span,
            "Option description has an unrelated retained port",
        ));
    }
    if source.heap_element.is_none() {
        return match (
            source.base,
            source.leaf.as_slice(),
            source.inline.as_slice(),
        ) {
            (HoldRoot::Parameter(position), [zil_ir::PlaceProjection::Variant(selector)], [])
                if selector.variant == 1 && selector.index == 0 =>
            {
                budget.spend(1)?;
                Ok(Root::InputPayload(position))
            }
            _ => Err(bad(f.span, "Option description lacks a checked input leaf")),
        };
    }
    if !source.leaf.is_empty() {
        return Err(bad(f.span, "Store window has an unrelated stored selector"));
    }
    let (ty, root) = match source.base {
        HoldRoot::Local(local) => (
            &f.locals
                .get(local.0 as usize)
                .ok_or_else(|| bad(f.span, "Option manager local is absent"))?
                .ty,
            Root::Local(local),
        ),
        HoldRoot::Parameter(position) => {
            let local = *f
                .parameters
                .get(position as usize)
                .ok_or_else(|| bad(f.span, "Option manager parameter is absent"))?;
            (&f.locals[local.0 as usize].ty, Root::Parameter(position))
        }
        HoldRoot::Capture(_) | HoldRoot::Static => {
            return Err(bad(
                f.span,
                "Option description lacks an ordinary Store manager",
            ));
        }
    };
    let mut head = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    let mut root = root;
    for (depth, projection) in source.inline.iter().enumerate() {
        let zil_ir::PlaceProjection::Field(index) = projection else {
            return Err(bad(
                f.span,
                "Option Store header path is not an actual inline field",
            ));
        };
        let Type::Package { fields, .. } = head else {
            return Err(bad(
                f.span,
                "Option Store header is not an actual Package field",
            ));
        };
        budget.spend(fields.len())?;
        head = &fields
            .get(*index as usize)
            .ok_or_else(|| bad(f.span, "Option Store header field is absent"))?
            .1;
        if depth == 0
            && let Root::Local(local) = root
        {
            root = Root::PackageField {
                local,
                index: *index,
            };
        }
    }
    if !matches!(head, Type::Managed(payload) if matches!(payload.as_ref(), Type::OwnedStore { .. }))
    {
        return Err(bad(
            f.span,
            "Option description header is not an actual owned Store",
        ));
    }
    budget.spend(source.heap_element.as_ref().map_or(0, Vec::len))?;
    Ok(root)
}

// Actual Store carrier descriptions remain distinct from descriptions derived
// from a borrowed element. The parent responsibility identity is preserved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum StoreParentRoot {
    Local(LocalId),
    PackageField { local: LocalId, index: u32 },
    ArrayElement { local: LocalId, index: u32 },
    Parameter(u32),
    OwnedCapture(u32),
    BorrowedCapture(u32),
}
impl Root {
    fn parent(self) -> Self {
        match self {
            Self::StoreParent(StoreParentRoot::Local(value)) => Self::Local(value),
            Self::StoreParent(StoreParentRoot::PackageField { local, index }) => {
                Self::PackageField { local, index }
            }
            Self::StoreParent(StoreParentRoot::ArrayElement { local, index }) => {
                Self::ArrayElement { local, index }
            }
            Self::StoreParent(StoreParentRoot::Parameter(value)) => Self::Parameter(value),
            Self::StoreParent(StoreParentRoot::OwnedCapture(value)) => Self::OwnedCapture(value),
            Self::StoreParent(StoreParentRoot::BorrowedCapture(value)) => {
                Self::BorrowedCapture(value)
            }
            other => other,
        }
    }
    fn store_parent(self) -> Self {
        match self.parent() {
            Self::Local(value) => Self::StoreParent(StoreParentRoot::Local(value)),
            Self::PackageField { local, index } => {
                Self::StoreParent(StoreParentRoot::PackageField { local, index })
            }
            Self::ArrayElement { local, index } => {
                Self::StoreParent(StoreParentRoot::ArrayElement { local, index })
            }
            Self::Parameter(value) => Self::StoreParent(StoreParentRoot::Parameter(value)),
            Self::InputPayload(_) => self,
            Self::OwnedCapture(value) => Self::StoreParent(StoreParentRoot::OwnedCapture(value)),
            Self::BorrowedCapture(value) => {
                Self::StoreParent(StoreParentRoot::BorrowedCapture(value))
            }
            Self::StoreParent(_) | Self::InvalidatedStore(_) => unreachable!(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct State {
    debt: Vec<u8>,
    fresh: Vec<u8>,
    // Independent SSA Package field provenance. These bytes do not add debt:
    // a complete Package still moves as one value, with local partial fields
    // checked by the existing responsibility slots.
    package_fresh: field_fresh::Fields,
    // Actual local/SSA Managed leaves of new Product functions, independently
    // rebuilt here. Empty on the historical flat path; never a public proof.
    product_fresh: Vec<u8>,
    desc: Vec<BTreeSet<Root>>,
    retirement: Vec<replacements::Sources>,
    // An edge a -> b means b may have been initialized later while a remained live.
    newer: Vec<BTreeSet<usize>>,
}

struct Slots {
    managed_local: Vec<Option<usize>>,
    managed_value: Vec<Option<usize>>,
    // Package shells participate in local cleanup order; their individual
    // fields do not. Field debts follow all SSA slots and precede captures so
    // they still participate in the ordinary-return conservation check.
    package_fields: Vec<Vec<Option<usize>>>,
    package_fresh_plan: field_fresh::Plan,
    product: product::Plan,
    desc_local: Vec<Option<usize>>,
    desc_value: Vec<Option<usize>>,
    capture_debt: Vec<Option<usize>>,
    package_view: Vec<Option<u32>>,
    package_capture_local: Vec<Option<LocalId>>,
    // Package views alias capture responsibility, although their reusable field
    // instructions address local slots. These debts are never ordinary locals.
    retained_debt: Vec<bool>,
    capture_start: usize,
    local_count: usize,
    debt_count: usize,
    desc_count: usize,
    retirement: bool,
}

// Resource accounting observes the physical owning Product. Full signature
// and operand equality elsewhere retain the lifetime-qualified wrapper.
fn package_storage(ty: &Type) -> Option<&[(String, Type)]> {
    match ty {
        Type::Package { fields, .. } => Some(fields),
        Type::BorrowedStorage { storage, .. } => match storage.as_ref() {
            Type::Package { fields, .. } => Some(fields),
            _ => None,
        },
        _ => None,
    }
}

/// A physical/profile dispatch only; the call certificate proves its origin.
pub(super) fn qualified_package_address(ty: &Type) -> bool {
    matches!(ty, Type::Address { mutable: false, inner }
        if matches!(inner.as_ref(), Type::BorrowedStorage { storage, .. }
            if matches!(storage.as_ref(), Type::Package { .. })))
}

impl Slots {
    fn new(
        module: &TypedModule,
        f: &TypedFunction,
        views: &packages::CaptureViews,
        budget: &mut Budget,
    ) -> Check<Self> {
        if budget.sum_function(f.id) {
            for ty in f.locals.iter().map(|l| &l.ty).chain(&f.values) {
                charge_payload(ty, budget)?;
            }
        }
        budget.spend(
            f.locals
                .len()
                .saturating_add(f.values.len())
                .saturating_mul(2),
        )?;
        budget.spend(module.closures.len().saturating_mul(2))?;
        let mut owned_closures = Vec::with_capacity(module.closures.len());
        let mut borrowed_closures = Vec::with_capacity(module.closures.len());
        for closure in &module.closures {
            budget.spend(closure.captures.len().saturating_mul(2))?;
            let mut inline_sum_owner = false;
            for ty in &closure.captures {
                if budget.inline_store_root(ty)? {
                    inline_sum_owner |= dynamic_store::inline_sum_capture(ty, budget)?;
                }
            }
            owned_closures.push(
                closure.kind == CallableKind::FnOnce
                    || inline_sum_owner
                    || closure.captures.iter().any(|ty| {
                        matches!(
                            ty,
                            Type::Managed(_) | Type::Package { .. } | Type::Closure(_)
                        )
                    }),
            );
            borrowed_closures.push(
                closure
                    .captures
                    .iter()
                    .any(|ty| matches!(ty, Type::Desc(_))),
            );
        }
        let mut debt_count = 0usize;
        let mut desc_count = 0usize;
        let sum_extension = budget.sums;
        let head_descriptions = budget.head_descriptions;
        let inline_store_addresses = budget.inline_store_addresses;
        // Only actual Sum addresses containing an inline Store need this
        // additional origin row. Leave old-only functions' slot inventory and
        // traversal unchanged.
        let (local_sum_address, value_sum_address) = if inline_store_addresses {
            budget.spend(f.locals.len().saturating_add(f.values.len()))?;
            let mut local = Vec::with_capacity(f.locals.len());
            let mut value = Vec::with_capacity(f.values.len());
            for declaration in &f.locals {
                local.push(match &declaration.ty {
                    Type::Address { inner, .. } if budget.inline_store_root(&declaration.ty)? => {
                        dynamic_store::inline_sum_capture(inner, budget)?
                    }
                    _ => false,
                });
            }
            for ty in &f.values {
                value.push(match ty {
                    Type::Address { inner, .. } if budget.inline_store_root(ty)? => {
                        dynamic_store::inline_sum_capture(inner, budget)?
                    }
                    _ => false,
                });
            }
            (local, value)
        } else {
            (Vec::new(), Vec::new())
        };
        let mut slot = |ty: &Type, managed: bool, sum_address: bool| {
            let count = if managed {
                &mut debt_count
            } else {
                &mut desc_count
            };
            if (managed && package_storage(ty).is_some())
                || matches!(
                    (ty, managed),
                    (Type::Managed(_) | Type::MutexGuard { .. } | Type::OwnedDyn { .. }, true)
                        | (Type::Desc(_) | Type::MutexGuard { .. }, false)
                )
                || (!managed && matches!(ty, Type::Address { inner, .. }
                    if matches!(inner.as_ref(), Type::MutexGuard { .. })))
                || (!managed && matches!(ty, Type::Address { inner, .. }
                    if matches!(inner.as_ref(), Type::Dyn { .. })))
                || (managed
                && (errors::owned(ty, sum_extension) || (sum_extension && sums::owned(ty))))
                || (managed
                    && matches!(ty, Type::Array { element, length }
                    if *length > 0 && matches!(element.as_ref(), Type::Package { .. } | Type::Closure(_))))
                || matches!(ty, Type::Closure(id) if if managed {
                    owned_closures[id.0 as usize]
                } else { borrowed_closures[id.0 as usize] })
                || (!managed
                    && (head_descriptions || matches!(ty, Type::Address { inner, .. }
                        if matches!(inner.as_ref(), Type::BorrowedStorage { storage, .. }
                            if matches!(storage.as_ref(), Type::Package { .. }))))
                    && (head_address(ty)
                        || ty.is_package_address()
                        || matches!(ty, Type::Address { inner, .. }
                            if package_storage(inner).is_some())))
                // A projected Store head can be reached through an ordinary
                // Sum address. Keep its actual address source through local
                // copies until BorrowPlaceAddress selects the checked field.
                || (!managed && sum_address)
                || (!managed
                    && matches!(ty,
                    Type::Address { inner, .. }
                    if matches!(inner.as_ref(), Type::Closure(id) if borrowed_closures[id.0 as usize])))
            {
                let id = *count;
                *count += 1;
                Some(id)
            } else {
                None
            }
        };
        // Collect only actual Package declarations during the existing local
        // slot pass. A function without a Package keeps the former traversal
        // and allocation cost; it does not build or scan an empty field table.
        let mut package_declarations = Vec::new();
        let mut array_declarations = Vec::new();
        let mut managed_local = Vec::with_capacity(f.locals.len());
        for (local, declaration) in f.locals.iter().enumerate() {
            managed_local.push(slot(&declaration.ty, true, false));
            if let Some(fields) = package_storage(&declaration.ty) {
                budget.spend(2)?;
                package_declarations.push((local, fields));
            }
            if let Type::Array { element, length } = &declaration.ty
                && matches!(element.as_ref(), Type::Package { .. } | Type::Closure(_))
            {
                let width = match element.as_ref() {
                    Type::Package { fields, .. } => fields.len().saturating_add(1),
                    _ => 1,
                };
                budget.spend((*length as usize).saturating_mul(width))?;
                array_declarations.push((local, *length));
            }
        }
        let local_count = managed_local.iter().flatten().count();
        let managed_value = f.values.iter().map(|t| slot(t, true, false)).collect();
        let desc_local = f
            .locals
            .iter()
            .enumerate()
            .map(|(index, local)| {
                slot(
                    &local.ty,
                    false,
                    local_sum_address.get(index).copied().unwrap_or(false),
                )
            })
            .collect();
        let desc_value = f
            .values
            .iter()
            .enumerate()
            .map(|(index, ty)| {
                slot(
                    ty,
                    false,
                    value_sum_address.get(index).copied().unwrap_or(false),
                )
            })
            .collect();
        budget.spend(f.values.len())?;
        let mut package_widths = Vec::new();
        for (value, ty) in f.values.iter().enumerate() {
            if let Some(fields) = package_storage(ty) {
                if package_widths.is_empty() {
                    budget.spend(f.values.len())?;
                    package_widths = vec![0; f.values.len()];
                }
                // Field byte allocation is charged in empty, before it occurs.
                budget.spend(1)?;
                package_widths[value] = fields.len();
            }
        }
        let mut package_fields = if package_declarations.is_empty() && array_declarations.is_empty()
        {
            Vec::new()
        } else {
            budget.spend(f.locals.len())?;
            vec![Vec::new(); f.locals.len()]
        };
        for (local, fields) in package_declarations {
            budget.spend(fields.len())?;
            package_fields[local] = fields
                .iter()
                .map(|(_, ty)| {
                    if packages::owned_field(ty, sum_extension) {
                        let next = debt_count;
                        debt_count += 1;
                        Some(next)
                    } else {
                        None
                    }
                })
                .collect();
        }
        for (local, length) in array_declarations {
            package_fields[local] = (0..length)
                .map(|_| {
                    let next = debt_count;
                    debt_count += 1;
                    Some(next)
                })
                .collect();
        }
        let capture_start = debt_count;
        let captures = f
            .environment
            .map_or(&[][..], |id| &module.closures[id.0 as usize].captures);
        budget.spend(captures.len())?;
        let capture_debt = captures
            .iter()
            .enumerate()
            .map(|(index, ty)| -> Check<Option<usize>> {
                if matches!(ty, Type::Managed(_) | Type::Closure(_))
                    || (budget.inline_store_root(ty)?
                        && dynamic_store::inline_sum_capture(ty, budget)?)
                {
                    let next = debt_count;
                    debt_count += 1;
                    Ok(Some(next))
                } else if matches!(ty, Type::Package { .. }) {
                    let local =
                        views.by_capture[index].expect("independently checked Package binding");
                    Ok(managed_local[local.0 as usize])
                } else {
                    Ok(None)
                }
            })
            .collect::<Check<Vec<_>>>()?;
        let mut retained_debt = Vec::new();
        if !views.by_local.is_empty() {
            budget.spend(
                capture_start
                    .saturating_add(views.by_local.len())
                    .saturating_add(views.by_capture.len().saturating_mul(2)),
            )?;
            retained_debt = vec![false; capture_start];
            for local in views.by_capture.iter().flatten() {
                retained_debt[managed_local[local.0 as usize].expect("Package shell debt")] = true;
                budget.spend(package_fields[local.0 as usize].len())?;
                for &field in package_fields[local.0 as usize].iter().flatten() {
                    retained_debt[field] = true;
                }
            }
        }
        let product = product::Plan::new(f, budget)?;
        let package_fresh_plan = field_fresh::Plan::new(package_widths, budget)?;
        Ok(Self {
            managed_local,
            managed_value,
            package_fields,
            package_fresh_plan,
            product,
            desc_local,
            desc_value,
            capture_debt,
            package_view: views.by_local.clone(),
            package_capture_local: views.by_capture.clone(),
            retained_debt,
            capture_start,
            local_count,
            debt_count,
            desc_count,
            retirement: false,
        })
    }

    fn fields(&self, local: LocalId) -> &[Option<usize>] {
        self.package_fields
            .get(local.0 as usize)
            .map_or(&[], Vec::as_slice)
    }

    fn view(&self, local: LocalId) -> Option<u32> {
        self.package_view.get(local.0 as usize).copied().flatten()
    }

    fn retained(&self, debt: usize) -> bool {
        self.retained_debt.get(debt).copied().unwrap_or(false)
    }

    fn empty(&self, budget: &mut Budget) -> Check<State> {
        budget.spend(
            self.debt_count
                .saturating_mul(2)
                .saturating_add(self.desc_count)
                .saturating_add(self.local_count),
        )?;
        // An input or unknown producer still starts every actual field INPUT.
        let package_fresh = self.package_fresh_plan.empty(budget)?;
        budget.spend(self.product.width())?;
        if self.retirement {
            budget.spend(self.desc_count)?;
        }
        Ok(State {
            retirement: if self.retirement {
                vec![replacements::Sources::default(); self.desc_count]
            } else {
                vec![]
            },
            debt: vec![EMPTY; self.debt_count],
            fresh: vec![0; self.debt_count],
            package_fresh,
            product_fresh: vec![INPUT; self.product.width()],
            desc: vec![BTreeSet::new(); self.desc_count],
            newer: vec![BTreeSet::new(); self.local_count],
        })
    }
}

fn state_cost(s: &State, budget: &mut Budget) -> Check<()> {
    if !s.retirement.is_empty() {
        budget.spend(s.retirement.len())?;
        for sources in &s.retirement {
            sources.cost(budget)?;
        }
    }
    budget.spend(
        s.debt
            .len()
            .saturating_mul(2)
            .saturating_add(s.product_fresh.len())
            .saturating_add(s.desc.len())
            .saturating_add(s.newer.len()),
    )?;
    s.package_fresh.cost(budget)?;
    for row in &s.desc {
        budget.spend(row.len())?;
    }
    for row in &s.newer {
        budget.spend(row.len())?;
    }
    Ok(())
}

fn clone_state(s: &State, budget: &mut Budget) -> Check<State> {
    state_cost(s, budget)?;
    Ok(s.clone())
}

fn merge(target: &mut State, source: &State, budget: &mut Budget) -> Check<bool> {
    state_cost(source, budget)?;
    let mut changed = false;
    for (a, b) in target.retirement.iter_mut().zip(&source.retirement) {
        changed |= a.merge(b, budget)?;
    }
    for (a, b) in target
        .debt
        .iter_mut()
        .zip(&source.debt)
        .chain(target.fresh.iter_mut().zip(&source.fresh))
        .chain(target.product_fresh.iter_mut().zip(&source.product_fresh))
    {
        let next = *a | *b;
        changed |= next != *a;
        *a = next;
    }
    changed |= target.package_fresh.merge(&source.package_fresh, budget)?;
    for (a, b) in target.desc.iter_mut().zip(&source.desc) {
        budget.spend(a.len().saturating_add(b.len()))?;
        let before = a.len();
        a.extend(b);
        changed |= a.len() != before;
    }
    for (a, b) in target.newer.iter_mut().zip(&source.newer) {
        budget.spend(a.len().saturating_add(b.len()))?;
        let before = a.len();
        a.extend(b);
        changed |= a.len() != before;
    }
    Ok(changed)
}

fn forget_order(state: &mut State, local: usize, budget: &mut Budget) -> Check<()> {
    budget.spend(state.newer.len())?;
    state.newer[local].clear();
    for row in &mut state.newer {
        row.remove(&local);
    }
    Ok(())
}

fn reverse(slots: &Slots, live: &mut [bool], instruction: &Instruction) {
    if let Some(value) = instruction.result
        && let Some(index) = slots.desc_value[value.0 as usize]
    {
        live[index] = false;
    }
    match instruction.kind {
        InstructionKind::Store { local, .. }
        | InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local) => {
            if let Some(index) = slots.desc_local[local.0 as usize] {
                live[index] = false;
            }
        }
        InstructionKind::Load(local)
        | InstructionKind::BorrowLocal { local }
        | InstructionKind::BorrowLocalMut { local }
        | InstructionKind::TakeLocal { local }
        | InstructionKind::CallClosureShared { callee: local, .. }
        | InstructionKind::CallClosureMut { callee: local, .. } => {
            if let Some(index) = slots.desc_local[local.0 as usize] {
                live[index] = true;
            }
        }
        _ => {}
    }
    for operand in instruction_operands(&instruction.kind) {
        if let Some(index) = slots.desc_value[operand.0 as usize] {
            live[index] = true;
        }
    }
}

fn liveness(
    f: &TypedFunction,
    graph: &FunctionGraph,
    slots: &Slots,
    budget: &mut Budget,
) -> Check<desc_liveness::History> {
    let n = f.blocks.len();
    budget.spend(
        n.checked_mul(slots.desc_count)
            .and_then(|v| v.checked_mul(2))
            .ok_or_else(|| limit("resource liveness overflow"))?,
    )?;
    let mut input = vec![vec![false; slots.desc_count]; n];
    let mut output = input.clone();
    let mut pending: VecDeque<_> = (0..n).filter(|&b| graph.reachable[b]).collect();
    budget.spend(n)?;
    let mut queued = graph.reachable.clone();
    while let Some(b) = pending.pop_front() {
        queued[b] = false;
        budget.spend(slots.desc_count.saturating_add(1))?;
        let mut live = vec![false; slots.desc_count];
        for &next in &graph.successors[b] {
            budget.spend(slots.desc_count)?;
            for (a, b) in live.iter_mut().zip(&input[next]) {
                *a |= *b;
            }
        }
        output[b].clone_from(&live);
        for i in f.blocks[b].instructions.iter().rev() {
            budget.spend(instruction_operands(&i.kind).count().saturating_add(1))?;
            reverse(slots, &mut live, i);
        }
        if input[b] != live {
            input[b] = live;
            for &p in &graph.predecessors[b] {
                budget.spend(1)?;
                if graph.reachable[p] && !queued[p] {
                    queued[p] = true;
                    pending.push_back(p);
                }
            }
        }
    }
    desc_liveness::history(f, slots, &output, budget)
}

fn description_live(
    state: &State,
    slots: &Slots,
    row: usize,
    span: Span,
    budget: &mut Budget,
) -> Check<()> {
    budget.spend(state.desc[row].len().saturating_add(1))?;
    if state.desc[row].is_empty() {
        return Err(bad(
            span,
            "resource description has no independently grounded source",
        ));
    }
    for root in &state.desc[row] {
        let root = root.parent();
        let slot = match &root {
            Root::Local(local) => slots.managed_local[local.0 as usize],
            Root::PackageField { local, index } | Root::ArrayElement { local, index } => {
                slots.package_fields[local.0 as usize][*index as usize]
            }
            Root::OwnedCapture(index) => slots.capture_debt[*index as usize],
            Root::Parameter(_) | Root::InputPayload(_) | Root::BorrowedCapture(_) => None,
            Root::StoreParent(_) => unreachable!(),
            Root::InvalidatedStore(_) => {
                return Err(bad(
                    span,
                    "effect marker is not a resource description origin",
                ));
            }
        };
        if let Some(slot) = slot
            && state.debt[slot] != LIVE
        {
            return Err(bad(
                span,
                "description may refer to an uninitialized, moved or discharged resource head",
            ));
        }
    }
    Ok(())
}

fn no_live_description(
    state: &State,
    live: desc_liveness::Row<'_>,
    root: Root,
    span: Span,
    budget: &mut Budget,
) -> Check<()> {
    for (index, roots) in state.desc.iter().take(live.len()).enumerate() {
        budget.spend(1)?;
        if live.get(index, budget)? {
            budget.spend(roots.len())?;
            if roots.iter().any(|candidate| {
                candidate.parent() == root.parent()
                    || matches!(
                        (candidate.parent(), root.parent()),
                        (Root::PackageField { local: a, .. } | Root::ArrayElement { local: a, .. }, Root::Local(b)) if a == b
                    )
            }) {
                return Err(bad(
                    span,
                    "moving, clearing or reconstructing a resource head would invalidate a live description",
                ));
            }
        }
    }
    Ok(())
}

struct Transfer<'a> {
    module: &'a TypedModule,
    f: &'a TypedFunction,
    slots: &'a Slots,
    after: &'a desc_liveness::History,
    effects: &'a [BTreeSet<Root>],
    sum_rows: Option<&'a [Vec<sums::Bridge>]>,
    replacements: Option<&'a replacements::Plan>,
}

impl Transfer<'_> {
    fn package_shell(
        &self,
        state: &State,
        local: LocalId,
        span: Span,
        validate: bool,
    ) -> Check<()> {
        let debt = self.slots.managed_local[local.0 as usize]
            .expect("typed package local has an independent shell debt");
        if validate && state.debt[debt] != LIVE {
            return Err(bad(
                span,
                "package field operation requires a live shell on every incoming path",
            ));
        }
        Ok(())
    }

    fn consume(&self, state: &mut State, value: ValueId, span: Span, validate: bool) -> Check<()> {
        if let Some(index) = self.slots.managed_value[value.0 as usize] {
            if validate && state.debt[index] != LIVE {
                return Err(bad(
                    span,
                    "managed SSA value was not available exactly once",
                ));
            }
            state.debt[index] = MOVED;
        }
        Ok(())
    }

    fn write_roots(
        &self,
        roots: &BTreeSet<Root>,
        written: &mut BTreeSet<Root>,
        span: Span,
        validate: bool,
        budget: &mut Budget,
    ) -> Check<()> {
        budget.spend(roots.len().saturating_add(written.len()))?;
        if validate
            && self
                .f
                .environment
                .is_some_and(|id| self.module.closures[id.0 as usize].kind == CallableKind::Fn)
            && roots
                .iter()
                .any(|root| matches!(root.parent(), Root::OwnedCapture(_))
                    || matches!(root.parent(), Root::PackageField { local, .. } if self.slots.view(local).is_some()))
        {
            return Err(bad(
                span,
                "a shared Fn entry cannot write an owned resource capture, including through a callee",
            ));
        }
        written.extend(roots.iter().map(|root| root.parent()).filter(|root| {
            !matches!(
                root,
                Root::Local(_) | Root::PackageField { .. } | Root::ArrayElement { .. }
            )
        }));
        Ok(())
    }

    fn block(
        &self,
        b: usize,
        state: &mut State,
        validate: bool,
        written: &mut BTreeSet<Root>,
        budget: &mut Budget,
    ) -> Check<()> {
        let f = self.f;
        let slots = self.slots;
        for (i, instruction) in f.blocks[b].instructions.iter().enumerate() {
            budget.spend(
                instruction_operands(&instruction.kind)
                    .count()
                    .saturating_add(1),
            )?;
            let span = instruction.span;
            let task_before = if validate {
                task_frames::begin(f, state, budget)?
            } else {
                None
            };
            if validate
                && matches!(
                    instruction.kind,
                    InstructionKind::TaskYield | InstructionKind::SuspendCall { .. }
                )
            {
                task_frames::capture(f, b, i, state, budget)?;
            }
            if validate {
                for operand in instruction_operands(&instruction.kind) {
                    if let Some(row) = slots.desc_value[operand.0 as usize] {
                        description_live(state, slots, row, span, budget)?;
                    }
                }
            }
            let result = instruction
                .result
                .and_then(|v| slots.managed_value[v.0 as usize]);
            let mut fresh_result = None;
            if validate && result.is_some_and(|index| state.debt[index] & LIVE != 0) {
                return Err(bad(
                    span,
                    "a loop redefines a managed SSA result before its previous responsibility was consumed",
                ));
            }
            if validate
                && !slots.package_view.is_empty()
                && f.environment
                    .is_some_and(|id| self.module.closures[id.0 as usize].kind == CallableKind::Fn)
            {
                let written_view = match instruction.kind {
                    InstructionKind::TakePackageField { local, .. }
                    | InstructionKind::DropPackageField { local, .. }
                    | InstructionKind::StorePackageField { local, .. }
                    | InstructionKind::TakeLocal { local }
                    | InstructionKind::DropPackage { local }
                    | InstructionKind::Store { local, .. }
                    | InstructionKind::BorrowPackageField {
                        local,
                        mutable: true,
                        ..
                    } => slots.view(local).is_some(),
                    _ => false,
                };
                if written_view {
                    return Err(bad(
                        span,
                        "a shared Fn entry cannot write or consume an owned Package capture",
                    ));
                }
            }
            if let Some((store, true)) = dynamic_store::access(&instruction.kind) {
                let row = slots.desc_value[store.0 as usize].expect("checked Store description");
                self.write_roots(&state.desc[row], written, span, validate, budget)?;
            }
            if let Some(plan) = self.replacements {
                plan.step(self, state, instruction, i, b, validate, written, budget)?;
            }
            match &instruction.kind {
                InstructionKind::MutexCreate { value } => {
                    if validate {
                        tasks::require_synchronous_value(
                            self.module,
                            f,
                            *value,
                            budget,
                            Some(span),
                        )?;
                    }
                    self.consume(state, *value, span, validate)?;
                    fresh_result = Some(FRESH);
                }
                InstructionKind::MakeDynOwned { source, .. } => {
                    self.consume(state, *source, span, validate)?;
                    fresh_result = Some(FRESH);
                }
                InstructionKind::BorrowDynOwned { local, .. } => {
                    let owner = slots.managed_local[local.0 as usize]
                        .ok_or_else(|| bad(span, "owned dyn borrow lacks real owner debt"))?;
                    if validate && state.debt[owner] != LIVE {
                        return Err(bad(span, "owned dyn borrow requires live owner"));
                    }
                    let dst = instruction
                        .result
                        .and_then(|v| slots.desc_value[v.0 as usize])
                        .ok_or_else(|| bad(span, "owned dyn borrow has no description slot"))?;
                    state.desc[dst] = BTreeSet::from([Root::Local(*local)]);
                }
                InstructionKind::MutexLock { mutex } => {
                    let src = slots.desc_value[mutex.0 as usize]
                        .ok_or_else(|| bad(span, "lock has no actual mutex head description"))?;
                    let dst = instruction
                        .result
                        .and_then(|v| slots.desc_value[v.0 as usize])
                        .ok_or_else(|| bad(span, "lock produced no guard source"))?;
                    if validate {
                        description_live(state, slots, src, span, budget)?;
                    }
                    budget.spend(state.desc[src].len())?;
                    state.desc[dst] = state.desc[src].clone();
                    fresh_result = Some(INPUT);
                }
                InstructionKind::MutexValue { guard } => {
                    if let Some(dst) = instruction
                        .result
                        .and_then(|v| slots.desc_value[v.0 as usize])
                    {
                        let src = slots.desc_value[guard.0 as usize]
                            .ok_or_else(|| bad(span, "guard projection has no real source"))?;
                        if validate {
                            description_live(state, slots, src, span, budget)?;
                        }
                        budget.spend(state.desc[src].len())?;
                        state.desc[dst] = state.desc[src].clone();
                    }
                }
                InstructionKind::MutexUnlock { guard } => {
                    self.consume(state, *guard, span, validate)?;
                }
                InstructionKind::MakeOwnedTuple { fields }
                | InstructionKind::MakeVariant { fields, .. }
                    if result.is_some() =>
                {
                    budget.spend(fields.len())?;
                    for &value in fields {
                        self.consume(state, value, span, validate)?;
                    }
                    fresh_result = Some(INPUT);
                }
                InstructionKind::VariantTagLocal { local, .. }
                | InstructionKind::ReadVariantLocal { local, .. }
                | InstructionKind::BorrowVariantLocal { local, .. }
                | InstructionKind::BorrowFieldLocal { local, .. }
                | InstructionKind::BorrowPlaceLocal { local, .. }
                | InstructionKind::PlaceTagLocal { local, .. } => {
                    if let Some(debt) = slots.managed_local[local.0 as usize]
                        && validate
                        && state.debt[debt] != LIVE
                    {
                        return Err(bad(
                            span,
                            "Enum observation requires a live complete carrier",
                        ));
                    }
                    if matches!(
                        instruction.kind,
                        InstructionKind::BorrowFieldLocal { .. }
                            | InstructionKind::BorrowPlaceLocal { .. }
                            | InstructionKind::BorrowVariantLocal { .. }
                    ) && let Some(row) = instruction
                        .result
                        .and_then(|value| slots.desc_value[value.0 as usize])
                    {
                        // This kernel's Sum flow checks the actual selected
                        // subtree; descriptor lifetime remains on its shell.
                        state.desc[row] = BTreeSet::from([Root::Local(*local)]);
                    }
                }
                InstructionKind::TakeTupleField { local, .. }
                | InstructionKind::TakeVariantField { local, .. } => {
                    let parent = slots.managed_local[local.0 as usize].ok_or_else(|| {
                        bad(span, "selective source is not an actual owning local")
                    })?;
                    if validate {
                        if state.debt[parent] != LIVE {
                            return Err(bad(span, "selective source responsibility is not live"));
                        }
                        no_live_description(
                            state,
                            self.after.row(b, i),
                            Root::Local(*local),
                            span,
                            budget,
                        )?;
                    }
                    if result.is_some() {
                        fresh_result = Some(INPUT);
                    }
                }
                InstructionKind::UnpackVariant { local, fields, .. }
                | InstructionKind::UnpackTuple { local, fields } => {
                    let source = slots.managed_local[local.0 as usize].expect("typed owned Enum");
                    budget.spend(fields.len())?;
                    // Check every destination before changing any debt. Copy
                    // vacancy and storage are checked by the independent Sum
                    // and cleanup flows; these are the real owning debts.
                    if validate {
                        if state.debt[source] != LIVE {
                            return Err(bad(span, "Unpack requires a live Enum exactly once"));
                        }
                        no_live_description(
                            state,
                            self.after.row(b, i),
                            Root::Local(*local),
                            span,
                            budget,
                        )?;
                        for &target in fields {
                            if let Some(debt) = slots.managed_local[target.0 as usize] {
                                let nested = slots.fields(target);
                                budget.spend(nested.len())?;
                                if state.debt[debt] & LIVE != 0
                                    || nested.iter().flatten().any(|&d| state.debt[d] & LIVE != 0)
                                {
                                    return Err(bad(
                                        span,
                                        "Unpack would overwrite a target responsibility",
                                    ));
                                }
                                no_live_description(
                                    state,
                                    self.after.row(b, i),
                                    Root::Local(target),
                                    span,
                                    budget,
                                )?;
                            }
                        }
                    }
                    state.debt[source] = MOVED;
                    forget_order(state, source, budget)?;
                    for &target in fields.iter().rev() {
                        if let Some(debt) = slots.managed_local[target.0 as usize] {
                            forget_order(state, debt, budget)?;
                            for other in 0..slots.local_count {
                                budget.spend(1)?;
                                if other != debt
                                    && !slots.retained(other)
                                    && state.debt[other] & LIVE != 0
                                {
                                    state.newer[other].insert(debt);
                                }
                            }
                            state.debt[debt] = LIVE;
                            state.fresh[debt] = INPUT;
                            budget.spend(slots.fields(target).len())?;
                            for &field in slots.fields(target).iter().flatten() {
                                state.debt[field] = LIVE;
                                state.fresh[field] = INPUT;
                            }
                        }
                    }
                }
                InstructionKind::OwnedStorePush { value, .. } => {
                    self.consume(state, *value, span, validate)?;
                    fresh_result = Some(INPUT);
                }
                InstructionKind::OwnedStorePop { .. } => {
                    fresh_result = Some(INPUT);
                }
                InstructionKind::MakeErrorUnion { value, is_error } if result.is_some() => {
                    let union = &f.values[instruction.result.unwrap().0 as usize];
                    let fresh = if errors::managed_success(union) {
                        if *is_error {
                            0 // This tag creates no success payload obligation.
                        } else {
                            let source =
                                slots.managed_value[value.0 as usize].ok_or_else(|| {
                                    bad(span, "owned success has no resource carrier")
                                })?;
                            state.fresh[source]
                        }
                    } else {
                        INPUT
                    };
                    self.consume(state, *value, span, validate)?;
                    fresh_result = Some(fresh);
                }
                InstructionKind::TakeErrorUnion { value, .. } => {
                    let fresh = if errors::managed_success(&f.values[value.0 as usize]) {
                        let source = slots.managed_value[value.0 as usize]
                            .ok_or_else(|| bad(span, "selected union has no responsibility"))?;
                        state.fresh[source]
                    } else {
                        INPUT
                    };
                    self.consume(state, *value, span, validate)?;
                    if result.is_some() {
                        fresh_result = Some(fresh);
                    }
                }
                InstructionKind::ErrorTag { value }
                    if errors::owned(&f.values[value.0 as usize], budget.sums) =>
                {
                    let debt = slots.managed_value[value.0 as usize].expect("typed owned union");
                    if validate && state.debt[debt] != LIVE {
                        return Err(bad(span, "error tag requires a live owned union"));
                    }
                }
                InstructionKind::BindPackageCapture { .. } => {}
                InstructionKind::EndPackageCapture { local, .. } => {
                    if validate {
                        no_live_description(
                            state,
                            self.after.row(b, i),
                            Root::Local(*local),
                            span,
                            budget,
                        )?;
                    }
                }
                InstructionKind::TaskPoll { .. } => {
                    if result.is_some() {
                        fresh_result = Some(INPUT);
                    }
                }
                InstructionKind::SuspendCall {
                    function,
                    arguments,
                } => {
                    budget.spend(arguments.len())?;
                    for &value in arguments {
                        self.consume(state, value, span, validate)?;
                    }
                    if result.is_some() {
                        fresh_result = Some(
                            if self.module.functions[function.0 as usize].result_mode
                                == PassingMode::Create
                            {
                                FRESH
                            } else {
                                INPUT
                            },
                        );
                    }
                }
                InstructionKind::TaskCreate { arguments, .. } => {
                    budget.spend(arguments.len())?;
                    for &v in arguments {
                        if matches!(&f.values[v.0 as usize],Type::Managed(p) if matches!(p.as_ref(),Type::AsyncRegistration{..}))
                        {
                            return Err(bad(
                                span,
                                "live pollable TaskCreate input lacks synchronous allocation-failure cleanup",
                            ));
                        }
                        self.consume(state, v, span, validate)?;
                    }
                    fresh_result = Some(FRESH);
                }
                InstructionKind::RegistrationStart { callback, .. }
                | InstructionKind::AsyncRegistrationStart { callback, .. } => {
                    self.consume(state, *callback, span, validate)?;
                    fresh_result = Some(FRESH);
                }
                InstructionKind::OwnedStoreNew { .. }
                | InstructionKind::MakeManaged { .. }
                | InstructionKind::ByteBufferNew
                | InstructionKind::TaskStart { .. } => fresh_result = Some(FRESH),
                InstructionKind::MakePackage { fields } => {
                    let output = instruction.result.expect("checked Package result");
                    budget.spend(fields.len())?;
                    for (index, &value) in fields.iter().enumerate() {
                        if matches!(f.values[value.0 as usize], Type::Managed(_)) {
                            let source = slots.managed_value[value.0 as usize]
                                .expect("checked Managed field source");
                            state.package_fresh.set(
                                output.0 as usize,
                                index,
                                state.fresh[source],
                                budget,
                            )?;
                        }
                        self.consume(state, value, span, validate)?;
                    }
                    fresh_result = Some(INPUT);
                }
                InstructionKind::MakeOwnedArray { elements: fields } => {
                    for &value in fields {
                        self.consume(state, value, span, validate)?;
                    }
                    fresh_result = Some(INPUT);
                }
                InstructionKind::BorrowPackageField { local, index, .. } => {
                    self.package_shell(state, *local, span, validate)?;
                    // A checked Copy field is initialized with its shell and
                    // has no independent cleanup responsibility to demand.
                    if let Some(field) = slots.package_fields[local.0 as usize][*index as usize]
                        && validate
                        && state.debt[field] != LIVE
                    {
                        return Err(bad(
                            span,
                            "callback field borrow requires a live field on every path",
                        ));
                    }
                    if let Some(row) = instruction
                        .result
                        .and_then(|v| slots.desc_value[v.0 as usize])
                    {
                        state.desc[row] = BTreeSet::from([Root::PackageField {
                            local: *local,
                            index: *index,
                        }]);
                    }
                }
                InstructionKind::BorrowOwnedArrayElement { local, index, .. } => {
                    self.package_shell(state, *local, span, validate)?;
                    let element = slots.package_fields[local.0 as usize][*index as usize]
                        .expect("independently checked callback array element");
                    if validate && state.debt[element] != LIVE {
                        return Err(bad(
                            span,
                            "callback array borrow requires a live element on every path",
                        ));
                    }
                }
                InstructionKind::ProjectOwnedArrayField { local, index, .. } => {
                    self.package_shell(state, *local, span, validate)?;
                    let element = slots.package_fields[local.0 as usize][*index as usize]
                        .expect("independently checked owning array element");
                    if validate && state.debt[element] != LIVE {
                        return Err(bad(
                            span,
                            "owning array field projection requires a live element on every path",
                        ));
                    }
                    if let Some(row) = instruction
                        .result
                        .and_then(|v| slots.desc_value[v.0 as usize])
                    {
                        state.desc[row] = BTreeSet::from([Root::ArrayElement {
                            local: *local,
                            index: *index,
                        }]);
                    }
                }
                InstructionKind::TakeOwnedArrayElement { local, index }
                | InstructionKind::DropOwnedArrayElement { local, index } => {
                    self.package_shell(state, *local, span, validate)?;
                    let element = slots.package_fields[local.0 as usize][*index as usize]
                        .expect("independently checked owning array element");
                    let take = matches!(
                        instruction.kind,
                        InstructionKind::TakeOwnedArrayElement { .. }
                    );
                    if validate {
                        if take && state.debt[element] != LIVE {
                            return Err(bad(
                                span,
                                "owning array take requires a complete live element on every path",
                            ));
                        }
                        no_live_description(
                            state,
                            self.after.row(b, i),
                            Root::ArrayElement {
                                local: *local,
                                index: *index,
                            },
                            span,
                            budget,
                        )?;
                    }
                    if !take && validate && state.debt[element] & LIVE != 0 {
                        let Type::Array { element: ty, .. } = &f.locals[local.0 as usize].ty else {
                            unreachable!("checked owning array")
                        };
                        if tasks::needs_synchronous_proof(self.module, ty, budget)? {
                            tasks::require_synchronous_drop(
                                self.module,
                                f,
                                *local,
                                budget,
                                Some(span),
                            )?;
                        }
                    }
                    if take {
                        state.debt[element] = MOVED;
                        fresh_result = Some(INPUT);
                    } else {
                        state.debt[element] = discharged(state.debt[element]);
                    }
                }
                InstructionKind::StoreOwnedArrayElement {
                    local,
                    index,
                    value,
                } => {
                    self.package_shell(state, *local, span, validate)?;
                    let element = slots.package_fields[local.0 as usize][*index as usize]
                        .expect("independently checked owning array element");
                    if validate {
                        if state.debt[element] & LIVE != 0 {
                            return Err(bad(
                                span,
                                "owning array refill would overwrite an outstanding element responsibility",
                            ));
                        }
                        no_live_description(
                            state,
                            self.after.row(b, i),
                            Root::ArrayElement {
                                local: *local,
                                index: *index,
                            },
                            span,
                            budget,
                        )?;
                    }
                    self.consume(state, *value, span, validate)?;
                    state.debt[element] = LIVE;
                    state.fresh[element] = INPUT;
                }
                InstructionKind::ReadPackageField { local, .. }
                | InstructionKind::DescribePackageField { local, .. } => {
                    self.package_shell(state, *local, span, validate)?;
                    if let InstructionKind::DescribePackageField { index, .. } = instruction.kind {
                        let field = slots.package_fields[local.0 as usize][index as usize]
                            .expect("typed managed package field");
                        if validate && state.debt[field] != LIVE {
                            return Err(bad(
                                span,
                                "package description requires a live field on every incoming path",
                            ));
                        }
                        let row = slots.desc_value
                            [instruction.result.expect("package description result").0 as usize]
                            .expect("package description row");
                        state.desc[row] = BTreeSet::from([Root::PackageField {
                            local: *local,
                            index,
                        }]);
                    }
                }
                InstructionKind::TakePackageField { local, index }
                | InstructionKind::DropPackageField { local, index } => {
                    self.package_shell(state, *local, span, validate)?;
                    let field = slots.package_fields[local.0 as usize][*index as usize]
                        .expect("typed managed package field");
                    let take = matches!(instruction.kind, InstructionKind::TakePackageField { .. });
                    if validate {
                        if take && state.debt[field] != LIVE {
                            return Err(bad(
                                span,
                                "package field transfer requires a live field on every incoming path",
                            ));
                        }
                        no_live_description(
                            state,
                            self.after.row(b, i),
                            Root::PackageField {
                                local: *local,
                                index: *index,
                            },
                            span,
                            budget,
                        )?;
                    }
                    if !take && validate && state.debt[field] & LIVE != 0 {
                        let fields = package_storage(&f.locals[local.0 as usize].ty)
                            .expect("checked package");
                        if tasks::needs_synchronous_proof(
                            self.module,
                            &fields[*index as usize].1,
                            budget,
                        )? {
                            tasks::require_synchronous_drop(
                                self.module,
                                f,
                                *local,
                                budget,
                                Some(span),
                            )?;
                        }
                    }
                    if take {
                        state.debt[field] = MOVED;
                        fresh_result = Some(state.fresh[field]);
                    } else {
                        state.debt[field] = discharged(state.debt[field]);
                    }
                }
                InstructionKind::StorePackageField {
                    local,
                    index,
                    value,
                } => {
                    self.package_shell(state, *local, span, validate)?;
                    if let Some(field) = slots.package_fields[local.0 as usize][*index as usize] {
                        if validate {
                            if state.debt[field] & LIVE != 0 {
                                return Err(bad(
                                    span,
                                    "package field store would overwrite an outstanding responsibility",
                                ));
                            }
                            no_live_description(
                                state,
                                self.after.row(b, i),
                                Root::PackageField {
                                    local: *local,
                                    index: *index,
                                },
                                span,
                                budget,
                            )?;
                        }
                        budget.spend(1)?;
                        let fresh = if matches!(f.values[value.0 as usize], Type::Managed(_)) {
                            let source = slots.managed_value[value.0 as usize]
                                .expect("checked Managed field source");
                            state.fresh[source]
                        } else {
                            INPUT
                        };
                        self.consume(state, *value, span, validate)?;
                        state.debt[field] = LIVE;
                        state.fresh[field] = fresh;
                    }
                }
                InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local }
                    if package_storage(&f.locals[local.0 as usize].ty).is_some() =>
                {
                    self.package_shell(state, *local, span, validate)?;
                    let fields = &slots.package_fields[local.0 as usize];
                    budget.spend(fields.len())?;
                    if validate
                        && fields
                            .iter()
                            .flatten()
                            .any(|&field| state.debt[field] != LIVE)
                    {
                        return Err(bad(
                            span,
                            if matches!(instruction.kind, InstructionKind::BorrowLocalMut { .. }) {
                                "Product borrow requires every owned child live on every path"
                            } else {
                                "shared Product borrow requires every owned child live on every path"
                            },
                        ));
                    }
                    if let Some(row) = instruction
                        .result
                        .and_then(|value| slots.desc_value[value.0 as usize])
                    {
                        budget.spend(1)?;
                        state.desc[row] = BTreeSet::from([Root::Local(*local)]);
                    }
                }
                InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local }
                    if matches!(f.locals[local.0 as usize].ty, Type::Managed(_)) =>
                {
                    let debt = slots.managed_local[local.0 as usize].expect("typed managed head");
                    if validate && state.debt[debt] != LIVE {
                        return Err(bad(span, "head address requires a live managed position"));
                    }
                }
                InstructionKind::ReadVariant {
                    value,
                    variant: 1,
                    index: 0,
                } if budget.store_option_results
                    && instruction
                        .result
                        .and_then(|id| slots.desc_value[id.0 as usize])
                        .is_some()
                    && crate::stored_flow::option_address::shape(
                        &f.values[value.0 as usize],
                        &budget.limits.clone(),
                        budget,
                    )? =>
                {
                    let id = instruction
                        .result
                        .ok_or_else(|| bad(span, "Option payload has no result"))?;
                    let checked = crate::stored_flow::option_sources_snapshot(f.id, id, budget)?;
                    if checked.is_empty() {
                        return Err(bad(span, "Option payload has no active Store window"));
                    }
                    budget.spend(checked.len())?;
                    let mut origins = BTreeSet::new();
                    for source in &checked {
                        origins.insert(option_store_origin(f, source, budget)?);
                    }
                    let dst = slots.desc_value[id.0 as usize].unwrap();
                    state.desc[dst] = origins;
                }
                InstructionKind::OwnedStoreBorrow { store, .. }
                | InstructionKind::OwnedStoreBorrowMut { store, .. }
                    if instruction
                        .result
                        .and_then(|v| slots.desc_value[v.0 as usize])
                        .is_some() =>
                {
                    let dst = slots.desc_value
                        [instruction.result.expect("Store view result").0 as usize]
                        .expect("Store view description slot");
                    let src = slots.desc_value[store.0 as usize]
                        .ok_or_else(|| bad(span, "Store view lacks an actual source descriptor"))?;
                    budget.spend(state.desc[src].len())?;
                    state.desc[dst] = state.desc[src].clone();
                }
                InstructionKind::DescribeAddress { address } => {
                    let dst = instruction
                        .result
                        .and_then(|v| slots.desc_value[v.0 as usize])
                        .expect("head descriptor slot");
                    let src = slots.desc_value[address.0 as usize].expect("head address slot");
                    budget.spend(state.desc[src].len())?;
                    state.desc[dst] = state.desc[src].clone();
                }
                InstructionKind::AddressField { address, index }
                    if instruction
                        .result
                        .and_then(|v| slots.desc_value[v.0 as usize])
                        .is_some() =>
                {
                    let dst = slots.desc_value
                        [instruction.result.expect("head projection").0 as usize]
                        .expect("head field slot");
                    let src = slots.desc_value[address.0 as usize].ok_or_else(|| {
                        bad(span, "projected head has no actual parent origin slot")
                    })?;
                    budget.spend(state.desc[src].len())?;
                    state.desc[dst] = state.desc[src]
                        .iter()
                        .map(|root| match root {
                            Root::Local(local)
                                if package_storage(&f.locals[local.0 as usize].ty).is_some() =>
                            {
                                Root::PackageField {
                                    local: *local,
                                    index: *index,
                                }
                            }
                            other => *other,
                        })
                        .collect();
                }
                InstructionKind::DescribeLocal { local } => {
                    let index =
                        slots.managed_local[local.0 as usize].expect("typed managed description");
                    if validate && state.debt[index] != LIVE {
                        return Err(bad(
                            span,
                            "only a live managed position may produce a description",
                        ));
                    }
                    let row = slots.desc_value
                        [instruction.result.expect("description result").0 as usize]
                        .expect("description row");
                    state.desc[row] = BTreeSet::from([Root::Local(*local)]);
                }
                InstructionKind::BorrowCapture { index, .. } => {
                    // Independent capture typing also permits an inline Copy
                    // field in a lending entry; such storage owns no resource.
                    if let Some(debt) = slots.capture_debt[*index as usize]
                        && validate
                        && state.debt[debt] != LIVE
                    {
                        return Err(bad(span, "borrow reads a transferred captured closure"));
                    }
                    if let Some(row) = instruction
                        .result
                        .and_then(|value| slots.desc_value[value.0 as usize])
                    {
                        budget.spend(1)?;
                        state.desc[row] =
                            BTreeSet::from([if slots.capture_debt[*index as usize].is_some() {
                                Root::OwnedCapture(*index)
                            } else {
                                Root::BorrowedCapture(*index)
                            }]);
                    }
                }
                InstructionKind::BorrowPlaceAddress { address, .. } => {
                    if let Some(dst) = instruction
                        .result
                        .and_then(|value| slots.desc_value[value.0 as usize])
                    {
                        let src = slots.desc_value[address.0 as usize].ok_or_else(|| {
                            bad(
                                span,
                                "projected resource head lacks an actual address source",
                            )
                        })?;
                        budget.spend(state.desc[src].len())?;
                        state.desc[dst] =
                            state.desc[src].iter().map(|root| root.parent()).collect();
                    }
                }
                InstructionKind::DescribeCapture { index } => {
                    let debt = slots.capture_debt[*index as usize].expect("typed owned capture");
                    if validate && state.debt[debt] != LIVE {
                        return Err(bad(
                            span,
                            "description reads a moved captured resource field",
                        ));
                    }
                    let row = slots.desc_value
                        [instruction.result.expect("description result").0 as usize]
                        .expect("description slot");
                    state.desc[row] = BTreeSet::from([Root::OwnedCapture(*index)]);
                }
                InstructionKind::ReadCapture { index } => {
                    if let Some(row) = instruction
                        .result
                        .and_then(|v| slots.desc_value[v.0 as usize])
                    {
                        state.desc[row] = BTreeSet::from([Root::BorrowedCapture(*index)]);
                    }
                }
                InstructionKind::TakeCapture { index } => {
                    let debt = slots.capture_debt[*index as usize].expect("typed owned capture");
                    if validate {
                        if state.debt[debt] != LIVE {
                            return Err(bad(
                                span,
                                "captured responsibility was already transferred",
                            ));
                        }
                        no_live_description(
                            state,
                            self.after.row(b, i),
                            Root::OwnedCapture(*index),
                            span,
                            budget,
                        )?;
                    }
                    fresh_result = Some(INPUT);
                    state.debt[debt] = MOVED;
                }
                InstructionKind::WriteCapture { index, value }
                    if self.f.environment.is_some_and(|id| {
                        matches!(
                            self.module.closures[id.0 as usize].captures[*index as usize],
                            Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }
                        )
                    }) && budget.inline_store_root(
                        &self.module.closures
                            [self.f.environment.expect("capture entry").0 as usize]
                            .captures[*index as usize],
                    )? && dynamic_store::inline_sum_capture(
                        &self.module.closures
                            [self.f.environment.expect("capture entry").0 as usize]
                            .captures[*index as usize],
                        budget,
                    )? =>
                {
                    let debt = slots.capture_debt[*index as usize]
                        .ok_or_else(|| bad(span, "owning Sum capture has no actual debt slot"))?;
                    if validate && state.debt[debt] != MOVED {
                        return Err(bad(
                            span,
                            "owning Sum capture refill requires its consumed empty slot",
                        ));
                    }
                    let source = slots.managed_value[value.0 as usize]
                        .ok_or_else(|| bad(span, "capture refill has no actual owner source"))?;
                    let fresh = state.fresh[source];
                    self.consume(state, *value, span, validate)?;
                    state.debt[debt] = LIVE;
                    state.fresh[debt] = fresh;
                    budget.spend(written.len().saturating_add(1))?;
                    written.insert(Root::OwnedCapture(*index));
                }
                InstructionKind::ByteBufferReserve { buffer: desc, .. }
                | InstructionKind::ByteBufferTruncate { buffer: desc, .. }
                | InstructionKind::ByteBufferPush { buffer: desc, .. }
                | InstructionKind::ByteBufferBorrow {
                    buffer: desc,
                    mutable: true,
                }
                | InstructionKind::WriteDesc { desc, .. } => {
                    let row = slots.desc_value[desc.0 as usize].expect("typed description");
                    self.write_roots(&state.desc[row], written, span, validate, budget)?;
                }
                InstructionKind::Load(local)
                | InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local } => {
                    if let Some(dst) = instruction
                        .result
                        .and_then(|v| slots.desc_value[v.0 as usize])
                    {
                        if let Some(src) = slots.desc_local[local.0 as usize] {
                            budget.spend(state.desc[src].len())?;
                            state.desc[dst] = state.desc[src].clone();
                        } else if let Some(debt) = slots.managed_local[local.0 as usize]
                            && matches!(
                                instruction.kind,
                                InstructionKind::BorrowLocal { .. }
                                    | InstructionKind::BorrowLocalMut { .. }
                            )
                        {
                            if validate && state.debt[debt] != LIVE {
                                return Err(bad(
                                    span,
                                    "Sum address requires a live owned position",
                                ));
                            }
                            budget.spend(1)?;
                            state.desc[dst] = BTreeSet::from([Root::Local(*local)]);
                        } else {
                            return Err(bad(span, "resource address has no actual local source"));
                        }
                    }
                }
                InstructionKind::ReborrowAddress { address, .. } => {
                    if let Some(dst) = instruction
                        .result
                        .and_then(|v| slots.desc_value[v.0 as usize])
                    {
                        let src = slots.desc_value[address.0 as usize]
                            .expect("typed environment reborrow dependency");
                        budget.spend(state.desc[src].len())?;
                        state.desc[dst] = state.desc[src].clone();
                    }
                }
                InstructionKind::TakeLocal { local } => {
                    if let Some(index) = slots.managed_local[local.0 as usize] {
                        let fields = slots.fields(*local);
                        budget.spend(fields.len().saturating_mul(if validate { 2 } else { 1 }))?;
                        if validate {
                            if state.debt[index] != LIVE {
                                return Err(bad(
                                    span,
                                    "give requires a live owned position on every incoming path",
                                ));
                            }
                            if fields
                                .iter()
                                .flatten()
                                .any(|&field| state.debt[field] != LIVE)
                            {
                                return Err(bad(
                                    span,
                                    "whole package transfer requires every managed field live on every incoming path",
                                ));
                            }
                            no_live_description(
                                state,
                                self.after.row(b, i),
                                Root::Local(*local),
                                span,
                                budget,
                            )?;
                        }
                        if let Some(types) = package_storage(&f.locals[local.0 as usize].ty) {
                            budget.spend(types.len())?;
                            let output = instruction.result.expect("checked Package move result");
                            for (position, (_, ty)) in types.iter().enumerate() {
                                if matches!(ty, Type::Managed(_)) {
                                    let field = fields[position].expect("checked Managed field");
                                    state.package_fresh.set(
                                        output.0 as usize,
                                        position,
                                        state.fresh[field],
                                        budget,
                                    )?;
                                }
                            }
                        }
                        fresh_result = Some(state.fresh[index]);
                        state.debt[index] = MOVED;
                        for &field in fields.iter().flatten() {
                            state.debt[field] = MOVED;
                        }
                        if !slots.retained(index) {
                            forget_order(state, index, budget)?;
                        }
                    }
                    if let Some(dst) = instruction
                        .result
                        .and_then(|v| slots.desc_value[v.0 as usize])
                    {
                        let src =
                            slots.desc_local[local.0 as usize].expect("typed closure description");
                        budget.spend(state.desc[src].len())?;
                        state.desc[dst] = state.desc[src].clone();
                        state.desc[src].clear();
                    }
                }
                InstructionKind::Store { local, value } => {
                    if let Some(index) = slots.managed_local[local.0 as usize] {
                        let fields = slots.fields(*local);
                        budget.spend(fields.len().saturating_mul(if validate { 2 } else { 1 }))?;
                        if validate {
                            if state.debt[index] & LIVE != 0
                                || fields
                                    .iter()
                                    .flatten()
                                    .any(|&field| state.debt[field] & LIVE != 0)
                            {
                                return Err(bad(
                                    span,
                                    "store would overwrite an outstanding resource responsibility",
                                ));
                            }
                            no_live_description(
                                state,
                                self.after.row(b, i),
                                Root::Local(*local),
                                span,
                                budget,
                            )?;
                        }
                        let source =
                            slots.managed_value[value.0 as usize].expect("typed managed store");
                        let fresh = state.fresh[source];
                        self.consume(state, *value, span, validate)?;
                        if !slots.retained(index) {
                            forget_order(state, index, budget)?;
                            for other in 0..slots.local_count {
                                budget.spend(1)?;
                                if other != index
                                    && !slots.retained(other)
                                    && state.debt[other] & LIVE != 0
                                {
                                    state.newer[other].insert(index);
                                }
                            }
                        }
                        state.debt[index] = LIVE;
                        state.fresh[index] = fresh;
                        budget.spend(fields.len())?;
                        for (position, field) in fields.iter().enumerate() {
                            if let Some(field) = field {
                                state.debt[*field] = LIVE;
                                state.fresh[*field] = if package_storage(
                                    &f.locals[local.0 as usize].ty,
                                )
                                .is_some_and(|types| matches!(types[position].1, Type::Managed(_)))
                                {
                                    state
                                        .package_fresh
                                        .value(value.0 as usize, position, budget)?
                                } else {
                                    INPUT
                                };
                            }
                        }
                    }
                    if let Some(dst) = slots.desc_local[local.0 as usize] {
                        let src =
                            slots.desc_value[value.0 as usize].expect("typed description store");
                        budget.spend(state.desc[src].len())?;
                        state.desc[dst] = state.desc[src].clone();
                    }
                }
                InstructionKind::DropManaged { local }
                | InstructionKind::DropDynOwned { local }
                | InstructionKind::DropMutexGuard { local }
                | InstructionKind::DropTuple { local }
                | InstructionKind::DropEnum { local }
                | InstructionKind::DropClosure { local }
                | InstructionKind::DropErrorUnion { local }
                | InstructionKind::DropOwnedArray { local }
                | InstructionKind::DropPackage { local } => {
                    let index = slots.managed_local[local.0 as usize].expect("typed cleanup local");
                    if validate && f.call_kind == CallKind::Direct && state.debt[index] & LIVE != 0
                    {
                        tasks::require_synchronous_drop(
                            self.module,
                            f,
                            *local,
                            budget,
                            Some(span),
                        )?;
                    }
                    if validate
                        && f.call_kind == CallKind::Direct
                        && state.debt[index] & LIVE != 0
                        && matches!(&f.locals[local.0 as usize].ty,Type::Managed(p) if matches!(p.as_ref(),Type::AsyncRegistration{..}))
                    {
                        return Err(bad(
                            span,
                            "Direct automatic Drop cannot reclaim an unknown pollable registration; explicitly advance and finish it",
                        ));
                    }
                    if validate {
                        no_live_description(
                            state,
                            self.after.row(b, i),
                            Root::Local(*local),
                            span,
                            budget,
                        )?;
                        if state.debt[index] == DONE {
                            return Err(bad(
                                span,
                                "duplicate automatic cleanup of an already discharged position",
                            ));
                        }
                        budget.spend(state.newer[index].len())?;
                        if !slots.retained(index)
                            && state.newer[index]
                                .iter()
                                .any(|&newer| state.debt[newer] & LIVE != 0)
                        {
                            return Err(bad(
                                span,
                                format!(
                                    "automatic cleanup is not in reverse successful initialization order (local {})",
                                    local.0
                                ),
                            ));
                        }
                    }
                    state.debt[index] = discharged(state.debt[index]);
                    let fields = slots.fields(*local);
                    budget.spend(fields.len())?;
                    for &field in fields.iter().flatten() {
                        state.debt[field] = discharged(state.debt[field]);
                    }
                    if !slots.retained(index) {
                        forget_order(state, index, budget)?;
                    }
                }
                InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local) => {
                    if let Some(index) = slots.managed_local[local.0 as usize] {
                        let fields = slots.fields(*local);
                        budget.spend(fields.len().saturating_mul(if validate { 2 } else { 1 }))?;
                        if validate {
                            if state.debt[index] & LIVE != 0
                                || fields
                                    .iter()
                                    .flatten()
                                    .any(|&field| state.debt[field] & LIVE != 0)
                            {
                                return Err(bad(
                                    span,
                                    "storage ends or restarts with an outstanding resource responsibility",
                                ));
                            }
                            no_live_description(
                                state,
                                self.after.row(b, i),
                                Root::Local(*local),
                                span,
                                budget,
                            )?;
                        }
                        state.debt[index] = EMPTY;
                        state.fresh[index] = 0;
                        for &field in fields.iter().flatten() {
                            state.debt[field] = EMPTY;
                            state.fresh[field] = 0;
                        }
                        forget_order(state, index, budget)?;
                    }
                    if let Some(index) = slots.desc_local[local.0 as usize] {
                        state.desc[index].clear();
                    }
                }
                InstructionKind::MakeClosure { captures, .. } => {
                    if let Some(row) = instruction
                        .result
                        .and_then(|v| slots.desc_value[v.0 as usize])
                    {
                        let mut roots = BTreeSet::new();
                        for value in captures {
                            budget.spend(1)?;
                            if let Some(source) = slots.desc_value[value.0 as usize] {
                                budget
                                    .spend(state.desc[source].len().saturating_add(roots.len()))?;
                                roots.extend(&state.desc[source]);
                            }
                        }
                        state.desc[row] = roots;
                    }
                    for &value in captures {
                        self.consume(state, value, span, validate)?;
                    }
                    if result.is_some() {
                        fresh_result = Some(INPUT);
                    }
                }
                InstructionKind::CallClosureShared { callee, arguments }
                | InstructionKind::CallClosureMut { callee, arguments } => {
                    if validate
                        && let Some(debt) = slots.managed_local[callee.0 as usize]
                        && state.debt[debt] != LIVE
                    {
                        return Err(bad(
                            span,
                            "closure environment is not live on every incoming path",
                        ));
                    }
                    if let Some(row) = slots.desc_local[callee.0 as usize] {
                        if validate {
                            description_live(state, slots, row, span, budget)?;
                        }
                        let Type::Closure(id) = f.locals[callee.0 as usize].ty else {
                            unreachable!()
                        };
                        let entry = self.module.closures[id.0 as usize].function.0 as usize;
                        budget.spend(self.effects[entry].len())?;
                        if self.effects[entry]
                            .iter()
                            .any(|r| matches!(r.parent(), Root::BorrowedCapture(_)))
                        {
                            self.write_roots(&state.desc[row], written, span, validate, budget)?;
                        }
                    }
                    for &value in arguments {
                        self.consume(state, value, span, validate)?;
                    }
                    if result.is_some() {
                        fresh_result = Some(INPUT);
                    }
                }
                InstructionKind::WithForeignCallback { .. }
                | InstructionKind::WithScopedThread { .. }
                | InstructionKind::WithSyncCell { .. } => {
                    // Even registration failure consumes and cleans the prepared body.
                    let (callbacks, body) = match &instruction.kind {
                        InstructionKind::WithForeignCallback { callback, body, .. } => {
                            ([Some(callback), Some(body)], body)
                        }
                        InstructionKind::WithScopedThread { body, .. }
                        | InstructionKind::WithSyncCell { body, .. } => ([None, Some(body)], body),
                        _ => unreachable!(),
                    };
                    for callee in callbacks.into_iter().flatten() {
                        if let Some(row) = slots.desc_value[callee.0 as usize] {
                            let ty = &f.values[callee.0 as usize];
                            let entry = effect_closure_target(self.module, ty, span)?;
                            budget.spend(self.effects[entry].len())?;
                            if self.effects[entry]
                                .iter()
                                .any(|root| matches!(root.parent(), Root::BorrowedCapture(_)))
                            {
                                self.write_roots(
                                    &state.desc[row],
                                    written,
                                    span,
                                    validate,
                                    budget,
                                )?;
                            }
                        }
                    }
                    self.consume(state, *body, span, validate)?;
                }
                InstructionKind::CallClosureBorrowed {
                    callee, arguments, ..
                } => {
                    if let Some(row) = slots.desc_value[callee.0 as usize] {
                        let Type::Address { inner, .. } = &f.values[callee.0 as usize] else {
                            unreachable!()
                        };
                        let Type::Closure(id) = inner.as_ref() else {
                            unreachable!()
                        };
                        let entry = self.module.closures[id.0 as usize].function.0 as usize;
                        budget.spend(self.effects[entry].len())?;
                        if self.effects[entry]
                            .iter()
                            .any(|r| matches!(r.parent(), Root::BorrowedCapture(_)))
                        {
                            self.write_roots(&state.desc[row], written, span, validate, budget)?;
                        }
                    }
                    for &value in arguments {
                        self.consume(state, value, span, validate)?;
                    }
                    if result.is_some() {
                        fresh_result = Some(INPUT);
                    }
                }
                InstructionKind::CallClosure { callee, arguments }
                | InstructionKind::CallClosureOnce { callee, arguments } => {
                    if let Some(row) = slots.desc_value[callee.0 as usize] {
                        let Type::Closure(id) = f.values[callee.0 as usize] else {
                            unreachable!()
                        };
                        let entry = self.module.closures[id.0 as usize].function.0 as usize;
                        budget.spend(self.effects[entry].len())?;
                        if self.effects[entry]
                            .iter()
                            .any(|r| matches!(r.parent(), Root::BorrowedCapture(_)))
                        {
                            self.write_roots(&state.desc[row], written, span, validate, budget)?;
                        }
                    }
                    if matches!(instruction.kind, InstructionKind::CallClosureOnce { .. }) {
                        if validate {
                            tasks::require_synchronous_value(
                                self.module,
                                f,
                                *callee,
                                budget,
                                Some(span),
                            )?;
                        }
                        self.consume(state, *callee, span, validate)?;
                    }
                    for &value in arguments {
                        self.consume(state, value, span, validate)?;
                    }
                    if result.is_some() {
                        // Ordinary callback results carry a fresh responsibility,
                        // never a callee-private Created origin.
                        fresh_result = Some(INPUT);
                    }
                }
                InstructionKind::AsyncRegistrationFinishDestroy { registration: task }
                | InstructionKind::AsyncRegistrationFinishDestroyChecked { registration: task }
                | InstructionKind::RegistrationClose { registration: task }
                | InstructionKind::TaskFinishDestroy { task } => {
                    self.consume(state, *task, span, validate)?;
                }
                InstructionKind::Call {
                    function,
                    arguments,
                } => {
                    let returned = &self.module.functions[function.0 as usize];
                    if qualified_package_address(&returned.return_type) {
                        let value = instruction
                            .result
                            .ok_or_else(|| bad(span, "Package return has no result"))?;
                        let position =
                            crate::stored_flow::package_return_input(f.id, value, budget)?;
                        let actual = *arguments
                            .get(position as usize)
                            .ok_or_else(|| bad(span, "Package return input is absent"))?;
                        let source = slots.desc_value[actual.0 as usize].ok_or_else(|| {
                            bad(span, "Package return input lacks actual description")
                        })?;
                        let destination = slots.desc_value[value.0 as usize].ok_or_else(|| {
                            bad(span, "Package return lacks actual description slot")
                        })?;
                        if validate {
                            description_live(state, slots, source, span, budget)?;
                        }
                        budget.spend(state.desc[source].len())?;
                        state.desc[destination] = state.desc[source].clone();
                    }
                    if returned.return_type.is_shared_package_address()
                        && let Some(bound) = &returned.reference_return_bound
                        && let Some(destination) = instruction
                            .result
                            .and_then(|id| slots.desc_value[id.0 as usize])
                    {
                        let mut roots = BTreeSet::new();
                        budget.spend(bound.len())?;
                        for position in bound {
                            let actual = *arguments.get(*position as usize).ok_or_else(|| {
                                bad(span, "shared Package return input is absent")
                            })?;
                            let source = slots.desc_value[actual.0 as usize].ok_or_else(|| {
                                bad(span, "shared Package return input has no manager source")
                            })?;
                            budget.spend(state.desc[source].len())?;
                            roots.extend(state.desc[source].iter().cloned());
                        }
                        if roots.is_empty() {
                            return Err(bad(
                                span,
                                "shared Package return has no actual manager source",
                            ));
                        }
                        state.desc[destination] = roots;
                    }
                    if instruction.result.is_some_and(|id| {
                        matches!(f.values[id.0 as usize], Type::MutexGuard { .. })
                    }) {
                        let dst = instruction
                            .result
                            .and_then(|id| slots.desc_value[id.0 as usize])
                            .ok_or_else(|| {
                                bad(span, "guard helper result lacks retained head slot")
                            })?;
                        let mut roots = BTreeSet::new();
                        let selectors = budget
                            .guard_return_sources
                            .get(function.0 as usize)
                            .ok_or_else(|| {
                                bad(span, "guard helper has no checked return source summary")
                            })?
                            .clone();
                        budget.spend(selectors.len())?;
                        for position in &selectors {
                            let value = *arguments.get(*position as usize).ok_or_else(|| {
                                bad(span, "guard helper selected input is absent")
                            })?;
                            if !matches!(&f.values[value.0 as usize], Type::Desc(inner) if matches!(inner.as_ref(), Type::Mutex { .. }))
                                && !matches!(&f.values[value.0 as usize], Type::MutexGuard { .. })
                            {
                                return Err(bad(
                                    span,
                                    "guard helper selected input is not an actual mutex carrier",
                                ));
                            }
                            let src = slots.desc_value[value.0 as usize].ok_or_else(|| {
                                bad(span, "guard helper selected input lacks actual head slot")
                            })?;
                            budget.spend(state.desc[src].len())?;
                            roots.extend(state.desc[src].iter().cloned());
                        }
                        if selectors.is_empty() || roots.is_empty() {
                            return Err(bad(
                                span,
                                "guard helper result has no selected actual mutex head input",
                            ));
                        }
                        state.desc[dst] = roots;
                    }
                    let callable = self.module.functions[function.0 as usize]
                        .callable_contract
                        .as_ref();
                    if let Some(contract) = callable {
                        // helpers::check_module already checked this relation.
                        // Callback consumers use its public write upper bound,
                        // never the helper's private body or inferred effects.
                        budget.spend(contract.roles.len())?;
                        for role in &contract.roles {
                            let invokes = match &contract.traces {
                                CallableTraces::Finite(paths) => {
                                    budget.spend(paths.len())?;
                                    let mut invokes = false;
                                    for path in paths {
                                        budget.spend(path.calls.len())?;
                                        invokes |= path
                                            .calls
                                            .iter()
                                            .any(|event| event.parameter == role.parameter);
                                    }
                                    invokes
                                }
                                CallableTraces::Flow(flow) => {
                                    // Every reachable prefix includes nonreturning paths.
                                    budget.spend(flow.nodes.len())?;
                                    let mut invokes = false;
                                    for node in &flow.nodes {
                                        budget.spend(node.calls.len())?;
                                        invokes |= node
                                            .calls
                                            .iter()
                                            .any(|event| event.parameter == role.parameter);
                                    }
                                    invokes
                                }
                            };
                            budget.spend(role.writes.len())?;
                            let definition = &self.module.closures[role.closure.0 as usize];
                            if invokes
                                && role.writes.iter().any(|i| {
                                    matches!(definition.captures[*i as usize], Type::Desc(_))
                                })
                            {
                                let argument = arguments[role.parameter as usize];
                                if let Some(row) = slots.desc_value[argument.0 as usize] {
                                    self.write_roots(
                                        &state.desc[row],
                                        written,
                                        span,
                                        validate,
                                        budget,
                                    )?;
                                }
                            }
                        }
                    } else {
                        // Helper extras are Copy or complete owned values;
                        // only legacy ordinary calls need private Desc effects.
                        // Do not inspect or charge the helper's private rows.
                        budget.spend(self.effects[function.0 as usize].len())?;
                        for root in &self.effects[function.0 as usize] {
                            if let Root::Parameter(parameter) = root {
                                let value = arguments[*parameter as usize];
                                if let Some(row) = slots.desc_value[value.0 as usize] {
                                    self.write_roots(
                                        &state.desc[row],
                                        written,
                                        span,
                                        validate,
                                        budget,
                                    )?;
                                }
                            }
                        }
                    }
                    for &value in arguments {
                        self.consume(state, value, span, validate)?;
                    }
                    if result.is_some() {
                        fresh_result = Some(
                            if self.module.functions[function.0 as usize].result_mode
                                == PassingMode::Create
                            {
                                FRESH
                            } else {
                                INPUT
                            },
                        );
                    }
                }
                _ => {}
            }
            if (budget.head_descriptions || self.replacements.is_some())
                && let InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local } = instruction.kind
                && (package_storage(&f.locals[local.0 as usize].ty).is_some()
                    || matches!(f.locals[local.0 as usize].ty, Type::Managed(_)))
                && let Some(row) = instruction
                    .result
                    .and_then(|v| slots.desc_value[v.0 as usize])
            {
                state.desc[row] = BTreeSet::from([Root::Local(local)]);
            }
            if let Some(index) = result {
                let fresh = fresh_result.ok_or_else(|| {
                    bad(span, "instruction cannot manufacture a managed SSA carrier")
                })?;
                state.debt[index] = LIVE;
                state.fresh[index] = fresh;
            }
            if self.replacements.is_some()
                && let Some(value) = instruction.result
                && let Some(row) = slots.desc_value[value.0 as usize]
            {
                if let Type::Desc(inner) = &f.values[value.0 as usize] {
                    budget.spend(state.desc[row].len())?;
                    let parent = matches!(inner.as_ref(), Type::OwnedStore { .. });
                    state.desc[row] = state.desc[row]
                        .iter()
                        .map(|root| {
                            if parent {
                                root.store_parent()
                            } else {
                                root.parent()
                            }
                        })
                        .collect();
                } else if matches!(
                    instruction.kind,
                    InstructionKind::OwnedStoreBorrow { .. }
                        | InstructionKind::OwnedStoreBorrowMut { .. }
                ) {
                    budget.spend(state.desc[row].len())?;
                    state.desc[row] = state.desc[row].iter().map(|root| root.parent()).collect();
                }
            }
            if !state.product_fresh.is_empty() {
                slots.product.apply(f, slots, instruction, state, budget)?;
            }
            if let Some(rows) = self.sum_rows {
                let bridge = rows.get(b).and_then(|row| row.get(i)).ok_or_else(|| {
                    reject(
                        "E-VERIFY-SUM",
                        Some(instruction.span),
                        "reachable instruction lacks actual Sum content bridge",
                    )
                })?;
                if let Some(output) = instruction.result {
                    if let Some(fresh) = bridge.result
                        && let Some(debt) = slots.managed_value[output.0 as usize]
                    {
                        state.fresh[debt] = fresh;
                    }
                    budget.spend(bridge.package.len())?;
                    for &(index, fresh) in &bridge.package {
                        state
                            .package_fresh
                            .set(output.0 as usize, index, fresh, budget)?;
                    }
                }
                budget.spend(bridge.unpack.len())?;
                for &(local, fresh, ref package) in &bridge.unpack {
                    if let Some(debt) = slots.managed_local[local.0 as usize] {
                        state.fresh[debt] = fresh;
                    }
                    budget.spend(package.len())?;
                    for &(index, fresh) in package {
                        if let Some(debt) = slots.fields(local)[index] {
                            state.fresh[debt] = fresh;
                        }
                    }
                }
            }
            if let Some(plan) = self.replacements {
                plan.source_step(self, state, instruction, budget)?;
            }
            if validate {
                task_frames::finish(f, b, i, task_before, state, budget)?;
            }
        }
        if let Terminator::Return(value) = f.blocks[b].terminator {
            if let Some(value) = value {
                if validate
                    && let Some(index) = slots.managed_value[value.0 as usize]
                    && f.result_mode == PassingMode::Create
                    && state.fresh[index] != FRESH
                    && !(errors::managed_success(&f.return_type) && state.fresh[index] == 0)
                {
                    return Err(bad(
                        f.span,
                        "create result is not independently proven fresh",
                    ));
                }
                self.consume(state, value, f.span, validate)?;
            }
            if validate {
                // This pre-existing ordinary-return scan formerly lacked its
                // own charge. Count every actually inspected responsibility,
                // including non-Package locals/SSA; do not hide old work to
                // claim that adding field accounting is free.
                budget.spend(slots.capture_start)?;
            }
            if validate
                && state.debt[..slots.capture_start]
                    .iter()
                    .enumerate()
                    .any(|(index, s)| !slots.retained(index) && s & LIVE != 0)
            {
                return Err(bad(
                    f.span,
                    "normal return leaves an outstanding resource responsibility",
                ));
            }
            // Shared/exclusive entries return the intact owned environment.
            // A Consume caller performs fixed-order cleanup of remaining live
            // capture flags after this entry returns; moved fields stay cleared.
            if validate
                && f.environment.is_some_and(|id| {
                    self.module.closures[id.0 as usize].kind != CallableKind::FnOnce
                })
                && {
                    budget.spend(
                        slots
                            .capture_debt
                            .len()
                            .saturating_add(slots.retained_debt.len()),
                    )?;
                    slots
                        .capture_debt
                        .iter()
                        .flatten()
                        .any(|&d| state.debt[d] != LIVE)
                        || slots
                            .retained_debt
                            .iter()
                            .enumerate()
                            .any(|(d, retained)| *retained && state.debt[d] != LIVE)
                }
            {
                return Err(bad(
                    f.span,
                    "repeatable closure entry does not preserve every owned field",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
fn run_function(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    effects: &[BTreeSet<Root>],
    validate: bool,
    budget: &mut Budget,
) -> Check<BTreeSet<Root>> {
    run_function_with_sums(module, f, graph, effects, validate, None, budget)
}

#[cfg(test)]
fn run_function_with_sums(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    effects: &[BTreeSet<Root>],
    validate: bool,
    sum_rows: Option<&[Vec<sums::Bridge>]>,
    budget: &mut Budget,
) -> Check<BTreeSet<Root>> {
    run_function_data(
        module,
        &mut desc_liveness::Binding::new(f, graph),
        effects,
        validate,
        sum_rows,
        budget,
        None,
    )
}
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn run_function_data(
    module: &TypedModule,
    binding: &mut desc_liveness::Binding<'_>,
    effects: &[BTreeSet<Root>],
    validate: bool,
    sum_rows: Option<&[Vec<sums::Bridge>]>,
    budget: &mut Budget,
    replacements: Option<&replacements::Plan>,
) -> Check<BTreeSet<Root>> {
    let f = binding.function();
    let graph = binding.graph();
    if f.foreign.is_some() {
        return Ok(BTreeSet::new());
    }
    let mut slots = Slots::new(module, f, &graph.package_views, budget)?;
    if let Some(plan) = replacements {
        plan.extend_slots(f, &mut slots, budget)?;
    }
    run_with_slots(
        module,
        binding,
        &slots,
        effects,
        validate,
        sum_rows,
        budget,
        replacements,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_with_slots(
    module: &TypedModule,
    binding: &mut desc_liveness::Binding<'_>,
    slots: &Slots,
    effects: &[BTreeSet<Root>],
    validate: bool,
    sum_rows: Option<&[Vec<sums::Bridge>]>,
    budget: &mut Budget,
    replacements: Option<&replacements::Plan>,
) -> Check<BTreeSet<Root>> {
    let f = binding.function();
    let graph = binding.graph();
    if slots.debt_count == 0 && slots.desc_count == 0 && !slots.retirement {
        return Ok(BTreeSet::new());
    }
    let mut written = if let Some(plan) = replacements {
        plan.writes(f.id, budget)?
    } else {
        BTreeSet::new()
    };
    let after = binding.after(slots, budget)?;
    let mut initial = slots.empty(budget)?;
    // Charge actual initialization work, including the old Managed-only scan.
    budget.spend(slots.capture_debt.len())?;
    for (index, debt) in slots.capture_debt.iter().enumerate() {
        if let Some(debt) = debt {
            initial.debt[*debt] = LIVE;
            initial.fresh[*debt] = INPUT;
        }
        if let Some(local) = slots.package_capture_local.get(index).copied().flatten() {
            budget.spend(slots.fields(local).len())?;
            for &field in slots.fields(local).iter().flatten() {
                initial.debt[field] = LIVE;
                initial.fresh[field] = INPUT;
            }
        }
    }
    for (p, local) in f.parameters.iter().enumerate() {
        if let Some(index) = slots.managed_local[local.0 as usize] {
            for old in 0..slots.local_count {
                budget.spend(1)?;
                if !slots.retained(old) && initial.debt[old] & LIVE != 0 {
                    initial.newer[old].insert(index);
                }
            }
            initial.debt[index] = LIVE;
            initial.fresh[index] = INPUT;
            let fields = slots.fields(*local);
            budget.spend(fields.len())?;
            for &field in fields.iter().flatten() {
                initial.debt[field] = LIVE;
                initial.fresh[field] = INPUT;
            }
        }
        if let Some(index) = slots.desc_local[local.0 as usize] {
            let root = Root::Parameter(p as u32);
            initial.desc[index].insert(if matches!(&f.locals[local.0 as usize].ty, Type::Desc(inner) if matches!(inner.as_ref(), Type::OwnedStore { .. })) { root.store_parent() } else { root });
        }
    }
    if let Some(plan) = replacements {
        plan.seed_sources(module, f, slots, &mut initial, budget)?;
    }
    if validate {
        task_frames::start(f, slots, &initial, budget)?;
    }
    budget.spend(f.blocks.len().saturating_mul(2))?;
    let mut inputs = vec![None; f.blocks.len()];
    inputs[f.entry.0 as usize] = Some(initial);
    let mut pending = schedule::Worklist::new(
        graph,
        f.entry.0 as usize,
        budget.pod_stores || (budget.store_option_results && budget.ordinary_error_places),
        budget,
    )?;
    let mut queued = vec![false; f.blocks.len()];
    queued[f.entry.0 as usize] = true;
    let transfer = Transfer {
        module,
        f,
        slots,
        after: after.history(),
        effects,
        sum_rows,
        replacements,
    };
    while let Some(b) = pending.pop(budget)? {
        queued[b] = false;
        let mut output = clone_state(inputs[b].as_ref().expect("queued resource state"), budget)?;
        transfer.block(b, &mut output, false, &mut written, budget)?;
        for &next in &graph.successors[b] {
            budget.spend(1)?;
            let changed = if let Some(input) = &mut inputs[next] {
                merge(input, &output, budget)?
            } else {
                inputs[next] = Some(clone_state(&output, budget)?);
                true
            };
            if changed && !queued[next] {
                queued[next] = true;
                pending.push(next, budget)?;
            }
        }
    }
    #[cfg(test)]
    TEST_RESOURCE_INPUTS.with(|capture| {
        let mut capture = capture.borrow_mut();
        if capture.is_some() {
            *capture = Some(inputs.clone());
        }
    });
    for (b, input) in inputs.into_iter().enumerate() {
        if let Some(mut state) = input {
            transfer.block(b, &mut state, validate, &mut written, budget)?;
        }
    }
    Ok(written)
}

fn effect_dependents(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    budget: &mut Budget,
) -> Check<Vec<Vec<usize>>> {
    let count = module.functions.len();
    budget.spend(
        count
            .checked_mul(5)
            .ok_or_else(|| limit("resource effect frontier size overflow"))?,
    )?;
    let mut dependents = vec![Vec::new(); count];
    for (user, (function, graph)) in module.functions.iter().zip(graphs).enumerate() {
        for (block, live) in function.blocks.iter().zip(&graph.reachable) {
            budget.spend(1)?;
            if !live {
                continue;
            }
            for instruction in &block.instructions {
                budget.spend(1)?;
                if let InstructionKind::WithForeignCallback { callback, body, .. } =
                    instruction.kind
                {
                    for value in [callback, body] {
                        let ty = function.values.get(value.0 as usize).ok_or_else(|| {
                            bad(instruction.span, "scoped callback operand is absent")
                        })?;
                        let dependency = effect_closure_target(module, ty, instruction.span)?;
                        let rows = dependents.get_mut(dependency).ok_or_else(|| {
                            bad(instruction.span, "scoped callback closure entry is absent")
                        })?;
                        if rows.last() != Some(&user) {
                            budget.spend(2)?;
                            rows.push(user);
                        }
                    }
                    continue;
                }
                let dependency = match &instruction.kind {
                    InstructionKind::Call { function, .. } => {
                        let callee =
                            module.functions.get(function.0 as usize).ok_or_else(|| {
                                bad(instruction.span, "resource effect call target is absent")
                            })?;
                        // A checked public callback envelope is the dependency
                        // boundary. No private row is consumed for such calls.
                        callee
                            .callable_contract
                            .is_none()
                            .then_some(function.0 as usize)
                    }
                    InstructionKind::CallClosureShared { callee, .. }
                    | InstructionKind::CallClosureMut { callee, .. } => {
                        let local = function.locals.get(callee.0 as usize).ok_or_else(|| {
                            bad(instruction.span, "resource effect closure local is absent")
                        })?;
                        Some(effect_closure_target(module, &local.ty, instruction.span)?)
                    }
                    InstructionKind::CallClosureBorrowed { callee, .. }
                    | InstructionKind::CallClosureOnce { callee, .. }
                    | InstructionKind::WithScopedThread { body: callee, .. }
                    | InstructionKind::WithSyncCell { body: callee, .. }
                    | InstructionKind::CallClosure { callee, .. } => {
                        let ty = function.values.get(callee.0 as usize).ok_or_else(|| {
                            bad(instruction.span, "resource effect closure value is absent")
                        })?;
                        Some(effect_closure_target(module, ty, instruction.span)?)
                    }
                    _ => None,
                };
                if let Some(dependency) = dependency {
                    let rows = dependents.get_mut(dependency).ok_or_else(|| {
                        bad(instruction.span, "resource effect closure entry is absent")
                    })?;
                    if rows.last() != Some(&user) {
                        budget.spend(2)?;
                        rows.push(user);
                    }
                }
            }
        }
    }
    Ok(dependents)
}

pub(super) fn effect_closure_target(module: &TypedModule, ty: &Type, span: Span) -> Check<usize> {
    let id = match ty {
        Type::Closure(id) => *id,
        Type::Address { inner, .. } => match inner.as_ref() {
            Type::Closure(id) => *id,
            _ => return Err(bad(span, "resource effect address has no closure entry")),
        },
        _ => return Err(bad(span, "resource effect call has no closure entry")),
    };
    let entry = module
        .closures
        .get(id.0 as usize)
        .ok_or_else(|| bad(span, "resource effect closure definition is absent"))?
        .function;
    Ok(entry.0 as usize)
}

#[cfg(test)]
fn infer_effects(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    budget: &mut Budget,
) -> Check<Vec<BTreeSet<Root>>> {
    slot_layout::Request::new(module, graphs, None, None, budget)?.infer_effects()
}

#[cfg(test)]
fn infer_effects_full_scan(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    budget: &mut Budget,
) -> Check<Vec<BTreeSet<Root>>> {
    budget.spend(module.functions.len())?;
    let mut effects = vec![BTreeSet::new(); module.functions.len()];
    loop {
        let mut changed = false;
        for (index, (function, graph)) in module.functions.iter().zip(graphs).enumerate() {
            budget.spend(1)?;
            TEST_EFFECT_SOLVES.with(|value| value.set(value.get() + 1));
            let found = run_function(module, function, graph, &effects, false, budget)?;
            budget.spend(found.len().saturating_add(effects[index].len()))?;
            let before = effects[index].len();
            effects[index].extend(found);
            changed |= effects[index].len() != before;
        }
        if !changed {
            return Ok(effects);
        }
    }
}

#[cfg(test)]
thread_local! {
    static TEST_EFFECT_SOLVES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static TEST_RESOURCE_INPUTS: std::cell::RefCell<Option<Vec<Option<State>>>> = const { std::cell::RefCell::new(None) };
}

// These graphs are rebuilt only from this kernel's final actual-content proof.
// The original graph remains the authority for structural/type checking.
fn resource_graphs(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    exclusions: &BTreeMap<usize, BTreeSet<(usize, usize)>>,
    budget: &mut Budget,
) -> Check<BTreeMap<usize, FunctionGraph>> {
    let mut refined = BTreeMap::new();
    budget.spend(exclusions.len())?;
    for (&index, excluded) in exclusions {
        let original = &graphs[index];
        let n = original.successors.len();
        budget.spend(n.saturating_add(1))?;
        let edges = original
            .successors
            .iter()
            .fold(0usize, |n, row| n.saturating_add(row.len()));
        // Row headers, copied edges, reachability/stack slots and retained
        // entry-view metadata are paid before allocation or traversal.
        budget.spend(
            n.saturating_mul(5)
                .saturating_add(edges.saturating_mul(3))
                .saturating_add(original.package_views.by_local.len())
                .saturating_add(original.package_views.by_capture.len())
                .saturating_add(original.role_data_uses.len())
                .saturating_add(1),
        )?;
        let mut successors = vec![Vec::new(); n];
        let mut predecessors = vec![Vec::new(); n];
        for (from, row) in original.successors.iter().enumerate() {
            for &to in row {
                budget.spend(excluded.len().saturating_add(1))?;
                if !excluded.contains(&(from, to)) {
                    successors[from].push(to);
                    predecessors[to].push(from);
                }
            }
        }
        let mut reachable = vec![false; n];
        let mut pending = Vec::with_capacity(n.saturating_add(edges));
        pending.push(module.functions[index].entry.0 as usize);
        while let Some(block) = pending.pop() {
            budget.spend(1)?;
            if !reachable[block] {
                reachable[block] = true;
                budget.spend(successors[block].len())?;
                pending.extend(&successors[block]);
            }
        }
        budget.spend(refined.len().saturating_add(1))?;
        refined.insert(
            index,
            FunctionGraph {
                successors,
                predecessors,
                reachable,
                package_views: packages::CaptureViews {
                    by_local: original.package_views.by_local.clone(),
                    by_capture: original.package_views.by_capture.clone(),
                },
                role_data_uses: original.role_data_uses.clone(),
            },
        );
    }
    Ok(refined)
}

pub(super) fn check_module(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    budget: &mut Budget,
) -> Check<()> {
    let sum_proofs = sums::check_module(module, graphs, budget)?;
    budget.spend(module.closures.len())?;
    if module.resources.is_empty() && !budget.sums {
        let mut owning_entry = false;
        for closure in &module.closures {
            budget.spend(closure.captures.len())?;
            owning_entry |= closure.kind == CallableKind::FnOnce
                || closure.captures.iter().any(|ty| {
                    matches!(
                        ty,
                        Type::Managed(_)
                            | Type::Package { .. }
                            | Type::Closure(_)
                            | Type::OwnedDyn { .. }
                    )
                });
        }
        for function in &module.functions {
            budget.spend(function.locals.len().saturating_add(function.values.len()))?;
            owning_entry |= function
                .locals
                .iter()
                .map(|l| &l.ty)
                .chain(&function.values)
                .any(|ty| arrays::is_owned_type(ty) || package_storage(ty).is_some() || matches!(ty, Type::OwnedDyn { .. }) || matches!(ty, Type::ErrorUnion { result, .. } if matches!(result.as_ref(), Type::OwnedDyn { .. })));
        }
        if !owning_entry {
            return Ok(());
        }
    }
    // Effects are inferred from executable writes and actual description origins,
    // including forwarding and closures. An empty candidate is never authority.
    let replacements = if budget.replacements {
        Some(replacements::Plan::new(
            module,
            graphs,
            &sum_proofs.replacement_writes,
            &sum_proofs.replacement_targets,
            &sum_proofs.retirement_targets,
            budget,
        )?)
    } else {
        None
    };
    let refined = if sum_proofs.excluded_edges.is_empty() {
        None
    } else {
        Some(resource_graphs(
            module,
            graphs,
            &sum_proofs.excluded_edges,
            budget,
        )?)
    };
    let mut request = slot_layout::Request::new(
        module,
        graphs,
        refined.as_ref(),
        replacements.as_ref(),
        budget,
    )?;
    let effects = request.infer_effects()?;
    for (index, function) in module.functions.iter().enumerate() {
        request.run(
            index,
            &effects,
            true,
            sum_proofs
                .rows
                .get(function.id.0 as usize)
                .filter(|rows| !rows.is_empty())
                .map(Vec::as_slice),
        )?;
    }
    request.close_pairs()?;
    Ok(())
}

#[cfg(test)]
mod frontier_tests;

/// A test-only consumer probe starts with a fully checked module. It removes
/// private callee rows only for the separate caller transfer, never for Verify.
#[cfg(test)]
pub(super) fn assert_public_helper_effect_independence(
    verified: &VerifiedModule,
    caller: FunctionId,
    helper: FunctionId,
    writes_parameter: bool,
) {
    let module = verified.module();
    let f = &module.functions[caller.0 as usize];
    let limits = Limits::default();
    let graph = check_function(
        module,
        f,
        &verified.plan().functions[caller.0 as usize],
        &mut Budget::new(&limits),
        &limits,
    )
    .unwrap();
    let empty_rows = vec![BTreeSet::new(); module.functions.len()];
    let mut forged_rows = empty_rows.clone();
    forged_rows[helper.0 as usize] = BTreeSet::from([
        Root::Parameter(0),
        Root::Parameter(u32::MAX),
        Root::BorrowedCapture(u32::MAX),
    ]);
    let expected = if writes_parameter {
        BTreeSet::from([Root::Parameter(0)])
    } else {
        BTreeSet::new()
    };
    let mut cost = None;
    for private_rows in [Vec::new(), empty_rows, forged_rows] {
        let mut budget = Budget::new(&limits);
        let result = run_function(module, f, &graph, &private_rows, true, &mut budget).unwrap();
        assert_eq!(result, expected);
        if let Some(previous) = cost.replace(budget.remaining) {
            assert_eq!(
                previous, budget.remaining,
                "caller read private helper rows"
            );
        }
    }
}
