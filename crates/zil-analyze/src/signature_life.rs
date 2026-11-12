//! Semantic class equations are not target origins or access paths.
use super::*;

// The signature relation assigns no loan to a value-only Array. The module
// preflight separately validates every qualified child and its exact instance.
fn pure_array_value(ty: &Type) -> bool {
    if !matches!(ty, Type::Array { .. }) {
        return false;
    }
    pure_value_tree(ty)
}

fn pure_nominal_value(ty: &Type) -> bool {
    matches!(ty, Type::BorrowedStorage { life, .. } if life.slots.is_empty()) && pure_value_tree(ty)
}

fn pure_value_tree(ty: &Type) -> bool {
    let mut pending = vec![ty];
    while let Some(current) = pending.pop() {
        match current {
            Type::BorrowedStorage { storage, life } if life.slots.is_empty() => {
                pending.push(storage)
            }
            Type::Array { element, .. } => pending.push(element),
            Type::Tuple(fields) => pending.extend(fields),
            Type::Record { fields, .. } => pending.extend(fields.iter().map(|(_, t)| t)),
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    pending.extend(fields);
                }
            }
            Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I8
            | Type::I16
            | Type::Isize
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::F32
            | Type::F64 => {}
            _ => return false,
        }
    }
    true
}

fn array_classes(
    ty: &Type,
    path: &mut Vec<LifeTypeProjection>,
    out: &mut Vec<(Vec<LifeTypeProjection>, u32)>,
) {
    match ty {
        Type::BorrowedStorage { life, .. } => {
            for class in 0..life.binder_count {
                out.push((path.clone(), class));
            }
        }
        Type::ErrorUnion { error, result } => {
            for (arm, child) in [(true, error.as_ref()), (false, result.as_ref())] {
                path.push(LifeTypeProjection::Place(PlaceProjection::ErrorArm(arm)));
                array_classes(child, path, out);
                path.pop();
            }
        }
        Type::Array { element, .. } => {
            path.push(LifeTypeProjection::Element);
            array_classes(element, path, out);
            path.pop();
        }
        Type::Tuple(fields) => {
            for (index, field) in fields.iter().enumerate() {
                path.push(LifeTypeProjection::Place(PlaceProjection::Field(
                    index as u32,
                )));
                array_classes(field, path, out);
                path.pop();
            }
        }
        Type::Record { fields, .. } => {
            for (index, (_, field)) in fields.iter().enumerate() {
                path.push(LifeTypeProjection::Place(PlaceProjection::Field(
                    index as u32,
                )));
                array_classes(field, path, out);
                path.pop();
            }
        }
        Type::Enum { variants, .. } => {
            for (variant, (_, fields)) in variants.iter().enumerate() {
                for (index, field) in fields.iter().enumerate() {
                    path.push(LifeTypeProjection::Place(PlaceProjection::Variant(
                        zil_ir::VariantProjection {
                            variant: variant as u32,
                            index: index as u32,
                        },
                    )));
                    array_classes(field, path, out);
                    path.pop();
                }
            }
        }
        _ => {}
    }
}

pub(super) fn charge(c: &StorageLifeContract, b: &mut Budget) -> CompileResult<()> {
    let rows = c.bindings.len().saturating_add(c.classes.len());
    b.spend(
        (c.fixed_binder_count as usize)
            .saturating_add(c.invocation_binder_count as usize)
            .saturating_add(rows.saturating_mul(rows))
            .saturating_add(rows.saturating_mul(2))
            .saturating_add(2),
        "signature relation row reservation",
    )?;
    for row in &c.bindings {
        b.spend(row.position.path.len(), "signature actual borrow path")?;
    }
    for row in &c.classes {
        b.spend(row.position.path.len(), "signature semantic class path")?;
    }
    Ok(())
}

pub(super) fn outer(c: &StorageLifeContract, base: StorageBase) -> Option<StorageBinder> {
    c.bindings
        .iter()
        .find(|r| r.position.base == base && r.position.path.is_empty())
        .map(|r| r.binder)
}

pub(super) fn relation(
    parameters: &[Type],
    result: &Type,
    c: &StorageLifeContract,
) -> CompileResult<()> {
    relation_inner(parameters, None, result, c, false, &[], false, &[], None)
}
pub(super) fn function_relation(
    parameters: &[Type],
    result: &Type,
    c: &StorageLifeContract,
    allow_callback: bool,
    closures: &[ClosureDefinition],
) -> CompileResult<()> {
    function_relation_inner(parameters, result, c, allow_callback, closures, false, &[])
}
fn function_relation_inner(
    parameters: &[Type],
    result: &Type,
    c: &StorageLifeContract,
    allow_callback: bool,
    closures: &[ClosureDefinition],
    allow_byte_input: bool,
    interfaces: &[DynInterface],
) -> CompileResult<()> {
    if let Type::ErrorUnion {
        result: success, ..
    } = result
        && let Type::Managed(payload) = success.as_ref()
        && let Type::Task { retained_life, .. } = payload.as_ref()
        && !retained_life.ports.is_empty()
    {
        super::tasks::require_closed_context(payload)?;
        let mut expected_bindings = Vec::new();
        let mut expected_classes = Vec::new();
        let inputs = parameters
            .iter()
            .enumerate()
            .filter(|(_, ty)| matches!(ty, Type::Address { .. }))
            .collect::<Vec<_>>();
        if inputs.len() != retained_life.ports.len() {
            return Err(bad(
                "borrowed Task signature input count differs from actual ports",
            ));
        }
        for (port, (input, ty)) in inputs.iter().enumerate() {
            if *ty != &retained_life.ports[port].leaf {
                return Err(bad(
                    "borrowed Task signature input type differs from actual port",
                ));
            }
            expected_bindings.push(StorageLifeBinding {
                position: StoragePosition {
                    base: StorageBase::Parameter(*input as u32),
                    path: vec![],
                },
                binder: StorageBinder::Invocation(retained_life.ports[port].binder),
            });
        }
        for (port, row) in retained_life.ports.iter().enumerate() {
            expected_bindings.push(StorageLifeBinding {
                position: StoragePosition {
                    base: StorageBase::Result,
                    path: vec![
                        ContractProjection::Place(PlaceProjection::ErrorArm(false)),
                        ContractProjection::RetainedPort(port as u32),
                    ],
                },
                binder: StorageBinder::Invocation(row.binder),
            });
        }
        for class in 0..retained_life.binder_count {
            expected_classes.push(SignatureLifeClassBinding {
                position: SignatureLifeClass {
                    base: StorageBase::Result,
                    path: vec![
                        LifeTypeProjection::Place(PlaceProjection::ErrorArm(false)),
                        LifeTypeProjection::Pointee,
                    ],
                    class,
                },
                binder: StorageBinder::Invocation(class),
            });
        }
        if c.fixed_binder_count != 0
            || c.invocation_binder_count != retained_life.binder_count
            || c.bindings != expected_bindings
            || c.classes != expected_classes
        {
            return Err(bad(
                "borrowed Task signature has no exact actual input and retained port relation",
            ));
        }
        return Ok(());
    }
    relation_inner(
        parameters,
        None,
        result,
        c,
        allow_callback,
        closures,
        allow_byte_input,
        interfaces,
        None,
    )
}

pub(super) fn closure_relation(
    closures: &[ClosureDefinition],
    kind: CallableKind,
    parameters: &[Type],
    captures: &[Type],
    result: &Type,
    c: &StorageLifeContract,
) -> CompileResult<()> {
    relation_inner(
        parameters,
        Some(captures),
        result,
        c,
        true,
        closures,
        false,
        &[],
        Some(kind),
    )
}
pub(super) fn function_relation_in_module(
    parameters: &[Type],
    result: &Type,
    c: &StorageLifeContract,
    allow_callback: bool,
    module: &TypedModule,
    allow_byte_input: bool,
) -> CompileResult<()> {
    function_relation_inner(
        parameters,
        result,
        c,
        allow_callback,
        &module.closures,
        allow_byte_input,
        &module.dyn_interfaces,
    )
}
#[allow(clippy::too_many_arguments)]
fn relation_inner(
    parameters: &[Type],
    captures: Option<&[Type]>,
    result: &Type,
    c: &StorageLifeContract,
    allow_callback: bool,
    closures: &[ClosureDefinition],
    allow_byte_input: bool,
    interfaces: &[DynInterface],
    receiver_kind: Option<CallableKind>,
) -> CompileResult<()> {
    if c.classes
        .iter()
        .any(|row| row.position.base == StorageBase::Receiver)
    {
        return Err(bad("receiver head has no persistent class directory"));
    }
    if c.fixed_binder_count == 0 && c.invocation_binder_count == 0 {
        return Err(bad(
            "vacuous borrowed signature must use the ordinary callable type",
        ));
    }
    let rows = c
        .bindings
        .len()
        .checked_add(c.classes.len())
        .ok_or_else(|| bad("signature row count overflow"))?;
    let domains = (c.fixed_binder_count as usize)
        .checked_add(c.invocation_binder_count as usize)
        .ok_or_else(|| bad("signature domain count overflow"))?;
    if domains > rows || captures.is_none() && c.fixed_binder_count as usize > c.classes.len() {
        return Err(bad("signature domain exceeds actual relation occurrences"));
    }
    let input_classes = c
        .classes
        .iter()
        .filter(|r| matches!(r.position.base, StorageBase::Parameter(_)))
        .count();
    if captures.is_none() && c.fixed_binder_count as usize > input_classes {
        return Err(bad("Fixed domain exceeds actual input class occurrences"));
    }
    struct Cursor<'a> {
        c: &'a StorageLifeContract,
        borrow: usize,
        class: usize,
        fixed: u32,
        invocation: u32,
        grounded: Vec<bool>,
        inputs: Option<(u32, u32)>,
        allow_callback: bool,
        allow_byte_input: bool,
        captured: bool,
        array_output_upper: Option<StorageBinder>,
        array_result_input: bool,
        closures: &'a [ClosureDefinition],
        interfaces: &'a [DynInterface],
    }
    impl Cursor<'_> {
        fn binder(&mut self, b: StorageBinder, retained: bool) -> CompileResult<()> {
            let (id, count, next, input) = match b {
                StorageBinder::Fixed(i) => (
                    i,
                    self.c.fixed_binder_count,
                    &mut self.fixed,
                    self.inputs.map(|x| x.0),
                ),
                StorageBinder::Invocation(i) => (
                    i,
                    self.c.invocation_binder_count,
                    &mut self.invocation,
                    self.inputs.map(|x| x.1),
                ),
            };
            if id >= count || id > *next || input.is_some_and(|n| id >= n) {
                return Err(bad("signature class is noncanonical or result-only"));
            }
            if id == *next {
                *next += 1;
            }
            if retained && let StorageBinder::Fixed(i) = b {
                self.grounded[i as usize] = true;
            }
            Ok(())
        }
        fn compound_child(
            &mut self,
            ty: &Type,
            base: StorageBase,
            path: &mut Vec<LifeTypeProjection>,
        ) -> CompileResult<()> {
            match ty {
                Type::Tuple(fields) => {
                    for (index, field) in fields.iter().enumerate() {
                        path.push(LifeTypeProjection::Place(PlaceProjection::Field(
                            u32::try_from(index)
                                .map_err(|_| bad("compound signature ordinal overflow"))?,
                        )));
                        self.compound_child(field, base, path)?;
                        path.pop();
                    }
                }
                Type::BorrowedStorage { life, .. } => {
                    if life.binder_count as usize > self.c.classes.len().saturating_sub(self.class)
                    {
                        return Err(bad("compound signature omits child classes"));
                    }
                    for class in 0..life.binder_count {
                        let row = &self.c.classes[self.class];
                        if row.position.base != base
                            || row.position.path != *path
                            || row.position.class != class
                        {
                            return Err(bad(
                                "compound signature class differs from actual child scope",
                            ));
                        }
                        let binder = row.binder;
                        let capture = matches!(base, StorageBase::Capture(_));
                        if capture && !matches!(binder, StorageBinder::Fixed(_)) {
                            return Err(bad("captured compound child class must be Fixed"));
                        }
                        self.class += 1;
                        self.binder(binder, capture || !self.captured && self.inputs.is_none())?;
                    }
                }
                Type::Managed(payload)
                    if matches!(
                        payload.as_ref(),
                        Type::Record { .. } | Type::ByteBuffer | Type::OwnedStore { .. }
                    ) => {}
                other if other.is_copy_value() => {}
                _ => {
                    return Err(bad(
                        "compound signature child needs its complete value consumer",
                    ));
                }
            }
            Ok(())
        }
        fn ty(&mut self, ty: &Type, base: StorageBase) -> CompileResult<()> {
            if matches!(ty, Type::Tuple(_)) && !ty.is_copy_value() {
                return self.compound_child(ty, base, &mut Vec::new());
            }
            let capture = matches!(base, StorageBase::Capture(_));
            let (target, pointed) = if let Type::Address { inner, .. } = ty {
                let row = self
                    .c
                    .bindings
                    .get(self.borrow)
                    .ok_or_else(|| bad("signature omits actual outer borrow"))?;
                if row.position.base != base || !row.position.path.is_empty() {
                    return Err(bad(
                        "signature borrow position is flattened or noncanonical",
                    ));
                }
                self.borrow += 1;
                if capture && !matches!(row.binder, StorageBinder::Fixed(_)) {
                    return Err(bad(
                        "captured Address must retain a Fixed environment class",
                    ));
                }
                self.binder(row.binder, capture)?;
                (inner.as_ref(), true)
            } else {
                (ty, false)
            };
            if super::stored_flow::shared_array_value(target) {
                if capture {
                    return Err(bad("array captures require their own retained consumer"));
                }
                let Type::Array { length, .. } = target else {
                    unreachable!()
                };
                for index in 0..*length {
                    let row =
                        self.c.bindings.get(self.borrow).ok_or_else(|| {
                            bad("array signature omits an actual element binding")
                        })?;
                    if row.position.base != base
                        || row.position.path.as_slice()
                            != [ContractProjection::Place(PlaceProjection::Field(index))]
                    {
                        return Err(bad(
                            "array signature element binding differs from actual ordinal",
                        ));
                    }
                    if matches!(base, StorageBase::Parameter(_))
                        && Some(row.binder) == self.array_output_upper
                    {
                        self.array_result_input = true;
                    }
                    self.borrow += 1;
                    self.binder(row.binder, false)?;
                }
                return Ok(());
            }
            let task = match target {
                Type::Task { .. } => Some((target, None, false)),
                Type::Managed(inner) | Type::Desc(inner)
                    if matches!(inner.as_ref(), Type::Task { .. }) =>
                {
                    Some((inner.as_ref(), None, true))
                }
                Type::ErrorUnion {
                    result: success, ..
                } => match success.as_ref() {
                    Type::Managed(inner) | Type::Desc(inner)
                        if matches!(inner.as_ref(), Type::Task { .. }) =>
                    {
                        Some((inner.as_ref(), Some(PlaceProjection::ErrorArm(false)), true))
                    }
                    _ => None,
                },
                _ => None,
            };
            if let Some((task, arm, wrapped)) = task {
                if capture {
                    return Err(bad("captured Task retained owner needs its own consumer"));
                }
                super::tasks::require_closed_context(task)?;
                let Type::Task { retained_life, .. } = task else {
                    unreachable!()
                };
                if retained_life.ports.is_empty() {
                    return Err(bad("closed Task has no rich retained class relation"));
                }
                let mut class_path = Vec::new();
                let mut borrow_path = Vec::new();
                if let Some(arm) = arm {
                    class_path.push(LifeTypeProjection::Place(arm));
                    borrow_path.push(ContractProjection::Place(arm));
                }
                if pointed {
                    class_path.push(LifeTypeProjection::Pointee);
                }
                if wrapped {
                    class_path.push(LifeTypeProjection::Pointee);
                }
                for (port, shape) in retained_life.ports.iter().enumerate() {
                    let row = self
                        .c
                        .bindings
                        .get(self.borrow)
                        .ok_or_else(|| bad("Task signature omits retained port"))?;
                    let mut path = borrow_path.clone();
                    path.push(ContractProjection::RetainedPort(port as u32));
                    if row.position.base != base || row.position.path != path {
                        return Err(bad("Task retained port relation differs from actual owner"));
                    }
                    let class_row = self
                        .c
                        .classes
                        .get(self.class + shape.binder as usize)
                        .ok_or_else(|| bad("Task retained class absent"))?;
                    if row.binder != class_row.binder {
                        return Err(bad("Task port and class bind different source lifetimes"));
                    }
                    self.borrow += 1;
                    self.binder(row.binder, self.inputs.is_none())?;
                }
                for local in 0..retained_life.binder_count {
                    let row = self
                        .c
                        .classes
                        .get(self.class)
                        .ok_or_else(|| bad("Task signature omits retained class"))?;
                    if row.position.base != base
                        || row.position.path != class_path
                        || row.position.class != local
                    {
                        return Err(bad(
                            "Task retained class position differs from actual owner",
                        ));
                    }
                    self.class += 1;
                    self.binder(row.binder, self.inputs.is_none())?;
                }
            } else if let Type::Dyn { interface } | Type::OwnedDyn { interface } = target {
                let item = self
                    .interfaces
                    .iter()
                    .find(|item| item.id == *interface)
                    .ok_or_else(|| bad("dyn signature class selector has no actual interface"))?;
                for local in 0..item.retained_life.binder_count {
                    let row = self.c.classes.get(self.class).ok_or_else(|| {
                        bad("dyn signature omits its complete Context class domain")
                    })?;
                    let path = if pointed {
                        row.position.path.as_slice() == [LifeTypeProjection::Pointee]
                    } else {
                        row.position.path.is_empty()
                    };
                    if row.position.base != base || row.position.class != local || !path {
                        return Err(bad(
                            "dyn signature class must select its actual Context boundary",
                        ));
                    }
                    if capture && !matches!(row.binder, StorageBinder::Fixed(_)) {
                        return Err(bad("captured dyn Context class must remain Fixed"));
                    }
                    self.class += 1;
                    self.binder(
                        row.binder,
                        capture || !self.captured && self.inputs.is_none(),
                    )?;
                }
            } else if let Type::BorrowedStorage { storage, life } = target {
                if (!matches!(
                    storage.as_ref(),
                    Type::Record { .. } | Type::Package { .. } | Type::Tuple(_)
                ) || (!life.retained.is_empty()
                    && !matches!(storage.as_ref(), Type::Package { .. })))
                    && !pure_nominal_value(target)
                    && !super::stored_flow::enum_address::signature(target)
                    && !super::stored_flow::option_address::signature(target)
                {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Analyze,
                        "signature qualified type needs its owning/nested consumer",
                        None,
                    )]);
                }
                if life.binder_count == 0
                    || life.binder_count as usize > self.c.classes.len().saturating_sub(self.class)
                {
                    return Err(bad("signature omits qualified class domain"));
                }
                for local in 0..life.binder_count {
                    let row = &self.c.classes[self.class];
                    if row.position.base != base
                        || row.position.class != local
                        || if pointed {
                            row.position.path.as_slice() != [LifeTypeProjection::Pointee]
                        } else {
                            !row.position.path.is_empty()
                        }
                    {
                        return Err(bad(
                            "signature class must name the exact qualified type boundary",
                        ));
                    }
                    self.class += 1;
                    if capture && !matches!(row.binder, StorageBinder::Fixed(_)) {
                        return Err(bad("captured qualified class must be Fixed"));
                    }
                    self.binder(
                        row.binder,
                        capture || !self.captured && self.inputs.is_none(),
                    )?;
                }
            } else if pure_array_value(target)
                || (matches!(target, Type::ErrorUnion { .. })
                    && super::stored_flow::enum_address::signature(target))
            {
                let mut path = if pointed {
                    vec![LifeTypeProjection::Pointee]
                } else {
                    Vec::new()
                };
                let mut classes = Vec::new();
                array_classes(target, &mut path, &mut classes);
                for (path, local) in classes {
                    let row = self
                        .c
                        .classes
                        .get(self.class)
                        .ok_or_else(|| bad("signature omits actual Array child class"))?;
                    if row.position.base != base
                        || row.position.path != path
                        || row.position.class != local
                    {
                        return Err(bad(
                            "signature Array child class differs from actual Element path",
                        ));
                    }
                    if capture && !matches!(row.binder, StorageBinder::Fixed(_)) {
                        return Err(bad("captured Array child class must be Fixed"));
                    }
                    self.class += 1;
                    self.binder(
                        row.binder,
                        capture || !self.captured && self.inputs.is_none(),
                    )?;
                }
            } else if self.allow_callback && matches!(target, Type::Closure(_)) {
                let Type::Closure(id) = target else {
                    unreachable!()
                };
                let definition = self
                    .closures
                    .get(id.0 as usize)
                    .ok_or_else(|| bad("closure class selector has no actual definition"))?;
                if let Some(life) = &definition.storage_life {
                    let count = life.fixed_binder_count as usize;
                    if count > self.c.classes.len().saturating_sub(self.class) {
                        return Err(bad("signature omits actual closure free Fixed domain"));
                    }
                    for class in 0..count {
                        let row = &self.c.classes[self.class];
                        if row.position.base != base
                            || row.position.class as usize != class
                            || if pointed {
                                row.position.path.as_slice() != [LifeTypeProjection::Pointee]
                            } else {
                                !row.position.path.is_empty()
                            }
                        {
                            return Err(bad(
                                "closure class selector must name its actual free Fixed domain",
                            ));
                        }
                        if capture && !matches!(row.binder, StorageBinder::Fixed(_)) {
                            return Err(bad("captured child Closure class must be Fixed"));
                        }
                        self.class += 1;
                        self.binder(row.binder, self.inputs.is_none())?;
                    }
                }
            } else if pointed {
                if !(target.is_address_pointee()
                    // A borrowed Package manager may ground a Store window.
                    // Mutable input shape does not supply a target or loan;
                    // body provenance and exclusive authority are checked later.
                    || self.inputs.is_none()
                        && matches!(base, StorageBase::Parameter(_))
                        && matches!(ty, Type::Address { .. })
                        && matches!(target, Type::Package { .. })
                    || self.allow_callback && matches!(target, Type::Closure(_)))
                {
                    return Err(bad("signature pointee is outside the implemented profile"));
                }
            } else if !(target.is_copy_value()
                || pure_array_value(target)
                // A proved query's exact weak input has no semantic class.
                // It creates/grounds no row and is not an owning consumer.
                || self.allow_byte_input && self.inputs.is_none()
                    && matches!(base,StorageBase::Parameter(_))
                    && matches!(ty,Type::Desc(payload) if payload.as_ref()==&Type::ByteBuffer)
                // A direct owned field contributes no class/borrow position.
                // Capture admission and the independent resource kernel still
                // establish its finite payload, actual mode and responsibility.
                || capture && matches!(target, Type::Managed(_))
                // An actual helper's finite Managed companion is a carrier,
                // not a class-bearing input. Body/resource proofs still settle it.
                || self.allow_callback && self.inputs.is_none()
                    && matches!(base, StorageBase::Parameter(_))
                    && matches!(target, Type::Managed(payload)
                        if matches!(payload.as_ref(), Type::Record { fields, .. }
                            if fields.iter().all(|(_, ty)| super::stored_callables::scalar(ty))))
                || self.allow_callback
                    && self.inputs.is_none()
                    && matches!(target, Type::Closure(_)))
            {
                return Err(bad("signature value needs its ownership consumer"));
            }
            Ok(())
        }
    }
    let mut cursor = Cursor {
        c,
        borrow: 0,
        class: 0,
        fixed: 0,
        invocation: 0,
        grounded: vec![false; c.fixed_binder_count as usize],
        inputs: None,
        allow_callback,
        allow_byte_input,
        captured: captures.is_some(),
        array_output_upper: match result {
            Type::Address { mutable: false, .. } => c
                .bindings
                .last()
                .filter(|row| {
                    row.position.base == StorageBase::Result && row.position.path.is_empty()
                })
                .map(|row| row.binder),
            _ => None,
        },
        array_result_input: false,
        closures,
        interfaces,
    };
    for (i, p) in parameters.iter().enumerate() {
        let i = u32::try_from(i).map_err(|_| bad("signature parameter ordinal overflow"))?;
        cursor.ty(p, StorageBase::Parameter(i))?;
    }
    if let Some(captures) = captures {
        for (i, capture) in captures.iter().enumerate() {
            let i = u32::try_from(i).map_err(|_| bad("capture ordinal overflow"))?;
            cursor.ty(capture, StorageBase::Capture(i))?;
        }
    }
    if let Some(row) = c.bindings.get(cursor.borrow)
        && row.position.base == StorageBase::Receiver
    {
        if !matches!(receiver_kind, Some(CallableKind::Fn | CallableKind::FnMut))
            || !row.position.path.is_empty()
            || !matches!(row.binder, StorageBinder::Invocation(_))
        {
            return Err(bad(
                "Receiver requires an actual non-consuming borrowed closure head",
            ));
        }
        if receiver_kind == Some(CallableKind::Fn)
            && matches!(result, Type::Address { mutable: true, .. })
            && c.bindings
                .iter()
                .any(|r| r.position.base == StorageBase::Result && r.binder == row.binder)
        {
            return Err(bad("shared receiver cannot grant an exclusive result view"));
        }
        cursor.borrow += 1;
        cursor.binder(row.binder, false)?;
    }
    cursor.inputs = Some((cursor.fixed, cursor.invocation));
    cursor.ty(result, StorageBase::Result)?;
    if cursor.borrow != c.bindings.len()
        || cursor.class != c.classes.len()
        || cursor.fixed != c.fixed_binder_count
        || cursor.invocation != c.invocation_binder_count
        || cursor.grounded.iter().any(|g| !g)
    {
        return Err(bad(
            "signature relation has missing/extra rows or ungrounded Fixed classes",
        ));
    }
    // A declared Address result still needs an actual authority-bearing input;
    // the body proof separately checks the exact returned target.
    if let Type::Address { mutable, .. } = result {
        let out = outer(c, StorageBase::Result);
        if !parameters.iter().enumerate().any(|(i, p)| {
            matches!(p,Type::Address{mutable:m,..}if !mutable||*m)
                && outer(c, StorageBase::Parameter(i as u32)) == out
        }) && !c.classes.iter().any(|r| {
            let StorageBase::Parameter(i) = r.position.base else {
                return false;
            };
            Some(r.binder) == out
                && (!mutable
                    || matches!(
                        parameters.get(i as usize),
                        Some(Type::Address { mutable: true, .. })
                    ))
                && (!mutable || r.position.path.contains(&LifeTypeProjection::Pointee))
        }) && !(matches!(receiver_kind, Some(CallableKind::Fn | CallableKind::FnMut))
            && (!mutable || receiver_kind == Some(CallableKind::FnMut))
            && outer(c, StorageBase::Receiver).is_some_and(|receiver| Some(receiver) == out))
            && !cursor.array_result_input
        {
            return Err(bad(
                "signature Address result has no eligible declared input",
            ));
        }
    }
    Ok(())
}

/// Each equality required by the concrete target must hold in the pointer's
/// universally callable relation. Distinct variables are not disjointness.
pub(super) fn entails(required: &StorageLifeContract, supplied: &StorageLifeContract) -> bool {
    if required.bindings.len() != supplied.bindings.len()
        || required.classes.len() != supplied.classes.len()
    {
        return false;
    }
    if !required
        .bindings
        .iter()
        .zip(&supplied.bindings)
        .all(|(a, z)| a.position == z.position)
        || !required
            .classes
            .iter()
            .zip(&supplied.classes)
            .all(|(a, z)| a.position == z.position)
    {
        return false;
    }
    let rows = required
        .bindings
        .iter()
        .map(|r| r.binder)
        .chain(required.classes.iter().map(|r| r.binder));
    let actual = supplied
        .bindings
        .iter()
        .map(|r| r.binder)
        .chain(supplied.classes.iter().map(|r| r.binder));
    for (left, mapped) in rows.clone().zip(actual.clone()) {
        for (right, other) in rows.clone().zip(actual.clone()) {
            if left == right && mapped != other {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tag() -> Type {
        Type::BorrowedStorage {
            storage: Box::new(Type::Record {
                name: "Tag".into(),
                fields: vec![("n".into(), Type::I32)],
            }),
            life: StorageLifeShape {
                binder_count: 1,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![NominalLifeArgument {
                    path: vec![],
                    declaration: LifeDeclarationId {
                        module: zil_base::ModuleId(0),
                        declaration: zil_base::NodeId(1),
                        compiler: false,
                    },
                    argument: 0,
                    binder: 0,
                }],
            },
        }
    }
    fn address(t: Type) -> Type {
        Type::Address {
            mutable: false,
            inner: Box::new(t),
        }
    }
    fn relation2(class: u32) -> StorageLifeContract {
        StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 2,
            bindings: (0..2)
                .map(|i| StorageLifeBinding {
                    position: StoragePosition {
                        base: StorageBase::Parameter(i),
                        path: vec![],
                    },
                    binder: StorageBinder::Invocation(i),
                })
                .collect(),
            classes: vec![SignatureLifeClassBinding {
                position: SignatureLifeClass {
                    base: StorageBase::Parameter(0),
                    path: vec![LifeTypeProjection::Pointee],
                    class: 0,
                },
                binder: StorageBinder::Invocation(class),
            }],
        }
    }
    #[test]
    fn impossible_domain_is_rejected_before_allocating_claimed_class_storage() {
        let impossible = StorageLifeContract {
            fixed_binder_count: u32::MAX,
            invocation_binder_count: 0,
            bindings: vec![],
            classes: vec![],
        };
        assert!(relation(&[], &Type::I32, &impossible).is_err());
    }
    #[test]
    fn captured_array_child_class_cannot_be_invocation_local() {
        let child = Type::Array {
            element: Box::new(tag()),
            length: 2,
        };
        let mut contract = StorageLifeContract {
            fixed_binder_count: 0,
            invocation_binder_count: 1,
            bindings: vec![],
            classes: vec![SignatureLifeClassBinding {
                position: SignatureLifeClass {
                    base: StorageBase::Capture(0),
                    path: vec![LifeTypeProjection::Element],
                    class: 0,
                },
                binder: StorageBinder::Invocation(0),
            }],
        };
        assert!(
            closure_relation(
                &[],
                CallableKind::Fn,
                &[],
                std::slice::from_ref(&child),
                &Type::Unit,
                &contract
            )
            .is_err()
        );
        contract.fixed_binder_count = 1;
        contract.invocation_binder_count = 0;
        contract.classes[0].binder = StorageBinder::Fixed(0);
        closure_relation(&[], CallableKind::Fn, &[], &[child], &Type::Unit, &contract).unwrap();
    }
    #[test]
    fn qualified_class_is_joint_with_outer_inputs_and_never_a_physical_borrow_row() {
        let p = vec![address(tag()), address(Type::I32)];
        let one = relation2(0);
        let two = relation2(1);
        relation(&p, &Type::I32, &one).unwrap();
        relation(&p, &Type::I32, &two).unwrap();
        assert!(!entails(&one, &two));
        assert!(!entails(&two, &one));
        assert!(entails(&one, &one));
        let mut flat = one.clone();
        flat.bindings.push(StorageLifeBinding {
            position: StoragePosition {
                base: StorageBase::Parameter(0),
                path: vec![ContractProjection::Place(PlaceProjection::Field(0))],
            },
            binder: StorageBinder::Invocation(0),
        });
        assert!(relation(&p, &Type::I32, &flat).is_err());
        let mut omitted = one;
        omitted.classes.clear();
        assert!(relation(&p, &Type::I32, &omitted).is_err());
    }
}
