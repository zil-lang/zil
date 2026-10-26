//! Independent raw-TypedIR explicit-state oracle. No candidate extractor,
//! instruction visitor, BDD, symbolic transfer or candidate CFG is used here.
use crate::{
    budget::{MAX_BITS, MAX_POINTS, MAX_ROWS, MAX_SLOTS, R, Work},
    schema::{QueryStatus, Relations, Schema},
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use zil_ir::*;

pub struct Reference {
    pub schema: Schema,
    pub relations: Relations,
    pub work: usize,
    pub rows: usize,
    pub labels: Vec<String>,
    pub status: QueryStatus,
}
struct Raw {
    schema: Schema,
    owners: Vec<String>,
    descs: Vec<String>,
    roots: Vec<String>,
    tokens: Vec<String>,
    locals: Vec<String>,
    points: Vec<(usize, usize)>,
    starts: Vec<usize>,
    after: Vec<BTreeSet<String>>,
    labels: Vec<String>,
}
fn affine(m: &TypedModule, t: &Type) -> bool {
    if matches!(t, Type::Managed(_)) {
        return true;
    }
    if let Type::Closure(c) = t {
        let d = &m.closures[c.0 as usize];
        return d.kind == CallableKind::FnOnce
            || d.captures.iter().any(|t| matches!(t, Type::Managed(_)));
    }
    false
}
fn desc_type(m: &TypedModule, t: &Type) -> bool {
    match t {
        Type::Desc(_) => true,
        Type::Closure(c) => m.closures[c.0 as usize]
            .captures
            .iter()
            .any(|t| matches!(t, Type::Desc(_))),
        _ => false,
    }
}
fn numeric_kind(t: &Type) -> Option<(&'static str, usize)> {
    match t {
        Type::Bool => Some(("b", 1)),
        Type::I32 | Type::I64 => Some(("n", 5)),
        Type::Enum { .. } => Some(("v", 4)),
        Type::ErrorUnion { .. } => Some(("e", 1)),
        _ => None,
    }
}
fn value_uses(k: &InstructionKind) -> Vec<u32> {
    match k {
        InstructionKind::Store { value, .. }
        | InstructionKind::WriteCapture { value, .. }
        | InstructionKind::Unary { operand: value, .. }
        | InstructionKind::ErrorTag { value }
        | InstructionKind::MakeErrorUnion { value, .. }
        | InstructionKind::ReadErrorUnion { value, .. }
        | InstructionKind::VariantTag { value }
        | InstructionKind::ReadVariant { value, .. }
        | InstructionKind::ExtractField {
            aggregate: value, ..
        } => vec![value.0],
        InstructionKind::ReadDesc { desc, .. } => vec![desc.0],
        InstructionKind::WriteDesc { desc, value, .. } => vec![desc.0, value.0],
        InstructionKind::Binary { left, right, .. } => vec![left.0, right.0],
        InstructionKind::Call { arguments, .. }
        | InstructionKind::ForeignCall { arguments, .. }
        | InstructionKind::CallClosureShared { arguments, .. }
        | InstructionKind::CallClosureMut { arguments, .. } => {
            arguments.iter().map(|v| v.0).collect()
        }
        InstructionKind::CallClosure { callee, arguments }
        | InstructionKind::CallClosureOnce { callee, arguments }
        | InstructionKind::CallIndirect { callee, arguments } => {
            arguments.iter().map(|v| v.0).chain([callee.0]).collect()
        }
        InstructionKind::MakeClosure { captures, .. } => captures.iter().map(|v| v.0).collect(),
        InstructionKind::MakeManaged { fields }
        | InstructionKind::MakeAggregate { fields }
        | InstructionKind::MakeVariant { fields, .. } => fields.iter().map(|v| v.0).collect(),
        InstructionKind::UpdateField {
            aggregate, value, ..
        } => vec![aggregate.0, value.0],
        InstructionKind::ExtractIndex { aggregate, index } => vec![aggregate.0, index.0],
        InstructionKind::UpdateIndex {
            aggregate,
            index,
            value,
        } => vec![aggregate.0, index.0, value.0],
        _ => vec![],
    }
}
fn read_slot(k: &InstructionKind, d: &str) -> bool {
    match k {
        InstructionKind::Load(l)
        | InstructionKind::TakeLocal { local: l }
        | InstructionKind::CallClosureShared { callee: l, .. }
        | InstructionKind::CallClosureMut { callee: l, .. }
            if d == format!("L{}", l.0) =>
        {
            true
        }
        _ => value_uses(k).iter().any(|v| d == format!("V{v}")),
    }
}
fn kills(ins: &Instruction, d: &str) -> bool {
    if ins.result.is_some_and(|v| d == format!("V{}", v.0)) {
        return true;
    }
    match ins.kind {
        InstructionKind::Store { local, .. }
        | InstructionKind::StorageLive(local)
        | InstructionKind::StorageDead(local) => d == format!("L{}", local.0),
        _ => false,
    }
}
fn following(f: &TypedFunction, raw: &Raw, p: usize) -> Vec<usize> {
    let (b, i) = raw.points[p];
    let block = &f.blocks[b];
    if i < block.instructions.len() {
        return vec![p + 1];
    }
    match block.terminator {
        Terminator::Jump(t) => vec![raw.starts[t.0 as usize]],
        Terminator::Branch {
            then_block,
            else_block,
            ..
        } => vec![
            raw.starts[then_block.0 as usize],
            raw.starts[else_block.0 as usize],
        ],
        _ => vec![],
    }
}
fn setup(m: &TypedModule, f: &TypedFunction, w: &mut Work) -> R<Raw> {
    if m.schema_version != 9 {
        return Err("Unsupported: oracle requires schema 9".into());
    }
    let mut slots = f.locals.len().saturating_add(f.values.len());
    if let Some(c) = f.environment {
        slots = slots.saturating_add(m.closures[c.0 as usize].captures.len());
    }
    if slots > MAX_SLOTS {
        return Err("ResourceLimit: oracle slot metadata".into());
    }
    w.charge(f.blocks.len())?;
    let mut size = 0usize;
    for bb in &f.blocks {
        size = size.saturating_add(bb.instructions.len()).saturating_add(1);
        if size > MAX_POINTS {
            return Err("ResourceLimit: oracle points".into());
        }
    }
    w.charge(size.saturating_mul(4))?;
    w.charge(
        f.locals
            .len()
            .saturating_add(f.values.len())
            .saturating_add(f.blocks.len()),
    )?;
    let mut raw = Raw {
        schema: Schema {
            fields: BTreeMap::new(),
            bits: 0,
        },
        owners: vec![],
        descs: vec![],
        roots: vec![],
        tokens: vec![],
        locals: vec![],
        points: vec![],
        starts: vec![],
        after: vec![],
        labels: vec![],
    };
    let mut fields = BTreeMap::new();
    for l in &f.locals {
        if let Type::Enum { variants, .. } = &l.ty
            && variants.len() > 15
        {
            return Err("Unsupported: oracle local enum domain".into());
        }
        let key = format!("L{}", l.id.0);
        if matches!(
            l.ty,
            Type::Address { .. } | Type::Package { .. } | Type::Unsupported(_)
        ) {
            return Err("Unsupported: oracle address/unknown type".into());
        }
        if affine(m, &l.ty) {
            raw.owners.push(key.clone());
            raw.locals.push(key.clone());
            raw.roots.push(key.clone());
        }
        if desc_type(m, &l.ty) {
            raw.descs.push(key.clone());
        }
        if let Some((p, width)) = numeric_kind(&l.ty) {
            fields.insert(format!("{p}:{key}"), width);
        }
    }
    for (i, t) in f.values.iter().enumerate() {
        if matches!(
            t,
            Type::Address { .. } | Type::Package { .. } | Type::Unsupported(_)
        ) {
            return Err("Unsupported: oracle address/unknown type".into());
        }
        let key = format!("V{i}");
        if affine(m, t) {
            raw.owners.push(key.clone());
        }
        if desc_type(m, t) {
            raw.descs.push(key.clone());
        }
        if let Type::Enum { variants, .. } = t
            && variants.len() > 15
        {
            return Err("Unsupported: oracle enum domain".into());
        }
        if let Some((p, width)) = numeric_kind(t) {
            fields.insert(format!("{p}:{key}"), width);
        }
    }
    for (i, l) in f.parameters.iter().enumerate() {
        let t = &f.locals[l.0 as usize].ty;
        if affine(m, t) {
            raw.tokens.push(format!("P{i}"));
        }
        if desc_type(m, t) {
            raw.roots.push(format!("P{i}"));
        }
    }
    if let Some(env) = f.environment {
        for (i, t) in m.closures[env.0 as usize].captures.iter().enumerate() {
            if let Type::Enum { variants, .. } = t
                && variants.len() > 15
            {
                return Err("Unsupported: oracle capture enum domain".into());
            }
            match t {
                Type::Managed(_) => {
                    raw.owners.push(format!("C{i}"));
                    raw.roots.push(format!("C{i}"));
                    raw.tokens.push(format!("C{i}"));
                }
                Type::Desc(_) => raw.roots.push(format!("B{i}")),
                _ => {
                    if let Some((p, n)) = numeric_kind(t) {
                        fields.insert(format!("{p}:C{i}"), n);
                    }
                }
            }
        }
    }
    for (b, block) in f.blocks.iter().enumerate() {
        raw.starts.push(raw.points.len());
        for (i, ins) in block.instructions.iter().enumerate() {
            let extra = match &ins.kind {
                InstructionKind::MakeClosure { captures, .. } => captures.len(),
                InstructionKind::MakeManaged { fields }
                | InstructionKind::MakeAggregate { fields }
                | InstructionKind::MakeVariant { fields, .. } => fields.len(),
                InstructionKind::Call { arguments, .. }
                | InstructionKind::CallIndirect { arguments, .. }
                | InstructionKind::ForeignCall { arguments, .. }
                | InstructionKind::CallClosure { arguments, .. }
                | InstructionKind::CallClosureOnce { arguments, .. }
                | InstructionKind::CallClosureMut { arguments, .. }
                | InstructionKind::CallClosureShared { arguments, .. } => arguments.len(),
                InstructionKind::Unsupported(_) => {
                    return Err("Unsupported: oracle instruction".into());
                }
                _ => 0,
            };
            w.charge(512usize.saturating_add(extra.saturating_mul(40)))?;
            raw.points.push((b, i));
            raw.labels.push(format!(
                "{}:{i}:{:?}:{:?}",
                block.id.0, ins.result, ins.kind
            ));
            if matches!(
                ins.kind,
                InstructionKind::Call { .. }
                    | InstructionKind::MakeClosure { .. }
                    | InstructionKind::MakeManaged { .. }
            ) && let Some(v) = ins.result
                && affine(m, &f.values[v.0 as usize])
            {
                raw.tokens.push(format!("V{}", v.0));
            }
        }
        w.charge(256)?;
        raw.points.push((b, block.instructions.len()));
        raw.labels.push(format!(
            "{}:{}:{:?}",
            block.id.0,
            block.instructions.len(),
            block.terminator
        ));
    }
    raw.owners.sort();
    raw.locals.sort();
    raw.descs.sort();
    raw.roots.sort();
    raw.roots.dedup();
    raw.tokens.sort();
    raw.tokens.dedup();
    w.charge(fields.len())?;
    let mut predicted = fields.values().copied().sum::<usize>();
    predicted = predicted.saturating_add(raw.tokens.len().saturating_mul(2));
    predicted = predicted.saturating_add(
        raw.owners
            .len()
            .saturating_mul(3usize.saturating_add(raw.tokens.len())),
    );
    predicted = predicted.saturating_add(raw.roots.len().saturating_mul(raw.descs.len()));
    predicted = predicted.saturating_add(
        raw.locals
            .len()
            .saturating_sub(1)
            .saturating_mul(raw.locals.len()),
    );
    if predicted > MAX_BITS {
        return Err("ResourceLimit: oracle coordinates before cross products".into());
    }
    w.charge(
        raw.owners
            .len()
            .saturating_mul(raw.tokens.len() + 2)
            .saturating_add(raw.descs.len().saturating_mul(raw.roots.len()))
            .saturating_add(raw.locals.len().saturating_mul(raw.locals.len())),
    )?;
    for o in &raw.owners {
        fields.insert(format!("s:{o}"), 2);
        fields.insert(format!("f:{o}"), 1);
        for t in &raw.tokens {
            fields.insert(format!("i:{o}:{t}"), 1);
        }
    }
    for t in &raw.tokens {
        fields.insert(format!("r:{t}"), 2);
    }
    for d in &raw.descs {
        for r in &raw.roots {
            fields.insert(format!("d:{d}:{r}"), 1);
        }
    }
    for a in &raw.locals {
        for b in &raw.locals {
            if a != b {
                fields.insert(format!("o:{a}:{b}"), 1);
            }
        }
    }
    raw.schema = Schema::from_fields(fields, w)?;
    w.charge(raw.points.len().saturating_mul(raw.descs.len() + 1))?;
    raw.after = vec![BTreeSet::new(); raw.points.len()];
    // Independent forward search: after p, can this coordinate be read before a
    // redefining event? It shares no backward equations or use graph with candidate.
    for p in 0..raw.points.len() {
        for d in &raw.descs {
            let mut todo = following(f, &raw, p);
            let mut visited = BTreeSet::new();
            let mut needed = false;
            while let Some(q) = todo.pop() {
                w.charge(1)?;
                if !visited.insert(q) {
                    continue;
                }
                let (b, i) = raw.points[q];
                if let Some(ins) = f.blocks[b].instructions.get(i) {
                    w.charge(value_uses(&ins.kind).len() + 1)?;
                    if read_slot(&ins.kind, d) {
                        needed = true;
                        break;
                    }
                    if kills(ins, d) {
                        continue;
                    }
                } else if let Terminator::Return(Some(v)) = f.blocks[b].terminator
                    && *d == format!("V{}", v.0)
                {
                    needed = true;
                    break;
                }
                todo.extend(following(f, &raw, q));
            }
            if needed {
                raw.after[p].insert(d.clone());
            }
        }
    }
    Ok(raw)
}

struct Eval<'a> {
    r: &'a Raw,
    pre: &'a [u64],
    out: Vec<u64>,
    faults: BTreeSet<String>,
    aborts: BTreeSet<String>,
    unknown: BTreeSet<String>,
}
impl<'a> Eval<'a> {
    fn g(&self, key: &str) -> R<u32> {
        self.r.schema.get(self.pre, key)
    }
    fn set(&mut self, key: &str, v: u32) -> R<()> {
        self.unknown.remove(key);
        self.r.schema.set(&mut self.out, key, v)
    }
    fn copy(&mut self, to: &str, from: &str) -> R<()> {
        let v = self.g(from)?;
        self.set(to, v)
    }
    fn bad(&mut self, label: &str, condition: bool) {
        if condition {
            self.faults.insert(label.into());
        }
    }
    fn alive(&mut self, key: &str) -> R<()> {
        self.bad("use-after-move", self.g(&format!("s:{key}"))? != 1);
        Ok(())
    }
    fn clear_order(&mut self, key: &str) -> R<()> {
        for b in &self.r.locals {
            if b != key {
                self.set(&format!("o:{key}:{b}"), 0)?;
                self.set(&format!("o:{b}:{key}"), 0)?;
            }
        }
        Ok(())
    }
    fn invalidate(&mut self, key: &str, p: usize) -> R<()> {
        for d in &self.r.after[p] {
            self.bad(
                "live-description-invalidation",
                self.g(&format!("d:{d}:{key}"))? != 0,
            );
        }
        Ok(())
    }
    fn description(&mut self, key: &str) -> R<()> {
        let mut any = false;
        let mut bad = false;
        for root in &self.r.roots {
            if self.g(&format!("d:{key}:{root}"))? != 0 {
                any = true;
                if self.r.owners.contains(root) && self.g(&format!("s:{root}"))? != 1 {
                    bad = true;
                }
            }
        }
        self.bad("empty-description", !any);
        self.bad("description-of-dead-root", bad);
        Ok(())
    }
    fn move_to(&mut self, to: &str, from: &str) -> R<()> {
        self.alive(from)?;
        self.bad("overwrite-live", self.g(&format!("s:{to}"))? == 1);
        self.set(&format!("s:{from}"), 2)?;
        self.set(&format!("s:{to}"), 1)?;
        self.copy(&format!("f:{to}"), &format!("f:{from}"))?;
        self.set(&format!("f:{from}"), 0)?;
        for t in &self.r.tokens {
            self.copy(&format!("i:{to}:{t}"), &format!("i:{from}:{t}"))?;
            self.set(&format!("i:{from}:{t}"), 0)?;
        }
        Ok(())
    }
    fn retire(&mut self, key: &str, class: u32, conditional: bool) -> R<()> {
        let status = self.g(&format!("s:{key}"))?;
        if !conditional {
            self.alive(key)?;
            self.set(&format!("s:{key}"), 2)?;
        } else if status == 1 {
            self.set(&format!("s:{key}"), 3)?;
        }
        self.set(&format!("f:{key}"), 0)?;
        for t in &self.r.tokens {
            if self.g(&format!("i:{key}:{t}"))? != 0 {
                self.set(&format!("r:{t}"), class)?;
            }
            self.set(&format!("i:{key}:{t}"), 0)?;
        }
        Ok(())
    }
    fn create(&mut self, key: &str, token: &str) -> R<()> {
        self.bad("overwrite-live", self.g(&format!("s:{key}"))? == 1);
        for owner in &self.r.owners {
            self.bad(
                "unsupported-overlapping-incarnation",
                self.g(&format!("i:{owner}:{token}"))? != 0,
            );
        }
        self.set(&format!("s:{key}"), 1)?;
        self.set(&format!("f:{key}"), 1)?;
        self.set(&format!("r:{token}"), 0)?;
        for t in &self.r.tokens {
            self.set(&format!("i:{key}:{t}"), u32::from(t == token))?;
        }
        Ok(())
    }
    fn roots_copy(&mut self, to: &str, from: &str) -> R<()> {
        for root in &self.r.roots {
            self.copy(&format!("d:{to}:{root}"), &format!("d:{from}:{root}"))?;
        }
        Ok(())
    }
    fn roots_one(&mut self, to: &str, root: &str) -> R<()> {
        for r in &self.r.roots {
            self.set(&format!("d:{to}:{r}"), u32::from(r == root))?;
        }
        Ok(())
    }
    fn unspecified(&mut self, key: &str, t: &Type) -> R<()> {
        if let Some((k, _)) = numeric_kind(t) {
            let name = format!("{k}:{key}");
            if k == "b" || k == "e" {
                self.unknown.insert(name);
            } else {
                self.set(&name, if k == "n" { 16 } else { 15 })?;
            }
        }
        Ok(())
    }
}

#[allow(unreachable_patterns)] // New schema instructions are never silently no-ops.
fn instruction<'a>(
    m: &TypedModule,
    f: &TypedFunction,
    raw: &'a Raw,
    p: usize,
    pre: &'a [u64],
    fresh_give: bool,
) -> R<Eval<'a>> {
    let (b, i) = raw.points[p];
    let ins = &f.blocks[b].instructions[i];
    let result = ins.result.map(|v| format!("V{}", v.0));
    let ty = ins.result.map(|v| &f.values[v.0 as usize]);
    match &ins.kind {
        InstructionKind::CallClosure { arguments, .. }
        | InstructionKind::CallClosureOnce { arguments, .. }
        | InstructionKind::CallClosureMut { arguments, .. }
        | InstructionKind::CallClosureShared { arguments, .. } => {
            for v in arguments {
                if affine(m, &f.values[v.0 as usize]) {
                    return Err("Unsupported: oracle affine closure-call arguments".into());
                }
            }
            if ty.is_some_and(|t| affine(m, t)) {
                return Err("Unsupported: oracle affine closure-call result".into());
            }
        }
        InstructionKind::Call { .. }
            if ty.is_some_and(|t| affine(m, t) && !matches!(t, Type::Managed(_))) =>
        {
            return Err("Unsupported: oracle returned closure bundle".into());
        }
        _ => {}
    }
    let mut e = Eval {
        r: raw,
        pre,
        out: pre.to_vec(),
        faults: BTreeSet::new(),
        aborts: BTreeSet::new(),
        unknown: BTreeSet::new(),
    };
    if let (Some(k), Some(t)) = (&result, ty) {
        e.unspecified(k, t)?;
    }
    for v in value_uses(&ins.kind) {
        let k = format!("V{v}");
        if raw.descs.contains(&k) {
            e.description(&k)?;
        }
    }
    match &ins.kind {
        InstructionKind::Constant(c) => {
            let key = result
                .as_ref()
                .ok_or("Unsupported: oracle constant result")?;
            match c {
                Constant::Unit => {}
                Constant::Bool(v) => e.set(&format!("b:{key}"), u32::from(*v))?,
                Constant::I32(v) => e.set(
                    &format!("n:{key}"),
                    u32::try_from(*v).ok().filter(|v| *v < 16).unwrap_or(16),
                )?,
                Constant::I64(v) => e.set(
                    &format!("n:{key}"),
                    u32::try_from(*v).ok().filter(|v| *v < 16).unwrap_or(16),
                )?,
            }
        }
        InstructionKind::MakeManaged { .. } => {
            let k = result
                .as_ref()
                .ok_or("Unsupported: oracle resource result")?;
            e.create(k, k)?;
        }
        InstructionKind::DescribeLocal { local } => {
            let k = format!("L{}", local.0);
            e.alive(&k)?;
            e.roots_one(
                result.as_ref().ok_or("Unsupported: oracle desc result")?,
                &k,
            )?;
        }
        InstructionKind::DescribeCapture { index } => {
            let k = format!("C{index}");
            e.alive(&k)?;
            e.roots_one(
                result.as_ref().ok_or("Unsupported: oracle desc result")?,
                &k,
            )?;
        }
        InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
            let s = format!("L{}", local.0);
            let d = result.as_ref().ok_or("Unsupported: oracle load result")?;
            let take = matches!(ins.kind, InstructionKind::TakeLocal { .. });
            if raw.owners.contains(&s) {
                if !take {
                    return Err("Unsupported: oracle owned Load".into());
                }
                e.invalidate(&s, p)?;
                e.move_to(d, &s)?;
                e.clear_order(&s)?;
            }
            if raw.descs.contains(&s) {
                e.description(&s)?;
                e.roots_copy(d, &s)?;
                if take {
                    for r in &raw.roots {
                        e.set(&format!("d:{s}:{r}"), 0)?;
                    }
                }
            }
            if let Some((kind, _)) = ty.and_then(numeric_kind) {
                e.copy(&format!("{kind}:{d}"), &format!("{kind}:{s}"))?;
            }
        }
        InstructionKind::Store { local, value } => {
            let to = format!("L{}", local.0);
            let from = format!("V{}", value.0);
            if raw.owners.contains(&to) {
                e.invalidate(&to, p)?;
                e.move_to(&to, &from)?;
                e.clear_order(&to)?;
                for other in &raw.locals {
                    if other != &to {
                        let live = e.g(&format!("s:{other}"))? == 1;
                        e.set(&format!("o:{other}:{to}"), u32::from(live))?;
                    }
                }
            }
            if raw.descs.contains(&to) {
                e.roots_copy(&to, &from)?;
            }
            if let Some((kind, _)) = numeric_kind(&f.locals[local.0 as usize].ty) {
                e.copy(&format!("{kind}:{to}"), &format!("{kind}:{from}"))?;
            }
        }
        InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local) => {
            let k = format!("L{}", local.0);
            if raw.owners.contains(&k) {
                e.bad("storage-with-live-debt", e.g(&format!("s:{k}"))? == 1);
                e.invalidate(&k, p)?;
                e.set(&format!("s:{k}"), 0)?;
                e.set(&format!("f:{k}"), 0)?;
                for t in &raw.tokens {
                    e.set(&format!("i:{k}:{t}"), 0)?;
                }
                e.clear_order(&k)?;
            }
            if raw.descs.contains(&k) {
                for r in &raw.roots {
                    e.set(&format!("d:{k}:{r}"), 0)?;
                }
            }
        }
        InstructionKind::DropManaged { local } | InstructionKind::DropClosure { local } => {
            let k = format!("L{}", local.0);
            e.invalidate(&k, p)?;
            e.bad("duplicate-cleanup", e.g(&format!("s:{k}"))? == 3);
            for other in &raw.locals {
                if other != &k {
                    e.bad(
                        "cleanup-order",
                        e.g(&format!("o:{k}:{other}"))? != 0 && e.g(&format!("s:{other}"))? == 1,
                    );
                }
            }
            e.retire(&k, 1, true)?;
            e.clear_order(&k)?;
        }
        InstructionKind::Unary { op, operand } => {
            if *op == UnaryOp::Not {
                let v = e.g(&format!("b:V{}", operand.0))?;
                e.set(
                    &format!("b:{}", result.as_ref().ok_or("Unsupported: oracle unary")?),
                    1 - v,
                )?;
            } else if *op == UnaryOp::Negate {
                e.aborts.insert("potential-arithmetic-trap".into());
            }
        }
        InstructionKind::Binary { op, left, right } => {
            if matches!(ty, Some(Type::Bool)) {
                let key = format!(
                    "b:{}",
                    result.as_ref().ok_or("Unsupported: oracle binary result")?
                );
                let is_bool = f.values[left.0 as usize] == Type::Bool;
                let prefix = if is_bool { "b" } else { "n" };
                let a = e.g(&format!("{prefix}:V{}", left.0))?;
                let b = e.g(&format!("{prefix}:V{}", right.0))?;
                if is_bool || a != 16 && b != 16 {
                    let value = match op {
                        BinaryOp::Equal => a == b,
                        BinaryOp::NotEqual => a != b,
                        BinaryOp::Less => a < b,
                        BinaryOp::LessEqual => a <= b,
                        BinaryOp::Greater => a > b,
                        BinaryOp::GreaterEqual => a >= b,
                        BinaryOp::And | BinaryOp::BitAnd if is_bool => a != 0 && b != 0,
                        BinaryOp::Or | BinaryOp::BitOr if is_bool => a != 0 || b != 0,
                        BinaryOp::BitXor if is_bool => a != b,
                        _ => return Err("Unsupported: oracle predicate".into()),
                    };
                    e.set(&key, u32::from(value))?;
                }
            } else if matches!(
                op,
                BinaryOp::Add
                    | BinaryOp::Subtract
                    | BinaryOp::Multiply
                    | BinaryOp::Divide
                    | BinaryOp::Remainder
                    | BinaryOp::ShiftLeft
                    | BinaryOp::ShiftRight
            ) {
                e.aborts.insert("potential-arithmetic-trap".into());
            }
        }
        InstructionKind::MakeErrorUnion { is_error, .. } => e.set(
            &format!(
                "e:{}",
                result.as_ref().ok_or("Unsupported: oracle error union")?
            ),
            u32::from(*is_error),
        )?,
        InstructionKind::ErrorTag { value } => e.copy(
            &format!("b:{}", result.as_ref().ok_or("Unsupported: oracle tag")?),
            &format!("e:V{}", value.0),
        )?,
        InstructionKind::ReadErrorUnion { value, is_error } => {
            if e.g(&format!("e:V{}", value.0))? != u32::from(*is_error) {
                e.aborts.insert("checked-error-tag-trap".into());
                e.faults.insert("projection-excludes-invalid-tag".into());
            }
        }
        InstructionKind::MakeVariant { variant, .. } => e.set(
            &format!(
                "v:{}",
                result.as_ref().ok_or("Unsupported: oracle variant")?
            ),
            *variant,
        )?,
        InstructionKind::VariantTag { value } => {
            let v = e.g(&format!("v:V{}", value.0))?;
            e.set(
                &format!(
                    "n:{}",
                    result.as_ref().ok_or("Unsupported: oracle variant tag")?
                ),
                if v == 15 { 16 } else { v },
            )?;
        }
        InstructionKind::ReadVariant { value, variant, .. } => {
            let v = e.g(&format!("v:V{}", value.0))?;
            if v != *variant {
                e.aborts.insert("checked-variant-trap".into());
                if v != 15 {
                    e.faults.insert("projection-excludes-invalid-tag".into());
                }
            }
        }
        InstructionKind::ExtractIndex { .. } | InstructionKind::UpdateIndex { .. } => {
            e.aborts.insert("potential-index-trap".into());
        }
        InstructionKind::MakeClosure { captures, .. } => {
            let d = result
                .as_ref()
                .ok_or("Unsupported: oracle closure result")?;
            if raw.owners.contains(d) {
                e.create(d, d)?;
                e.set(&format!("f:{d}"), 0)?;
                let mut moved = BTreeSet::new();
                for v in captures {
                    let from = format!("V{}", v.0);
                    if raw.owners.contains(&from) {
                        e.bad("duplicate-transfer", !moved.insert(from.clone()));
                        e.alive(&from)?;
                        e.set(&format!("s:{from}"), 2)?;
                        e.set(&format!("f:{from}"), 0)?;
                    }
                }
                for token in &raw.tokens {
                    let mut present = token == d;
                    for from in &moved {
                        present |= e.g(&format!("i:{from}:{token}"))? != 0;
                        e.set(&format!("i:{from}:{token}"), 0)?;
                    }
                    e.set(&format!("i:{d}:{token}"), u32::from(present))?;
                }
            }
            if raw.descs.contains(d) {
                for root in &raw.roots {
                    let mut present = false;
                    for v in captures {
                        let from = format!("V{}", v.0);
                        if raw.descs.contains(&from) {
                            present |= e.g(&format!("d:{from}:{root}"))? != 0;
                        }
                    }
                    e.set(&format!("d:{d}:{root}"), u32::from(present))?;
                }
            }
        }
        InstructionKind::ReadCapture { index } => {
            let env = f.environment.ok_or("Unsupported: oracle environment")?;
            let t = &m.closures[env.0 as usize].captures[*index as usize];
            let d = result
                .as_ref()
                .ok_or("Unsupported: oracle capture result")?;
            if matches!(t, Type::Desc(_)) {
                e.roots_one(d, &format!("B{index}"))?;
            } else if let Some((kind, _)) = numeric_kind(t) {
                e.copy(&format!("{kind}:{d}"), &format!("{kind}:C{index}"))?;
            }
        }
        InstructionKind::TakeCapture { index } => {
            let from = format!("C{index}");
            e.invalidate(&from, p)?;
            e.move_to(
                result.as_ref().ok_or("Unsupported: oracle take capture")?,
                &from,
            )?;
        }
        InstructionKind::WriteCapture { index, value } => {
            if let Some((kind, _)) = numeric_kind(&f.values[value.0 as usize]) {
                e.copy(&format!("{kind}:C{index}"), &format!("{kind}:V{}", value.0))?;
            }
        }
        InstructionKind::CallClosureShared { callee, .. }
        | InstructionKind::CallClosureMut { callee, .. } => {
            let k = format!("L{}", callee.0);
            if raw.owners.contains(&k) {
                e.alive(&k)?;
            }
            if raw.descs.contains(&k) {
                e.description(&k)?;
            }
        }
        InstructionKind::CallClosureOnce { callee, .. } => {
            e.retire(&format!("V{}", callee.0), 2, false)?
        }
        InstructionKind::CallClosure { callee, .. } => {
            if raw.owners.contains(&format!("V{}", callee.0)) {
                return Err("Unsupported: oracle owned generic closure call".into());
            }
        }
        InstructionKind::Call {
            function,
            arguments,
        } => {
            let own = arguments
                .iter()
                .filter(|v| affine(m, &f.values[v.0 as usize]))
                .collect::<Vec<_>>();
            let target = &m.functions[function.0 as usize];
            if target.result_mode == PassingMode::Give
                && (own.len() != 1
                    || !matches!(f.values[own[0].0 as usize], Type::Managed(_))
                    || f.values[own[0].0 as usize] != target.return_type)
            {
                return Err("Unsupported: oracle give input/result type fragment".into());
            }
            let mut seen = BTreeSet::new();
            for v in &own {
                e.bad("duplicate-transfer", !seen.insert(v.0));
                e.retire(&format!("V{}", v.0), 2, false)?;
            }
            if let Some(d) = &result
                && raw.owners.contains(d)
            {
                let give = m.functions[function.0 as usize].result_mode == PassingMode::Give;
                if give && !fresh_give {
                    let from = format!("V{}", own[0].0);
                    e.bad("overwrite-live", e.g(&format!("s:{d}"))? == 1);
                    e.set(&format!("s:{d}"), 1)?;
                    e.copy(&format!("f:{d}"), &format!("f:{from}"))?;
                    for token in &raw.tokens {
                        let present = e.g(&format!("i:{from}:{token}"))?;
                        e.set(&format!("i:{d}:{token}"), present)?;
                        if present != 0 {
                            e.set(&format!("r:{token}"), 0)?;
                        }
                    }
                } else {
                    e.create(d, d)?;
                }
            }
        }
        InstructionKind::ScopeEnter { .. }
        | InstructionKind::ScopeExit { .. }
        | InstructionKind::ReadDesc { .. }
        | InstructionKind::WriteDesc { .. }
        | InstructionKind::MakeAggregate { .. }
        | InstructionKind::ExtractField { .. }
        | InstructionKind::UpdateField { .. }
        | InstructionKind::UnsafeEnter { .. }
        | InstructionKind::UnsafeExit { .. }
        | InstructionKind::ForeignCall { .. }
        | InstructionKind::FunctionRef { .. }
        | InstructionKind::CallIndirect { .. } => {}
        InstructionKind::BorrowLocal { .. }
        | InstructionKind::BorrowLocalMut { .. }
        | InstructionKind::ReborrowAddress { .. }
        | InstructionKind::ReadAddress { .. }
        | InstructionKind::WriteAddress { .. }
        | InstructionKind::AddressField { .. }
        | InstructionKind::AddressIndex { .. }
        | InstructionKind::Unsupported(_) => {
            return Err("Unsupported: oracle address instruction".into());
        }
        _ => return Err("Unsupported: oracle instruction outside schema 8".into()),
    }
    Ok(e)
}

fn initial(m: &TypedModule, f: &TypedFunction, r: &Raw, w: &mut Work) -> R<Vec<Vec<u64>>> {
    w.charge(r.schema.bits)?;
    let pre = vec![0; r.schema.bits.div_ceil(64)];
    let mut e = Eval {
        r,
        pre: &pre,
        out: pre.clone(),
        faults: BTreeSet::new(),
        aborts: BTreeSet::new(),
        unknown: BTreeSet::new(),
    };
    let mut prior: Vec<String> = vec![];
    for (i, local) in f.parameters.iter().enumerate() {
        let key = format!("L{}", local.0);
        let t = &f.locals[local.0 as usize].ty;
        if affine(m, t) {
            e.set(&format!("s:{key}"), 1)?;
            e.set(&format!("i:{key}:P{i}"), 1)?;
            for old in &prior {
                e.set(&format!("o:{old}:{key}"), 1)?;
            }
            prior.push(key.clone());
        }
        if desc_type(m, t) {
            e.set(&format!("d:{key}:P{i}"), 1)?;
        }
        e.unspecified(&key, t)?;
    }
    if let Some(env) = f.environment {
        for (i, t) in m.closures[env.0 as usize].captures.iter().enumerate() {
            if matches!(t, Type::Managed(_)) {
                e.set(&format!("s:C{i}"), 1)?;
                e.set(&format!("i:C{i}:C{i}"), 1)?;
            } else {
                e.unspecified(&format!("C{i}"), t)?;
            }
        }
    }
    expand(e, w)
}
fn expand(e: Eval<'_>, w: &mut Work) -> R<Vec<Vec<u64>>> {
    let mut rows = vec![e.out];
    for key in e.unknown {
        if rows.len() > MAX_ROWS / 2 {
            return Err("ResourceLimit: oracle choices".into());
        }
        w.charge(rows.len().saturating_mul(e.r.schema.bits + 1))?;
        let mut next = Vec::with_capacity(rows.len() * 2);
        for mut row in rows {
            let mut other = row.clone();
            e.r.schema.set(&mut row, &key, 0)?;
            e.r.schema.set(&mut other, &key, 1)?;
            next.extend([row, other]);
        }
        rows = next;
    }
    Ok(rows)
}
fn insert(
    rel: &mut Relations,
    key: String,
    row: &[u64],
    w: &mut Work,
    total: &mut usize,
) -> R<bool> {
    w.charge(row.len() + key.len() + 1)?;
    let set = rel.entry(key).or_default();
    if set.contains(row) {
        return Ok(false);
    }
    if *total >= MAX_ROWS {
        return Err("ResourceLimit: oracle total rows".into());
    }
    *total += 1;
    set.insert(row.to_vec());
    Ok(true)
}

pub fn solve(m: &TypedModule, f: &TypedFunction, limit: usize) -> R<Reference> {
    let mut w = Work::new(limit);
    let raw = setup(m, f, &mut w)?;
    w.charge(raw.points.len().saturating_mul(32))?;
    let mut relations = Relations::new();
    for p in 0..raw.points.len() {
        relations.insert(format!("point:{p}"), BTreeSet::new());
    }
    let mut total = 0;
    let mut status = QueryStatus::Complete;
    let mut todo = VecDeque::new();
    let entry = raw.starts[f.entry.0 as usize];
    for row in initial(m, f, &raw, &mut w)? {
        insert(
            &mut relations,
            format!("point:{entry}"),
            &row,
            &mut w,
            &mut total,
        )?;
        todo.push_back((entry, row));
    }
    while let Some((p, pre)) = todo.pop_front() {
        let (block, ordinal) = raw.points[p];
        let bb = &f.blocks[block];
        if let Some(ins) = bb.instructions.get(ordinal) {
            let variants = if matches!(ins.kind,InstructionKind::Call{function,..} if m.functions[function.0 as usize].result_mode==PassingMode::Give)
            {
                2
            } else {
                1
            };
            for choice in 0..variants {
                w.charge(
                    raw.schema
                        .bits
                        .saturating_mul(value_uses(&ins.kind).len() + raw.owners.len() + 8)
                        .saturating_add(1),
                )?;
                let e = instruction(m, f, &raw, p, &pre, choice == 1)?;
                for label in &e.aborts {
                    insert(
                        &mut relations,
                        format!("abort:{p}:{label}"),
                        &pre,
                        &mut w,
                        &mut total,
                    )?;
                }
                for label in &e.faults {
                    if label == "unsupported-overlapping-incarnation" {
                        status = QueryStatus::UnsupportedMultiplicity;
                        insert(
                            &mut relations,
                            format!("unsupported:{p}:overlapping-incarnation"),
                            &pre,
                            &mut w,
                            &mut total,
                        )?;
                    } else if label != "projection-excludes-invalid-tag" {
                        insert(
                            &mut relations,
                            format!("fault:{p}:{label}"),
                            &pre,
                            &mut w,
                            &mut total,
                        )?;
                    }
                }
                if e.faults.is_empty() {
                    for row in expand(e, &mut w)? {
                        if insert(
                            &mut relations,
                            format!("point:{}", p + 1),
                            &row,
                            &mut w,
                            &mut total,
                        )? {
                            w.charge(row.len())?;
                            todo.push_back((p + 1, row));
                        }
                    }
                }
            }
        } else {
            w.charge(
                raw.schema
                    .bits
                    .saturating_mul(raw.owners.len() + 8)
                    .saturating_add(1),
            )?;
            let mut e = Eval {
                r: &raw,
                pre: &pre,
                out: pre.clone(),
                faults: BTreeSet::new(),
                aborts: BTreeSet::new(),
                unknown: BTreeSet::new(),
            };
            let mut next = vec![];
            let exit = match bb.terminator {
                Terminator::Jump(b) => {
                    next.push(raw.starts[b.0 as usize]);
                    None
                }
                Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                } => {
                    let b = if e.g(&format!("b:V{}", condition.0))? != 0 {
                        then_block
                    } else {
                        else_block
                    };
                    next.push(raw.starts[b.0 as usize]);
                    None
                }
                Terminator::Trap => Some("abort"),
                Terminator::Unreachable => {
                    e.faults.insert("reachable-unreachable".into());
                    Some("unreachable")
                }
                Terminator::Return(value) => {
                    let returned = value.map(|v| format!("V{}", v.0));
                    if let Some(k) = &returned
                        && raw.owners.contains(k)
                    {
                        if f.result_mode == PassingMode::Create {
                            e.bad("nonfresh-create-return", e.g(&format!("f:{k}"))? == 0);
                        }
                        e.retire(k, 3, false)?;
                    }
                    for k in &raw.owners {
                        if !k.starts_with('C') && Some(k) != returned.as_ref() {
                            e.bad("normal-exit-debt", e.g(&format!("s:{k}"))? == 1);
                        }
                    }
                    if let Some(env) = f.environment
                        && m.closures[env.0 as usize].kind != CallableKind::FnOnce
                    {
                        for k in raw.owners.iter().filter(|k| k.starts_with('C')) {
                            e.bad(
                                "repeatable-environment-consumed",
                                e.g(&format!("s:{k}"))? != 1,
                            );
                        }
                    }
                    if let Some(v) = value
                        && matches!(f.return_type, Type::ErrorUnion { .. })
                    {
                        Some(if e.g(&format!("e:V{}", v.0))? == 0 {
                            "success-return"
                        } else {
                            "error-return"
                        })
                    } else {
                        Some("return")
                    }
                }
            };
            for label in &e.faults {
                insert(
                    &mut relations,
                    format!("fault:{p}:{label}"),
                    &pre,
                    &mut w,
                    &mut total,
                )?;
            }
            if e.faults.is_empty() {
                if let Some(label) = exit {
                    insert(
                        &mut relations,
                        format!("exit:{p}:{label}"),
                        &e.out,
                        &mut w,
                        &mut total,
                    )?;
                }
                for q in next {
                    if insert(
                        &mut relations,
                        format!("point:{q}"),
                        &e.out,
                        &mut w,
                        &mut total,
                    )? {
                        w.charge(e.out.len())?;
                        todo.push_back((q, e.out.clone()));
                    }
                }
            }
        }
    }
    Ok(Reference {
        schema: raw.schema,
        relations,
        work: w.used,
        rows: total,
        labels: raw.labels,
        status,
    })
}
