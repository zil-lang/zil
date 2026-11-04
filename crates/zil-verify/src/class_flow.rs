//! Independently reconstruct invariant signature equations. This grants no roots,
//! storage initialization, parent loans, effects or physical access rights.
use super::*;
fn bad(message: &str) -> Diagnostic {
    reject("E-VERIFY-CLASS-EQUATION", None, message)
}

#[derive(Clone, Default)]
struct Ports {
    outer: Option<usize>,
    classes: Vec<usize>,
}
// Verify uses explicit representative substitution, not Analyze's parent arena.
struct Equations {
    representative: Vec<usize>,
    rigid: Vec<Option<usize>>,
    next_rigid: usize,
}
impl Equations {
    fn fresh(&mut self, rigid: bool, b: &mut Budget) -> Check<usize> {
        b.spend(3)?;
        let id = self.representative.len();
        self.representative.push(id);
        self.rigid.push(if rigid {
            let label = self.next_rigid;
            self.next_rigid += 1;
            Some(label)
        } else {
            None
        });
        Ok(id)
    }
    fn equal(&mut self, a: usize, z: usize, b: &mut Budget) -> Check<()> {
        b.spend(
            self.representative
                .len()
                .saturating_mul(2)
                .saturating_add(1),
        )?;
        let a = self.representative[a];
        let z = self.representative[z];
        if a == z {
            return Ok(());
        }
        if self.rigid[a].is_some() && self.rigid[z].is_some() && self.rigid[a] != self.rigid[z] {
            return Err(bad(
                "actual retained class equality is not universally valid",
            ));
        }
        for row in &mut self.representative {
            if *row == z {
                *row = a;
            }
        }
        self.rigid[a] = self.rigid[a].or(self.rigid[z]);
        Ok(())
    }
    fn ports(&mut self, ty: &Type, b: &mut Budget) -> Check<Ports> {
        let outer = if matches!(ty, Type::Address { .. }) {
            Some(self.fresh(false, b)?)
        } else {
            None
        };
        let ty = if let Type::Address { inner, .. } = ty {
            inner.as_ref()
        } else {
            ty
        };
        let count = if let Type::BorrowedStorage { life, .. } = ty {
            life.binder_count as usize
        } else {
            0
        };
        b.spend(count)?;
        let mut classes = Vec::with_capacity(count);
        for _ in 0..count {
            classes.push(self.fresh(false, b)?);
        }
        Ok(Ports { outer, classes })
    }
    fn copy(&mut self, a: &Ports, z: &Ports, b: &mut Budget) -> Check<()> {
        b.spend(
            a.classes
                .len()
                .saturating_add(z.classes.len())
                .saturating_add(1),
        )?;
        if a.classes.len() != z.classes.len() {
            return Err(bad("class port arity differs from actual value"));
        }
        if let (Some(a), Some(z)) = (a.outer, z.outer) {
            self.equal(a, z, b)?;
        }
        for (a, z) in a.classes.iter().zip(&z.classes) {
            self.equal(*a, *z, b)?;
        }
        Ok(())
    }
}
fn shape(ty: &Type) -> Option<&StorageLifeShape> {
    let ty = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    if let Type::BorrowedStorage { life, .. } = ty {
        Some(life)
    } else {
        None
    }
}
fn variables(
    c: &StorageLifeContract,
    e: &mut Equations,
    rigid: bool,
    b: &mut Budget,
) -> Check<(Vec<usize>, Vec<usize>)> {
    crate::signature_life::charge(c, b)?;
    let mut fixed = Vec::new();
    let mut invocation = Vec::new();
    for _ in 0..c.fixed_binder_count {
        fixed.push(e.fresh(rigid, b)?);
    }
    for _ in 0..c.invocation_binder_count {
        invocation.push(e.fresh(rigid, b)?);
    }
    Ok((fixed, invocation))
}
fn select(v: &(Vec<usize>, Vec<usize>), binder: StorageBinder) -> Check<usize> {
    let value = match binder {
        StorageBinder::Fixed(i) => v.0.get(i as usize),
        StorageBinder::Invocation(i) => v.1.get(i as usize),
    };
    value
        .copied()
        .ok_or_else(|| bad("class binder outside invocation domain"))
}
fn bind(
    c: &StorageLifeContract,
    base: StorageBase,
    p: &Ports,
    v: &(Vec<usize>, Vec<usize>),
    e: &mut Equations,
    b: &mut Budget,
) -> Check<()> {
    crate::signature_life::charge(c, b)?;
    for row in &c.bindings {
        if row.position.base == base {
            if !row.position.path.is_empty() {
                return Err(bad(
                    "class substitution cannot interpret physical/retained access paths",
                ));
            }
            e.equal(
                p.outer
                    .ok_or_else(|| bad("borrow binding has no actual Address"))?,
                select(v, row.binder)?,
                b,
            )?;
        }
    }
    for row in &c.classes {
        if row.position.base == base {
            let actual = p
                .classes
                .get(row.position.class as usize)
                .ok_or_else(|| bad("qualified class has no actual carrier position"))?;
            e.equal(*actual, select(v, row.binder)?, b)?;
        }
    }
    Ok(())
}
fn call(
    c: &StorageLifeContract,
    args: &[ValueId],
    output: Option<ValueId>,
    values: &[Ports],
    e: &mut Equations,
    b: &mut Budget,
) -> Check<()> {
    let formal = variables(c, e, false, b)?;
    for (i, arg) in args.iter().enumerate() {
        bind(
            c,
            StorageBase::Parameter(i as u32),
            &values[arg.0 as usize],
            &formal,
            e,
            b,
        )?;
    }
    if let Some(out) = output {
        bind(
            c,
            StorageBase::Result,
            &values[out.0 as usize],
            &formal,
            e,
            b,
        )?;
    }
    Ok(())
}

pub(super) fn check(
    module: &TypedModule,
    f: &TypedFunction,
    reachable: &[bool],
    b: &mut Budget,
) -> Check<()> {
    check_with(module, f, reachable, b, f.storage_life.as_ref())
}
pub(super) fn check_role(
    module: &TypedModule,
    f: &TypedFunction,
    reachable: &[bool],
    life: &StorageLifeContract,
    b: &mut Budget,
) -> Check<()> {
    crate::signature_life::charge(life, b)?;
    let d = module
        .closures
        .get(
            f.environment
                .ok_or_else(|| bad("role actual entry has no environment"))?
                .0 as usize,
        )
        .ok_or_else(|| bad("role entry environment is absent"))?;
    super::borrowed_fp::signature_work(&d.parameters, &d.result, b)?;
    super::signature_life::relation(&d.parameters, &d.result, life, Some(f.span))?;
    check_with(module, f, reachable, b, Some(life))
}
fn check_with(
    module: &TypedModule,
    f: &TypedFunction,
    reachable: &[bool],
    b: &mut Budget,
    assumptions: Option<&StorageLifeContract>,
) -> Check<()> {
    let mut e = Equations {
        representative: Vec::new(),
        rigid: Vec::new(),
        next_rigid: 0,
    };
    b.spend(f.values.len().saturating_add(f.locals.len()))?;
    let mut values = Vec::new();
    for ty in &f.values {
        values.push(e.ports(ty, b)?);
    }
    let mut locals = Vec::new();
    for local in &f.locals {
        locals.push(e.ports(&local.ty, b)?);
    }
    let header = if let Some(c) = assumptions {
        let vars = variables(c, &mut e, true, b)?;
        for (i, local) in f.parameters.iter().enumerate() {
            bind(
                c,
                StorageBase::Parameter(i as u32),
                &locals[local.0 as usize],
                &vars,
                &mut e,
                b,
            )?;
        }
        Some(vars)
    } else {
        if f.environment.is_some() {
            for local in &f.parameters {
                let p = &locals[local.0 as usize];
                if let Some(outer) = p.outer {
                    let rigid = e.fresh(true, b)?;
                    e.equal(outer, rigid, b)?;
                }
                for class in &p.classes {
                    let rigid = e.fresh(true, b)?;
                    e.equal(*class, rigid, b)?;
                }
            }
        }
        None
    };
    for block in &f.blocks {
        for i in &block.instructions {
            b.spend(1)?;
            let out = i.result.map(|v| &values[v.0 as usize]);
            match &i.kind {
                InstructionKind::Load(local) => {
                    if let Some(out) = out {
                        e.copy(out, &locals[local.0 as usize], b)?;
                    }
                }
                InstructionKind::Store { local, value } => {
                    e.copy(&locals[local.0 as usize], &values[value.0 as usize], b)?
                }
                InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local } => {
                    if let Some(out) = out {
                        let stored = &locals[local.0 as usize];
                        b.spend(out.classes.len().saturating_add(stored.classes.len()))?;
                        if out.classes.len() != stored.classes.len() {
                            return Err(bad("borrowed actual class arity differs"));
                        }
                        for (a, z) in out.classes.iter().zip(&stored.classes) {
                            e.equal(*a, *z, b)?;
                        }
                    }
                }
                InstructionKind::ReborrowAddress { address, .. } => {
                    if let Some(out) = out {
                        e.copy(out, &values[address.0 as usize], b)?;
                    }
                }
                InstructionKind::AddressField { address, .. }
                | InstructionKind::AddressIndex { address, .. }
                | InstructionKind::AddressSlice { address, .. } => {
                    if let (Some(out), Some(parent)) =
                        (out.and_then(|o| o.outer), values[address.0 as usize].outer)
                    {
                        e.equal(out, parent, b)?;
                    }
                }
                InstructionKind::LoadStoredBorrow { storage, path, .. } => {
                    let actual = shape(&f.values[storage.0 as usize])
                        .ok_or_else(|| bad("stored load lacks actual class shape"))?;
                    b.spend(
                        actual
                            .slots
                            .len()
                            .saturating_mul(path.len().saturating_add(1)),
                    )?;
                    let leaf = actual
                        .slots
                        .iter()
                        .find(|s| s.path == *path)
                        .ok_or_else(|| bad("stored class slot is absent"))?;
                    let class = values[storage.0 as usize].classes[leaf.binder as usize];
                    e.equal(
                        out.and_then(|o| o.outer)
                            .ok_or_else(|| bad("stored class load lacks Address result"))?,
                        class,
                        b,
                    )?;
                }
                InstructionKind::MakeAggregate { fields } => {
                    if let (Some(out), Some(id)) = (out, i.result)
                        && let Some(actual) = shape(&f.values[id.0 as usize])
                    {
                        for slot in &actual.slots {
                            b.spend(slot.path.len().saturating_add(1))?;
                            let [PlaceProjection::Field(index)] = slot.path.as_slice() else {
                                return Err(bad(
                                    "nested constructor class requires its physical operand consumer",
                                ));
                            };
                            let arg = fields
                                .get(*index as usize)
                                .ok_or_else(|| bad("aggregate class operand is absent"))?;
                            e.equal(
                                out.classes[slot.binder as usize],
                                values[arg.0 as usize]
                                    .outer
                                    .ok_or_else(|| bad("stored class operand is not an Address"))?,
                                b,
                            )?;
                        }
                    }
                }
                InstructionKind::Call {
                    function,
                    arguments,
                } => {
                    let callee = &module.functions[function.0 as usize];
                    if let Some(c) = &callee.storage_life {
                        call(c, arguments, i.result, &values, &mut e, b)?;
                    }
                }
                InstructionKind::CallClosureBorrowed { .. }
                | InstructionKind::CallClosureShared { .. } => {
                    if let Some(life) = super::stored_callables::role(f, &i.kind, b)? {
                        let (_, arguments) = super::stored_callables::called(module, f, &i.kind)
                            .ok_or_else(|| bad("role call has no actual qualified closure"))?;
                        call(life, arguments, i.result, &values, &mut e, b)?;
                    }
                }
                InstructionKind::CallIndirect { callee, arguments } => {
                    let Type::BorrowedFunctionPointer { life, .. } = &f.values[callee.0 as usize]
                    else {
                        return Err(bad(
                            "stored indirect class call requires its complete borrowed signature",
                        ));
                    };
                    call(life, arguments, i.result, &values, &mut e, b)?;
                }
                _ => {}
            }
        }
    }
    if let (Some(c), Some(vars)) = (assumptions, header) {
        for (block, live) in f.blocks.iter().zip(reachable) {
            if *live && let Terminator::Return(Some(v)) = block.terminator {
                bind(
                    c,
                    StorageBase::Result,
                    &values[v.0 as usize],
                    &vars,
                    &mut e,
                    b,
                )?;
            }
        }
    }
    Ok(())
}

/// The first rich executable pointer lane returns ordinary Copy values. Actual
/// returned-target substitution remains closed here until separately proved.
fn scalar(t: &Type) -> bool {
    matches!(
        t,
        Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::U8
            | Type::U16
            | Type::F32
            | Type::F64
    )
}
pub(super) fn pointer_instruction(f: &TypedFunction, i: &Instruction) -> bool {
    match &i.kind {
        InstructionKind::FunctionRef { .. } => i
            .result
            .and_then(|v| f.values.get(v.0 as usize))
            .is_some_and(|t| matches!(t,Type::BorrowedFunctionPointer{result,..}if scalar(result))),
        InstructionKind::CallIndirect { callee, .. } => f
            .values
            .get(callee.0 as usize)
            .is_some_and(|t| matches!(t,Type::BorrowedFunctionPointer{result,..}if scalar(result))),
        _ => false,
    }
}

pub(super) fn pointer_types(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<()> {
    match &i.kind {
        InstructionKind::FunctionRef { function } => {
            let ty = i
                .result
                .and_then(|v| f.values.get(v.0 as usize))
                .ok_or_else(|| bad("function reference lacks actual result"))?;
            let Type::BorrowedFunctionPointer {
                parameters,
                result,
                life,
            } = ty
            else {
                return Err(bad("stored pointer producer needs full borrowed signature"));
            };
            let target = module
                .functions
                .get(function.0 as usize)
                .ok_or_else(|| bad("pointer target is absent"))?;
            b.spend(target.parameters.len().saturating_add(1))?;
            super::borrowed_fp::signature_work(parameters, result, b)?;
            if target.foreign.is_some()
                || target.environment.is_some()
                || target.call_kind != CallKind::Direct
                || target.callable_contract.is_some()
                || target.parameters.len() != parameters.len()
                || target.return_type != **result
            {
                return Err(bad("rich pointer target complete signature/effect differs"));
            }
            for (p, ty) in target.parameters.iter().zip(parameters) {
                if target.locals.get(p.0 as usize).map(|p| &p.ty) != Some(ty) {
                    return Err(bad("rich pointer target parameter differs"));
                }
            }
            if let Some(required) = &target.storage_life {
                super::signature_life::charge(required, b)?;
                super::signature_life::charge(life, b)?;
                if !super::signature_life::entails(required, life) {
                    return Err(bad(
                        "pointer relation does not entail actual target class requirements",
                    ));
                }
            } else if !life.classes.is_empty() {
                return Err(bad(
                    "qualified pointer target omits its actual class contract",
                ));
            }
        }
        InstructionKind::CallIndirect { callee, arguments } => {
            let Type::BorrowedFunctionPointer {
                parameters, result, ..
            } = &f.values[callee.0 as usize]
            else {
                return Err(bad("indirect call lacks rich signature"));
            };
            if parameters.len() != arguments.len() {
                return Err(bad("indirect call arity differs"));
            }
            for (p, v) in parameters.iter().zip(arguments) {
                resources::charge_payload(p, b)?;
                resources::charge_payload(p, b)?;
                if f.values.get(v.0 as usize) != Some(p) {
                    return Err(bad("indirect call actual argument differs"));
                }
            }
            resources::charge_payload(result, b)?;
            resources::charge_payload(result, b)?;
            if **result == Type::Unit {
                if i.result.is_some() {
                    return Err(bad("Unit indirect call fabricates result"));
                }
            } else if i.result.and_then(|v| f.values.get(v.0 as usize)) != Some(result.as_ref()) {
                return Err(bad("rich indirect result differs"));
            }
        }
        _ => {}
    }
    Ok(())
}
