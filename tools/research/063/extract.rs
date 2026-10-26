use crate::{
    budget::{MAX_BITS, MAX_POINTS, MAX_SLOTS, R, Work},
    schema::Schema,
};
use std::collections::{BTreeMap, BTreeSet};
use zil_ir::*;

pub struct Layout {
    pub schema: Schema,
    pub owners: Vec<String>,
    pub descriptions: Vec<String>,
    pub roots: Vec<String>,
    pub tokens: Vec<String>,
    pub locals: Vec<String>,
    pub points: Vec<(usize, usize)>,
    pub starts: Vec<usize>,
    pub after: Vec<BTreeSet<String>>,
    pub labels: Vec<String>,
}
pub fn owned(m: &TypedModule, t: &Type) -> bool {
    match t {
        Type::Managed(_) => true,
        Type::Closure(c) => m.closures.get(c.0 as usize).is_some_and(|c| {
            c.kind == CallableKind::FnOnce
                || c.captures.iter().any(|t| matches!(t, Type::Managed(_)))
        }),
        _ => false,
    }
}
pub fn described(m: &TypedModule, t: &Type) -> bool {
    matches!(t, Type::Desc(_))
        || matches!(t,Type::Closure(c) if m.closures.get(c.0 as usize).is_some_and(|c|c.captures.iter().any(|t|matches!(t,Type::Desc(_)))))
}
pub fn scalar_field(t: &Type) -> Option<(&'static str, usize)> {
    match t {
        Type::Bool => Some(("b", 1)),
        Type::I32 | Type::I64 => Some(("n", 5)),
        Type::ErrorUnion { .. } => Some(("e", 1)),
        Type::Enum { .. } => Some(("v", 4)),
        _ => None,
    }
}
pub fn inputs(k: &InstructionKind) -> Vec<ValueId> {
    match k {
        InstructionKind::MakeErrorUnion { value, .. }
        | InstructionKind::ErrorTag { value }
        | InstructionKind::ReadErrorUnion { value, .. }
        | InstructionKind::Unary { operand: value, .. }
        | InstructionKind::VariantTag { value }
        | InstructionKind::ReadVariant { value, .. }
        | InstructionKind::ExtractField {
            aggregate: value, ..
        } => vec![*value],
        InstructionKind::ReadDesc { desc, .. } => vec![*desc],
        InstructionKind::WriteDesc { desc, value, .. } => vec![*desc, *value],
        InstructionKind::Store { value, .. } => vec![*value],
        InstructionKind::Binary { left, right, .. } => vec![*left, *right],
        InstructionKind::Call { arguments, .. }
        | InstructionKind::ForeignCall { arguments, .. }
        | InstructionKind::CallClosureShared { arguments, .. }
        | InstructionKind::CallClosureMut { arguments, .. } => arguments.clone(),
        InstructionKind::CallClosure { callee, arguments }
        | InstructionKind::CallClosureOnce { callee, arguments }
        | InstructionKind::CallIndirect { callee, arguments } => std::iter::once(*callee)
            .chain(arguments.iter().copied())
            .collect(),
        InstructionKind::MakeClosure { captures, .. } => captures.clone(),
        InstructionKind::MakeManaged { fields }
        | InstructionKind::MakeAggregate { fields }
        | InstructionKind::MakeVariant { fields, .. } => fields.clone(),
        InstructionKind::ExtractIndex { aggregate, index } => vec![*aggregate, *index],
        InstructionKind::UpdateIndex {
            aggregate,
            index,
            value,
        } => vec![*aggregate, *index, *value],
        InstructionKind::UpdateField {
            aggregate, value, ..
        } => vec![*aggregate, *value],
        InstructionKind::WriteCapture { value, .. } => vec![*value],
        _ => vec![],
    }
}
pub fn extract(m: &TypedModule, f: &TypedFunction, work: &mut Work) -> R<Layout> {
    if m.schema_version != 9 {
        return Err("Unsupported: research 063 requires supported schema 9 subset".into());
    }
    let captures = f
        .environment
        .map_or(0, |c| m.closures[c.0 as usize].captures.len());
    if f.locals
        .len()
        .saturating_add(f.values.len())
        .saturating_add(captures)
        > MAX_SLOTS
    {
        return Err("ResourceLimit: slot metadata".into());
    }
    work.charge(f.blocks.len())?;
    let point_count = f.blocks.iter().try_fold(0usize, |n, b| {
        n.checked_add(b.instructions.len())
            .and_then(|n| n.checked_add(1))
            .ok_or("ResourceLimit: point overflow")
    })?;
    if point_count > MAX_POINTS {
        return Err("ResourceLimit: points".into());
    }
    work.charge(
        f.locals
            .len()
            .saturating_add(f.values.len())
            .saturating_add(f.blocks.len()),
    )?;
    let mut owners = Vec::new();
    let mut descriptions = Vec::new();
    let mut roots = Vec::new();
    let mut tokens = Vec::new();
    let mut fields = BTreeMap::new();
    for (key, t) in f
        .locals
        .iter()
        .map(|l| (format!("L{}", l.id.0), &l.ty))
        .chain(
            f.values
                .iter()
                .enumerate()
                .map(|(i, t)| (format!("V{i}"), t)),
        )
    {
        if matches!(
            t,
            Type::Address { .. } | Type::Package { .. } | Type::Unsupported(_)
        ) {
            return Err("Unsupported: address/unknown type outside joint resource query".into());
        }
        if owned(m, t) {
            owners.push(key.clone());
            if key.starts_with('L') {
                roots.push(key.clone());
            }
        }
        if described(m, t) {
            descriptions.push(key.clone());
        }
        if let Type::Enum { variants, .. } = t
            && variants.len() > 15
        {
            return Err("Unsupported: enum tag domain exceeds 15".into());
        }
        if let Some((prefix, width)) = scalar_field(t) {
            fields.insert(format!("{prefix}:{key}"), width);
        }
    }
    for (i, l) in f.parameters.iter().enumerate() {
        let t = &f.locals[l.0 as usize].ty;
        if owned(m, t) {
            tokens.push(format!("P{i}"));
        }
        if described(m, t) {
            roots.push(format!("P{i}"));
        }
    }
    if let Some(c) = f.environment {
        for (i, t) in m.closures[c.0 as usize].captures.iter().enumerate() {
            if let Type::Enum { variants, .. } = t
                && variants.len() > 15
            {
                return Err("Unsupported: capture enum tag domain exceeds 15".into());
            }
            if matches!(t, Type::Managed(_)) {
                owners.push(format!("C{i}"));
                roots.push(format!("C{i}"));
                tokens.push(format!("C{i}"));
            } else if matches!(t, Type::Desc(_)) {
                roots.push(format!("B{i}"));
            } else if let Some((prefix, width)) = scalar_field(t) {
                fields.insert(format!("{prefix}:C{i}"), width);
            }
        }
    }
    work.charge(point_count.saturating_mul(4))?;
    let mut points = Vec::new();
    let mut labels = Vec::new();
    let mut starts = Vec::new();
    for (b, block) in f.blocks.iter().enumerate() {
        starts.push(points.len());
        for (i, ins) in block.instructions.iter().enumerate() {
            let variable_fields = match &ins.kind {
                InstructionKind::Call { arguments, .. }
                | InstructionKind::ForeignCall { arguments, .. }
                | InstructionKind::CallClosure { arguments, .. }
                | InstructionKind::CallClosureShared { arguments, .. }
                | InstructionKind::CallClosureMut { arguments, .. }
                | InstructionKind::CallClosureOnce { arguments, .. }
                | InstructionKind::CallIndirect { arguments, .. } => arguments.len(),
                InstructionKind::MakeManaged { fields }
                | InstructionKind::MakeAggregate { fields }
                | InstructionKind::MakeVariant { fields, .. } => fields.len(),
                InstructionKind::MakeClosure { captures, .. } => captures.len(),
                InstructionKind::Unsupported(_) => return Err("Unsupported: instruction".into()),
                _ => 0,
            };
            work.charge(512usize.saturating_add(variable_fields.saturating_mul(40)))?;
            points.push((b, i));
            labels.push(format!(
                "{}:{i}:{:?}:{:?}",
                block.id.0, ins.result, ins.kind
            ));
            if let Some(v) = ins.result
                && owned(m, &f.values[v.0 as usize])
                && matches!(
                    ins.kind,
                    InstructionKind::MakeManaged { .. }
                        | InstructionKind::MakeClosure { .. }
                        | InstructionKind::Call { .. }
                )
            {
                tokens.push(format!("V{}", v.0));
            }
        }
        work.charge(256)?;
        points.push((b, block.instructions.len()));
        labels.push(format!(
            "{}:{}:{:?}",
            block.id.0,
            block.instructions.len(),
            block.terminator
        ));
    }
    owners.sort();
    descriptions.sort();
    roots.sort();
    roots.dedup();
    tokens.sort();
    tokens.dedup();
    let locals = owners
        .iter()
        .filter(|k| k.starts_with('L'))
        .cloned()
        .collect::<Vec<_>>();
    work.charge(fields.len())?;
    let scalar_bits = fields.values().copied().sum::<usize>();
    let bit_count = scalar_bits
        .saturating_add(owners.len().saturating_mul(tokens.len().saturating_add(3)))
        .saturating_add(tokens.len().saturating_mul(2))
        .saturating_add(descriptions.len().saturating_mul(roots.len()))
        .saturating_add(locals.len().saturating_mul(locals.len().saturating_sub(1)));
    if bit_count > MAX_BITS {
        return Err("ResourceLimit: schema bits before cross-product allocation".into());
    }
    work.charge(
        owners
            .len()
            .saturating_mul(tokens.len() + 1)
            .saturating_add(descriptions.len().saturating_mul(roots.len()))
            .saturating_add(locals.len().saturating_mul(locals.len())),
    )?;
    for s in &owners {
        fields.insert(format!("s:{s}"), 2);
        fields.insert(format!("f:{s}"), 1);
        for t in &tokens {
            fields.insert(format!("i:{s}:{t}"), 1);
        }
    }
    for t in &tokens {
        fields.insert(format!("r:{t}"), 2);
    }
    for d in &descriptions {
        for r in &roots {
            fields.insert(format!("d:{d}:{r}"), 1);
        }
    }
    for a in &locals {
        for b in &locals {
            if a != b {
                fields.insert(format!("o:{a}:{b}"), 1);
            }
        }
    }
    let schema = Schema::from_fields(fields, work)?;
    // Candidate's backwards use analysis. The oracle reconstructs this separately.
    work.charge(points.len().saturating_mul(descriptions.len() + 1))?;
    let mut before = vec![BTreeSet::<String>::new(); points.len()];
    let mut after = before.clone();
    let mut changed = true;
    while changed {
        changed = false;
        for p in (0..points.len()).rev() {
            let (b, i) = points[p];
            let block = &f.blocks[b];
            let mut live = BTreeSet::new();
            let next = if i < block.instructions.len() {
                vec![p + 1]
            } else {
                match block.terminator {
                    Terminator::Jump(b) => vec![starts[b.0 as usize]],
                    Terminator::Branch {
                        then_block,
                        else_block,
                        ..
                    } => vec![starts[then_block.0 as usize], starts[else_block.0 as usize]],
                    _ => vec![],
                }
            };
            for q in next {
                work.charge(before[q].len() + 1)?;
                live.extend(before[q].iter().cloned());
            }
            work.charge(live.len() + 1)?;
            after[p] = live.clone();
            if i < block.instructions.len() {
                let ins = &block.instructions[i];
                if let Some(v) = ins.result {
                    live.remove(&format!("V{}", v.0));
                }
                match ins.kind {
                    InstructionKind::Store { local, .. }
                    | InstructionKind::StorageLive(local)
                    | InstructionKind::StorageDead(local) => {
                        live.remove(&format!("L{}", local.0));
                    }
                    InstructionKind::Load(local)
                    | InstructionKind::TakeLocal { local }
                    | InstructionKind::CallClosureShared { callee: local, .. }
                    | InstructionKind::CallClosureMut { callee: local, .. } => {
                        let k = format!("L{}", local.0);
                        if descriptions.contains(&k) {
                            live.insert(k);
                        }
                    }
                    _ => {}
                }
                let uses = inputs(&ins.kind);
                work.charge(uses.len().saturating_mul(descriptions.len() + 1))?;
                for v in uses {
                    let k = format!("V{}", v.0);
                    if descriptions.contains(&k) {
                        live.insert(k);
                    }
                }
            } else if let Terminator::Return(Some(v)) = block.terminator {
                let k = format!("V{}", v.0);
                if descriptions.contains(&k) {
                    live.insert(k);
                }
            }
            work.charge(live.len() + before[p].len() + 1)?;
            if before[p] != live {
                before[p] = live;
                changed = true;
            }
        }
    }
    Ok(Layout {
        schema,
        owners,
        descriptions,
        roots,
        tokens,
        locals,
        points,
        starts,
        after,
        labels,
    })
}
