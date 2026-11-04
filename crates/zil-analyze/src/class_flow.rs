//! Signature equations are invariant metadata. This pass grants no roots,
//! storage initialization, parent loans, effects or physical access rights.
use super::*;

#[derive(Clone, Default)]
struct Ports {
    outer: Option<usize>,
    classes: Vec<usize>,
}
struct Equations {
    parent: Vec<usize>,
    rigid: Vec<Option<usize>>,
    next_rigid: usize,
}
impl Equations {
    fn fresh(&mut self, rigid: bool, b: &mut Budget) -> CompileResult<usize> {
        b.spend(3, "class variable allocation")?;
        let i = self.parent.len();
        self.parent.push(i);
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
        self.parent[z] = a;
        self.rigid[a] = self.rigid[a].or(self.rigid[z]);
        Ok(())
    }
    fn ports(&mut self, ty: &Type, b: &mut Budget) -> CompileResult<Ports> {
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
        b.spend(count, "class port allocation")?;
        let mut classes = Vec::with_capacity(count);
        for _ in 0..count {
            classes.push(self.fresh(false, b)?);
        }
        Ok(Ports { outer, classes })
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
    b.spend(
        super::signature_life::work(c),
        "class invocation variable domain",
    )?;
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
    b.spend(
        super::signature_life::work(c),
        "actual class position substitution",
    )?;
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
) -> CompileResult<()> {
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
) -> CompileResult<()> {
    let mut e = Equations {
        parent: Vec::new(),
        rigid: Vec::new(),
        next_rigid: 0,
    };
    b.spend(
        f.values.len().saturating_add(f.locals.len()),
        "function class fact allocation",
    )?;
    let mut values = Vec::new();
    for ty in &f.values {
        values.push(e.ports(ty, b)?);
    }
    let mut locals = Vec::new();
    for local in &f.locals {
        locals.push(e.ports(&local.ty, b)?);
    }
    let header = if let Some(c) = &f.storage_life {
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
        None
    };
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
                InstructionKind::Store { local, value } => {
                    e.copy(&locals[local.0 as usize], &values[value.0 as usize], b)?
                }
                InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local } => {
                    if let Some(out) = out {
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
                        call(c, arguments, i.result, &values, &mut e, b)?;
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
    if let (Some(c), Some(vars)) = (&f.storage_life, header) {
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
