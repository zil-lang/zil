//! Verify's sparse physical-leaf phase reconstruction. Keys name actual SSA or
//! local storage and checked place projections; empty/unknown states stay empty.
use super::*;
use std::collections::BTreeMap;
type Path = Vec<zil_ir::PlaceProjection>;
#[derive(Clone, Default, PartialEq, Eq)]
pub(super) struct Fields(BTreeMap<(bool, u32, Path), u8>);
impl Fields {
    pub(super) fn cost(&self) -> usize {
        self.0.keys().fold(0usize, |n, (_, _, p)| {
            n.saturating_add(p.len().saturating_add(3))
        })
    }
    pub(super) fn join(&mut self, rhs: &Self) {
        for (key, value) in &rhs.0 {
            *self.0.entry(key.clone()).or_default() |= *value;
        }
    }
    fn rows(&self, local: bool, index: u32, b: &mut Budget) -> Check<Vec<(Path, u8)>> {
        b.spend(self.cost().saturating_add(1))?;
        Ok(self
            .0
            .iter()
            .filter(|((l, i, _), _)| *l == local && *i == index)
            .map(|((_, _, p), s)| (p.clone(), *s))
            .collect())
    }
    fn remove(&mut self, local: bool, index: u32) {
        self.0.retain(|(l, i, _), _| *l != local || *i != index);
    }
    fn set(&mut self, local: bool, index: u32, rows: Vec<(Path, u8)>, b: &mut Budget) -> Check<()> {
        b.spend(
            self.cost()
                .saturating_add(rows.iter().map(|(p, _)| p.len() + 3).sum::<usize>()),
        )?;
        self.remove(local, index);
        for (path, mask) in rows {
            *self.0.entry((local, index, path)).or_default() |= mask;
        }
        Ok(())
    }
}
fn value(
    fields: &Fields,
    s: &Flow,
    f: &TypedFunction,
    v: ValueId,
    b: &mut Budget,
) -> Check<Vec<(Path, u8)>> {
    if is_owner(&f.values[v.0 as usize]) {
        b.spend(1)?;
        Ok(vec![(vec![], s.values[v.0 as usize])])
    } else {
        fields.rows(false, v.0, b)
    }
}
fn projection(
    rows: Vec<(Path, u8)>,
    prefix: &[zil_ir::PlaceProjection],
    b: &mut Budget,
) -> Check<Vec<(Path, u8)>> {
    let mut result = vec![];
    for (path, mask) in rows {
        b.spend(path.len().saturating_add(prefix.len()).saturating_add(1))?;
        if let Some(path) = path.strip_prefix(prefix) {
            result.push((path.to_vec(), mask));
        }
    }
    Ok(result)
}
fn prefix(
    to: &mut Vec<(Path, u8)>,
    rows: Vec<(Path, u8)>,
    prefix: &[zil_ir::PlaceProjection],
    b: &mut Budget,
) -> Check<()> {
    for (path, mask) in rows {
        b.spend(path.len().saturating_add(prefix.len()).saturating_add(1))?;
        let mut joined = prefix.to_vec();
        joined.extend(path);
        to.push((joined, mask));
    }
    Ok(())
}
pub(super) fn step(
    s: &mut Flow,
    i: &Instruction,
    f: &TypedFunction,
    m: &TypedModule,
    b: &mut Budget,
) -> Check<()> {
    let Some(mut fields) = s.fields.take() else {
        return Ok(());
    };
    let mut produced = vec![];
    match &i.kind {
        InstructionKind::TaskCreateOwned {
            worker, arguments, ..
        } => {
            let target = &m.functions[worker.0 as usize];
            for contract in &target.protocol_contracts {
                let arg = *arguments
                    .get(contract.parameter as usize)
                    .ok_or_else(|| invalid(f, "Task input protocol role has no actual argument"))?;
                let actual = if is_owner(&f.values[arg.0 as usize]) {
                    s.values[arg.0 as usize]
                } else {
                    s.value_aliases[arg.0 as usize].map_or(0, |root| s.phases[root])
                };
                let admitted = mask(&contract.entry);
                if actual == 0 || actual & admitted != actual {
                    return Err(invalid(
                        f,
                        "Task owning constructor input does not satisfy every actual protocol state",
                    ));
                }
            }
            for (index, arg) in arguments.iter().enumerate() {
                prefix(
                    &mut produced,
                    value(&fields, s, f, *arg, b)?,
                    &[
                        zil_ir::PlaceProjection::ErrorArm(true),
                        zil_ir::PlaceProjection::Field(1),
                        zil_ir::PlaceProjection::Field(index as u32),
                    ],
                    b,
                )?;
            }
        }
        InstructionKind::Store {
            local,
            value: input,
        } => {
            let rows = value(&fields, s, f, *input, b)?;
            fields.set(true, local.0, rows, b)?;
        }
        InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
            produced = fields.rows(true, local.0, b)?;
            if matches!(i.kind, InstructionKind::TakeLocal { .. }) {
                fields.remove(true, local.0);
            }
        }
        InstructionKind::TakeErrorUnion {
            value: input,
            is_error,
        }
        | InstructionKind::ReadErrorUnion {
            value: input,
            is_error,
        } => {
            produced = projection(
                value(&fields, s, f, *input, b)?,
                &[zil_ir::PlaceProjection::ErrorArm(*is_error)],
                b,
            )?
        }
        InstructionKind::MakeErrorUnion {
            value: input,
            is_error,
        } => prefix(
            &mut produced,
            value(&fields, s, f, *input, b)?,
            &[zil_ir::PlaceProjection::ErrorArm(*is_error)],
            b,
        )?,
        InstructionKind::MakePackage { fields: inputs }
        | InstructionKind::MakeOwnedTuple { fields: inputs }
        | InstructionKind::MakeAggregate { fields: inputs } => {
            for (index, input) in inputs.iter().enumerate() {
                prefix(
                    &mut produced,
                    value(&fields, s, f, *input, b)?,
                    &[zil_ir::PlaceProjection::Field(index as u32)],
                    b,
                )?;
            }
        }
        InstructionKind::MakeVariant {
            variant,
            fields: inputs,
        } => {
            for (index, input) in inputs.iter().enumerate() {
                prefix(
                    &mut produced,
                    value(&fields, s, f, *input, b)?,
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
        InstructionKind::ExtractField { aggregate, index } => {
            produced = projection(
                value(&fields, s, f, *aggregate, b)?,
                &[zil_ir::PlaceProjection::Field(*index)],
                b,
            )?
        }
        InstructionKind::TakePackageField { local, index } => {
            let part = zil_ir::PlaceProjection::Field(*index);
            produced = projection(fields.rows(true, local.0, b)?, &[part], b)?;
            fields
                .0
                .retain(|(l, id, path), _| !(*l && *id == local.0 && path.first() == Some(&part)));
        }
        InstructionKind::TakeTupleField { local, path }
        | InstructionKind::TakeVariantField { local, path } => {
            produced = projection(fields.rows(true, local.0, b)?, path, b)?;
            fields
                .0
                .retain(|(l, id, p), _| !(*l && *id == local.0 && p.starts_with(path)));
        }
        InstructionKind::UnpackTuple {
            local,
            fields: targets,
        }
        | InstructionKind::UnpackVariant {
            local,
            fields: targets,
            ..
        } => {
            let rows = fields.rows(true, local.0, b)?;
            fields.remove(true, local.0);
            for (index, target) in targets.iter().enumerate() {
                b.spend(rows.iter().map(|(p, _)| p.len() + 2).sum())?;
                let part = if let InstructionKind::UnpackVariant { variant, .. } = i.kind {
                    zil_ir::PlaceProjection::Variant(zil_ir::VariantProjection {
                        variant,
                        index: index as u32,
                    })
                } else {
                    zil_ir::PlaceProjection::Field(index as u32)
                };
                let selected = projection(rows.clone(), &[part], b)?;
                if is_owner(&f.locals[target.0 as usize].ty) {
                    s.invalidate(target.0 as usize, b)?;
                    s.phases[target.0 as usize] = selected
                        .iter()
                        .filter(|(p, _)| p.is_empty())
                        .fold(0, |a, (_, m)| a | m);
                    s.aliases[target.0 as usize] = Some(target.0 as usize);
                    s.generations[target.0 as usize] =
                        s.generations[target.0 as usize].saturating_add(1);
                }
                fields.set(true, target.0, selected, b)?;
            }
        }
        InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local)
        | InstructionKind::DropManaged { local }
        | InstructionKind::DropPackage { local }
        | InstructionKind::DropTuple { local }
        | InstructionKind::DropEnum { local }
        | InstructionKind::DropErrorUnion { local } => fields.remove(true, local.0),
        _ => {}
    }
    if let Some(output) = i.result {
        if is_owner(&f.values[output.0 as usize])
            && !matches!(
                i.kind,
                InstructionKind::Load(_) | InstructionKind::TakeLocal { .. }
            )
            && let Some((_, mask)) = produced.iter().find(|(p, _)| p.is_empty())
        {
            s.values[output.0 as usize] = *mask;
        }
        fields.set(false, output.0, produced, b)?;
    }
    s.fields = Some(fields);
    Ok(())
}
