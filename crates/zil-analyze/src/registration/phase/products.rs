//! Per-field phase transport for the original inputs of a failed constructor.
//! These are actual execution facts; a type or declared phase never fills them.
use super::*;
use std::collections::BTreeMap;
type Path = Vec<zil_ir::PlaceProjection>;
type Leaves = BTreeMap<Path, u8>;
#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct Products {
    values: Vec<Leaves>,
    locals: Vec<Leaves>,
}
impl Products {
    pub(super) fn new(f: &TypedFunction) -> Self {
        Self {
            values: vec![Leaves::new(); f.values.len()],
            locals: vec![Leaves::new(); f.locals.len()],
        }
    }
    pub(super) fn merge(&mut self, other: &Self) -> bool {
        let mut changed = false;
        for (to, from) in self
            .values
            .iter_mut()
            .zip(&other.values)
            .chain(self.locals.iter_mut().zip(&other.locals))
        {
            for (path, mask) in from {
                let entry = to.entry(path.clone()).or_default();
                let old = *entry;
                *entry |= *mask;
                changed |= old != *entry;
            }
        }
        changed
    }
    pub(super) fn work(&self) -> usize {
        self.values
            .iter()
            .chain(&self.locals)
            .flat_map(|v| v.keys())
            .fold(0usize, |n, p| n.saturating_add(p.len().saturating_add(2)))
    }
}
fn source(
    p: &Products,
    s: &Flow,
    f: &TypedFunction,
    v: ValueId,
    b: &mut Budget,
) -> CompileResult<Leaves> {
    let row = &p.values[v.0 as usize];
    b.spend(
        row.keys()
            .map(|p| p.len() + 2)
            .sum::<usize>()
            .saturating_add(1),
        "original input protocol source snapshot",
    )?;
    if is_owner(&f.values[v.0 as usize]) {
        Ok(BTreeMap::from([(vec![], s.values[v.0 as usize])]))
    } else {
        Ok(row.clone())
    }
}
fn select(
    row: &Leaves,
    prefix: &[zil_ir::PlaceProjection],
    b: &mut Budget,
) -> CompileResult<Leaves> {
    let mut result = Leaves::new();
    for (path, mask) in row {
        b.spend(
            path.len().saturating_add(prefix.len()).saturating_add(2),
            "original input phase projection",
        )?;
        if let Some(path) = path.strip_prefix(prefix) {
            result.insert(path.to_vec(), *mask);
        }
    }
    Ok(result)
}
fn append(
    to: &mut Leaves,
    from: Leaves,
    prefix: &[zil_ir::PlaceProjection],
    b: &mut Budget,
) -> CompileResult<()> {
    for (path, mask) in from {
        b.spend(
            path.len().saturating_add(prefix.len()).saturating_add(2),
            "original input phase container",
        )?;
        let mut key = prefix.to_vec();
        key.extend(path);
        *to.entry(key).or_default() |= mask;
    }
    Ok(())
}
pub(super) fn step(
    s: &mut Flow,
    i: &Instruction,
    f: &TypedFunction,
    m: &TypedModule,
    b: &mut Budget,
) -> CompileResult<()> {
    let Some(mut p) = s.products.take() else {
        return Ok(());
    };
    let mut out = Leaves::new();
    match &i.kind {
        InstructionKind::Store { local, value } => {
            p.locals[local.0 as usize] = source(&p, s, f, *value, b)?
        }
        InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
            b.spend(
                p.locals[local.0 as usize]
                    .keys()
                    .map(|p| p.len() + 2)
                    .sum::<usize>(),
                "original input phase local",
            )?;
            out = p.locals[local.0 as usize].clone();
            if matches!(i.kind, InstructionKind::TakeLocal { .. }) {
                p.locals[local.0 as usize].clear();
            }
        }
        InstructionKind::MakeOwnedTuple { fields }
        | InstructionKind::MakePackage { fields }
        | InstructionKind::MakeAggregate { fields } => {
            for (index, v) in fields.iter().enumerate() {
                append(
                    &mut out,
                    source(&p, s, f, *v, b)?,
                    &[zil_ir::PlaceProjection::Field(index as u32)],
                    b,
                )?;
            }
        }
        InstructionKind::MakeVariant { variant, fields } => {
            for (index, v) in fields.iter().enumerate() {
                append(
                    &mut out,
                    source(&p, s, f, *v, b)?,
                    &[zil_ir::PlaceProjection::Variant(
                        zil_ir::VariantProjection {
                            variant: *variant,
                            index: index as u32,
                        },
                    )],
                    b,
                )?;
            }
        }
        InstructionKind::MakeErrorUnion { value, is_error } => append(
            &mut out,
            source(&p, s, f, *value, b)?,
            &[zil_ir::PlaceProjection::ErrorArm(*is_error)],
            b,
        )?,
        InstructionKind::TakeErrorUnion { value, is_error }
        | InstructionKind::ReadErrorUnion { value, is_error } => {
            out = select(
                &source(&p, s, f, *value, b)?,
                &[zil_ir::PlaceProjection::ErrorArm(*is_error)],
                b,
            )?
        }
        InstructionKind::ExtractField { aggregate, index } => {
            out = select(
                &source(&p, s, f, *aggregate, b)?,
                &[zil_ir::PlaceProjection::Field(*index)],
                b,
            )?
        }
        InstructionKind::TakePackageField { local, index } => {
            out = select(
                &p.locals[local.0 as usize],
                &[zil_ir::PlaceProjection::Field(*index)],
                b,
            )?;
            p.locals[local.0 as usize]
                .retain(|path, _| path.first() != Some(&zil_ir::PlaceProjection::Field(*index)));
        }
        InstructionKind::TakeTupleField { local, path }
        | InstructionKind::TakeVariantField { local, path } => {
            out = select(&p.locals[local.0 as usize], path, b)?;
            p.locals[local.0 as usize].retain(|key, _| !key.starts_with(path));
        }
        InstructionKind::UnpackTuple { local, fields }
        | InstructionKind::UnpackVariant { local, fields, .. } => {
            let row = std::mem::take(&mut p.locals[local.0 as usize]);
            for (index, target) in fields.iter().enumerate() {
                let part = if let InstructionKind::UnpackVariant { variant, .. } = i.kind {
                    zil_ir::PlaceProjection::Variant(zil_ir::VariantProjection {
                        variant,
                        index: index as u32,
                    })
                } else {
                    zil_ir::PlaceProjection::Field(index as u32)
                };
                let selected = select(&row, &[part], b)?;
                if is_owner(&f.locals[target.0 as usize].ty) {
                    s.invalidate(target.0 as usize, b)?;
                    s.phases[target.0 as usize] = selected.get(&vec![]).copied().unwrap_or(0);
                    s.aliases[target.0 as usize] = Some(target.0 as usize);
                    s.generations[target.0 as usize] =
                        s.generations[target.0 as usize].saturating_add(1);
                }
                p.locals[target.0 as usize] = selected;
            }
        }
        InstructionKind::TaskCreateOwned {
            worker, arguments, ..
        } => {
            if let Some(c) = &m.functions[worker.0 as usize].protocol_contract {
                let input = *arguments
                    .get(c.parameter as usize)
                    .ok_or_else(|| invalid(f, "Task owning input protocol role absent"))?;
                let actual = s.values[input.0 as usize];
                if actual == 0 || actual & !mask(&c.entry) != 0 {
                    return Err(invalid(
                        f,
                        "Task owning constructor cannot satisfy the original input protocol state",
                    ));
                }
            }
            for (index, v) in arguments.iter().enumerate() {
                append(
                    &mut out,
                    source(&p, s, f, *v, b)?,
                    &[
                        zil_ir::PlaceProjection::ErrorArm(true),
                        zil_ir::PlaceProjection::Field(1),
                        zil_ir::PlaceProjection::Field(index as u32),
                    ],
                    b,
                )?;
            }
        }
        InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local)
        | InstructionKind::DropManaged { local }
        | InstructionKind::DropPackage { local }
        | InstructionKind::DropTuple { local }
        | InstructionKind::DropEnum { local }
        | InstructionKind::DropErrorUnion { local } => {
            p.locals[local.0 as usize].clear();
        }
        _ => {}
    }
    if let Some(value) = i.result {
        if is_owner(&f.values[value.0 as usize])
            && !matches!(
                i.kind,
                InstructionKind::Load(_) | InstructionKind::TakeLocal { .. }
            )
            && let Some(phase) = out.get(&vec![])
        {
            s.values[value.0 as usize] = *phase;
        }
        p.values[value.0 as usize] = out;
    }
    s.products = Some(p);
    Ok(())
}
