//! Signature equations are invariant metadata. This pass grants no roots,
//! storage initialization, parent loans, effects or physical access rights.
use super::*;

#[derive(Clone, Default)]
struct Ports {
    outer: Option<usize>,
    classes: Vec<usize>,
    environment: bool,
}
struct Equations {
    parent: Vec<usize>,
    rigid: Vec<Option<usize>>,
    next_rigid: usize,
    grounded: Option<Vec<bool>>,
}
impl Equations {
    fn fresh(&mut self, rigid: bool, b: &mut Budget) -> CompileResult<usize> {
        b.spend(
            if self.grounded.is_some() { 4 } else { 3 },
            "class variable allocation",
        )?;
        let i = self.parent.len();
        self.parent.push(i);
        if let Some(grounded) = &mut self.grounded {
            grounded.push(rigid);
        }
        self.rigid.push(if rigid {
            let id = self.next_rigid;
            self.next_rigid += 1;
            Some(id)
        } else {
            None
        });
        Ok(i)
    }
    fn root(&self, mut i: usize, b: &mut Budget) -> CompileResult<usize> {
        b.spend(
            self.parent.len().saturating_add(1),
            "class variable ancestry",
        )?;
        while self.parent[i] != i {
            i = self.parent[i];
        }
        Ok(i)
    }
    fn equal(&mut self, a: usize, z: usize, b: &mut Budget) -> CompileResult<()> {
        let a = self.root(a, b)?;
        let z = self.root(z, b)?;
        if a == z {
            return Ok(());
        }
        if let (Some(a), Some(z)) = (self.rigid[a], self.rigid[z])
            && a != z
        {
            return Err(bad(
                "actual retained classes do not satisfy this invocation relation",
            ));
        }
        if let Some(grounded) = &mut self.grounded {
            b.spend(1, "actual class grounding propagation")?;
            grounded[a] |= grounded[z];
        }
        self.parent[z] = a;
        self.rigid[a] = self.rigid[a].or(self.rigid[z]);
        Ok(())
    }
    fn address_origin(&mut self, variable: usize, b: &mut Budget) -> CompileResult<()> {
        if self.grounded.is_some() {
            let root = self.root(variable, b)?;
            b.spend(1, "actual local Address class origin")?;
            if let Some(grounded) = &mut self.grounded {
                grounded[root] = true;
            }
        }
        Ok(())
    }
    fn retain(&mut self, variable: usize, b: &mut Budget) -> CompileResult<()> {
        let root = self.root(variable, b)?;
        b.spend(1, "freeze actual environment class")?;
        if !self.grounded.as_ref().is_some_and(|g| g[root]) {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "captured Fixed class has no actual header or Address origin",
                None,
            )]);
        }
        if self.rigid[root].is_none() {
            self.rigid[root] = Some(self.next_rigid);
            self.next_rigid += 1;
        }
        Ok(())
    }
    fn ports(&mut self, module: &TypedModule, ty: &Type, b: &mut Budget) -> CompileResult<Ports> {
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
        } else if let Type::Closure(id) = ty {
            module
                .closures
                .get(id.0 as usize)
                .and_then(|d| d.storage_life.as_ref())
                .map_or(0, |life| life.fixed_binder_count as usize)
        } else {
            0
        };
        b.spend(count, "class port allocation")?;
        let mut classes = Vec::with_capacity(count);
        for _ in 0..count {
            classes.push(self.fresh(false, b)?);
        }
        Ok(Ports {
            outer,
            classes,
            environment: matches!(ty, Type::Closure(_)) && count != 0,
        })
    }
    fn copy(&mut self, a: &Ports, z: &Ports, b: &mut Budget) -> CompileResult<()> {
        b.spend(
            a.classes
                .len()
                .saturating_add(z.classes.len())
                .saturating_add(1),
            "class port propagation",
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
) -> CompileResult<(Vec<usize>, Vec<usize>)> {
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
fn select(v: &(Vec<usize>, Vec<usize>), binder: StorageBinder) -> CompileResult<usize> {
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
) -> CompileResult<()> {
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
    fixed: Option<&[usize]>,
    e: &mut Equations,
    b: &mut Budget,
) -> CompileResult<()> {
    let formal = if let Some(fixed) = fixed {
        crate::signature_life::charge(c, b)?;
        b.spend(fixed.len(), "actual closure persistent Fixed snapshot")?;
        if fixed.len() != c.fixed_binder_count as usize {
            return Err(bad(
                "actual closure free Fixed domain differs from call relation",
            ));
        }
        let mut invocation = Vec::new();
        for _ in 0..c.invocation_binder_count {
            invocation.push(e.fresh(false, b)?);
        }
        (fixed.to_vec(), invocation)
    } else {
        variables(c, e, false, b)?
    };
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
) -> CompileResult<()> {
    check_with(module, f, reachable, b, f.storage_life.as_ref())
}
pub(super) fn check_role(
    module: &TypedModule,
    f: &TypedFunction,
    reachable: &[bool],
    life: &StorageLifeContract,
    b: &mut Budget,
) -> CompileResult<()> {
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
    if let Some(actual) = &d.storage_life {
        super::signature_life::charge(actual, b)?;
        if actual != life {
            return Err(vec![Diagnostic::unsupported(
                Phase::Analyze,
                "Fixed helper role needs the complete actual capture relation",
                None,
            )]);
        }
        super::signature_life::capture_relation(&d.parameters, &d.captures, &d.result, life)?;
    } else {
        super::borrowed_fp::relation(&d.parameters, &d.result, life)?;
    }
    check_with(module, f, reachable, b, Some(life))
}
fn check_with(
    module: &TypedModule,
    f: &TypedFunction,
    reachable: &[bool],
    b: &mut Budget,
    assumptions: Option<&StorageLifeContract>,
) -> CompileResult<()> {
    let mut e = Equations {
        parent: Vec::new(),
        rigid: Vec::new(),
        next_rigid: 0,
        grounded: b.capture_relations.then(Vec::new),
    };
    b.spend(
        f.values.len().saturating_add(f.locals.len()),
        "function class fact allocation",
    )?;
    let mut values = Vec::new();
    let mut retained_environment = false;
    for ty in &f.values {
        let p = e.ports(module, ty, b)?;
        retained_environment |= p.environment;
        values.push(p);
    }
    let mut locals = Vec::new();
    for local in &f.locals {
        let p = e.ports(module, &local.ty, b)?;
        retained_environment |= p.environment;
        locals.push(p);
    }
    let mut captures = Vec::new();
    if let Some(d) = f
        .environment
        .and_then(|id| module.closures.get(id.0 as usize))
        && d.storage_life.is_some()
    {
        b.spend(d.captures.len(), "actual entry capture class ports")?;
        for ty in &d.captures {
            captures.push(e.ports(module, ty, b)?);
        }
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
        for (i, capture) in captures.iter().enumerate() {
            bind(c, StorageBase::Capture(i as u32), capture, &vars, &mut e, b)?;
        }
        Some(vars)
    } else {
        if f.environment.is_some() || retained_environment {
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
    let mut deferred_helpers = Vec::new();
    let mut deferred_closures = Vec::new();
    for block in &f.blocks {
        for i in &block.instructions {
            b.spend(1, "class equation instruction")?;
            let out = i.result.map(|v| &values[v.0 as usize]);
            match &i.kind {
                InstructionKind::Load(local) => {
                    if let Some(out) = out {
                        e.copy(out, &locals[local.0 as usize], b)?;
                    }
                }
                InstructionKind::TakeLocal { local }
                    if super::stored_callables::fixed_value(
                        module,
                        &f.locals[local.0 as usize].ty,
                    ) =>
                {
                    if let Some(out) = out {
                        e.copy(out, &locals[local.0 as usize], b)?;
                    }
                }
                InstructionKind::ReadCapture { index } if !captures.is_empty() => {
                    if let Some(out) = out {
                        let capture = captures
                            .get(*index as usize)
                            .ok_or_else(|| bad("actual capture class source is absent"))?;
                        e.copy(out, capture, b)?;
                    }
                }
                InstructionKind::MakeClosure {
                    closure,
                    captures: actuals,
                } => {
                    let d = &module.closures[closure.0 as usize];
                    if let Some(c) = &d.storage_life {
                        let out = out
                            .ok_or_else(|| bad("captured environment lacks actual class result"))?;
                        b.spend(out.classes.len(), "environment Fixed class snapshot")?;
                        let mut invocation = Vec::new();
                        for _ in 0..c.invocation_binder_count {
                            invocation.push(e.fresh(false, b)?);
                        }
                        let vars = (out.classes.clone(), invocation);
                        for (index, value) in actuals.iter().enumerate() {
                            bind(
                                c,
                                StorageBase::Capture(index as u32),
                                &values[value.0 as usize],
                                &vars,
                                &mut e,
                                b,
                            )?;
                        }
                    }
                }
                InstructionKind::Store { local, value } => {
                    e.copy(&locals[local.0 as usize], &values[value.0 as usize], b)?
                }
                InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local } => {
                    if let Some(out) = out {
                        if let Some(outer) = out.outer {
                            e.address_origin(outer, b)?;
                        }
                        let stored = &locals[local.0 as usize];
                        b.spend(
                            out.classes.len().saturating_add(stored.classes.len()),
                            "borrowed carrier classes",
                        )?;
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
                        "stored class slot selection",
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
                            b.spend(
                                slot.path.len().saturating_add(1),
                                "aggregate physical class operand",
                            )?;
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
                        let defer = if retained_environment {
                            b.spend(arguments.len(), "actual environment argument detection")?;
                            arguments.iter().any(|v| values[v.0 as usize].environment)
                        } else {
                            false
                        };
                        if defer {
                            b.spend(1, "deferred actual environment helper call")?;
                            deferred_helpers.push((c, arguments.as_slice(), i.result));
                        } else {
                            call(c, arguments, i.result, &values, None, &mut e, b)?;
                        }
                    }
                }
                InstructionKind::CallClosureBorrowed { .. }
                | InstructionKind::CallClosure { .. }
                | InstructionKind::CallClosureShared { .. }
                | InstructionKind::CallClosureMut { .. }
                | InstructionKind::CallClosureOnce { .. } => {
                    let role = super::stored_callables::role(module, f, &i.kind, b)?;
                    let (definition, arguments) =
                        super::stored_callables::called(module, f, &i.kind)
                            .ok_or_else(|| bad("class call has no actual qualified closure"))?;
                    if let Some(actual) = &definition.storage_life {
                        let life = role.unwrap_or(actual);
                        if let Some(role) = role {
                            crate::signature_life::charge(actual, b)?;
                            crate::signature_life::charge(role, b)?;
                            if role != actual {
                                return Err(vec![Diagnostic::unsupported(
                                    Phase::Analyze,
                                    "Fixed role differs from complete actual capture relation",
                                    None,
                                )]);
                            }
                        }
                        let receiver = match &i.kind {
                            InstructionKind::CallClosureShared { callee, .. }
                            | InstructionKind::CallClosureMut { callee, .. } => {
                                &locals[callee.0 as usize]
                            }
                            InstructionKind::CallClosureBorrowed { callee, .. }
                            | InstructionKind::CallClosure { callee, .. }
                            | InstructionKind::CallClosureOnce { callee, .. } => {
                                &values[callee.0 as usize]
                            }
                            _ => unreachable!(),
                        };
                        b.spend(1, "deferred actual environment invocation")?;
                        deferred_closures.push((
                            life,
                            arguments,
                            i.result,
                            receiver.classes.as_slice(),
                        ));
                    } else if let Some(life) = role {
                        call(life, arguments, i.result, &values, None, &mut e, b)?;
                    }
                }
                InstructionKind::CallIndirect { callee, arguments } => {
                    let Type::BorrowedFunctionPointer { life, .. } = &f.values[callee.0 as usize]
                    else {
                        return Err(bad(
                            "stored indirect class call requires its complete borrowed signature",
                        ));
                    };
                    call(life, arguments, i.result, &values, None, &mut e, b)?;
                }
                _ => {}
            }
        }
    }
    // Ordinary sources and structural propagation precede any use of a free
    // environment class. Only grounded capture facts get persistent identities;
    // neither a local phantom constructor nor the first call can choose one.
    if retained_environment {
        b.spend(values.len(), "retained environment inventory")?;
        for p in &values {
            if p.environment {
                b.spend(p.classes.len(), "actual environment class retention")?;
                for variable in &p.classes {
                    e.retain(*variable, b)?;
                }
            }
        }
    }
    for (contract, arguments, output) in deferred_helpers {
        call(contract, arguments, output, &values, None, &mut e, b)?;
    }
    for (contract, arguments, output, fixed) in deferred_closures {
        call(contract, arguments, output, &values, Some(fixed), &mut e, b)?;
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
) -> CompileResult<()> {
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
            b.spend(
                target.parameters.len().saturating_add(1),
                "rich pointer target signature",
            )?;
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
                b.type_work(p, false, "rich indirect argument equality")?;
                if f.values.get(v.0 as usize) != Some(p) {
                    return Err(bad("indirect call actual argument differs"));
                }
            }
            b.type_work(result, false, "rich indirect result equality")?;
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
