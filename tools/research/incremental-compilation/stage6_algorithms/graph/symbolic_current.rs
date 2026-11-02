use crate::{
    bdd::{Bdd, Id, Method},
    budget::{MAX_RELATIONS, R},
    encoding::{Encoding, Representation},
    extract::{self, Layout},
    schema::{QueryStatus, Schema},
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use zil_ir::*;

pub struct Transfer {
    pub invariant_violation: Id,
    pub faults: Vec<(String, Id)>,
    pub aborts: Vec<(String, Id)>,
    pub factors: Vec<(usize, Id)>,
    pub assignments: crate::encoding::Assignments,
}
pub struct Event {
    pub transfers: Vec<Transfer>,
    pub next: Vec<(usize, Id)>,
    pub terminal: Vec<(String, Id)>,
}
pub struct Solution {
    pub encoding: Encoding,
    pub layout: Layout,
    pub bdd: Bdd,
    pub relations: BTreeMap<String, Id>,
    pub explicit: Option<crate::schema::Relations>,
    pub events: usize,
    pub status: QueryStatus,
    pub phase_ns: [u128; 4], // extraction, event construction, planning, fixed point
}
struct Build<'a> {
    direct: bool,
    s: &'a Schema,
    l: &'a Layout,
    e: &'a Encoding,
    b: &'a mut Bdd,
    changes: BTreeMap<usize, (Id, Id)>,
    faults: Vec<(String, Id)>,
    aborts: Vec<(String, Id)>,
}
impl<'a> Build<'a> {
    fn new(l: &'a Layout, e: &'a Encoding, b: &'a mut Bdd) -> Self {
        Self {
            direct: false,
            s: &l.schema,
            l,
            e,
            b,
            changes: BTreeMap::new(),
            faults: vec![],
            aborts: vec![],
        }
    }
    fn bits(&mut self, key: &str) -> R<Vec<Id>> {
        let (at, n) = self.s.field(key)?;
        self.b.work.charge(n)?;
        (0..n).map(|i| self.e.read(at + i, self.b)).collect()
    }
    fn changed_bits(&mut self, key: &str) -> R<Vec<Id>> {
        let (at, n) = self.s.field(key)?;
        let mut result = Vec::new();
        for i in 0..n {
            let x = if let Some((rhs, 0)) = self.changes.get(&(at + i)) {
                *rhs
            } else {
                self.e.read(at + i, self.b)?
            };
            result.push(x);
        }
        Ok(result)
    }
    fn bit(&mut self, key: &str) -> R<Id> {
        Ok(self.bits(key)?[0])
    }
    fn eq(&mut self, key: &str, value: u32) -> R<Id> {
        let bits = self.bits(key)?;
        let mut r = 1;
        for (i, x) in bits.into_iter().enumerate() {
            let v = if value >> i & 1 == 1 {
                x
            } else {
                self.b.not(x)?
            };
            r = self.b.and(r, v)?;
        }
        Ok(r)
    }
    fn set(&mut self, key: &str, value: u32) -> R<()> {
        let (at, n) = self.s.field(key)?;
        self.b.work.charge(n)?;
        for i in 0..n {
            self.changes.insert(at + i, ((value >> i) & 1, 0));
        }
        Ok(())
    }
    fn put(&mut self, key: &str, expr: Id) -> R<()> {
        let (at, n) = self.s.field(key)?;
        if n != 1 {
            return Err("InternalError: single-bit output".into());
        }
        self.changes.insert(at, (expr, 0));
        Ok(())
    }
    fn copy(&mut self, dst: &str, src: &str) -> R<()> {
        let (at, n) = self.s.field(dst)?;
        let bits = self.bits(src)?;
        if bits.len() != n {
            return Err("Unsupported: field copy width".into());
        }
        for (i, x) in bits.into_iter().enumerate() {
            self.changes.insert(at + i, (x, 0));
        }
        Ok(())
    }
    fn unknown(&mut self, key: &str) -> R<()> {
        let (at, n) = self.s.field(key)?;
        for i in 0..n {
            self.changes.insert(at + i, (0, 1));
        }
        Ok(())
    }
    fn require(&mut self, label: &str, predicate: Id) -> R<()> {
        let bad = self.b.not(predicate)?;
        self.faults.push((label.into(), bad));
        Ok(())
    }
    fn live(&mut self, k: &str) -> R<Id> {
        self.eq(&format!("s:{k}"), 1)
    }
    fn require_live(&mut self, k: &str) -> R<()> {
        let p = self.live(k)?;
        self.require("use-after-move", p)
    }
    fn empty_destination(&mut self, k: &str) -> R<()> {
        let p = self.live(k)?;
        self.faults.push(("overwrite-live".into(), p));
        Ok(())
    }
    fn forget_order(&mut self, k: &str) -> R<()> {
        for other in &self.l.locals {
            if other != k {
                self.set(&format!("o:{k}:{other}"), 0)?;
                self.set(&format!("o:{other}:{k}"), 0)?;
            }
        }
        Ok(())
    }
    fn invalidate(&mut self, root: &str, after: &BTreeSet<String>) -> R<()> {
        for d in after {
            let p = self.bit(&format!("d:{d}:{root}"))?;
            self.faults
                .push(("live-description-invalidation".into(), p));
        }
        Ok(())
    }
    fn use_description(&mut self, d: &str) -> R<()> {
        let mut any = 0;
        let mut invalid = 0;
        for root in &self.l.roots {
            let v = self.bit(&format!("d:{d}:{root}"))?;
            any = self.b.or(any, v)?;
            if self.l.owners.contains(root) {
                let live = self.live(root)?;
                let dead = self.b.not(live)?;
                let bad = self.b.and(v, dead)?;
                invalid = self.b.or(invalid, bad)?;
            }
        }
        self.require("empty-description", any)?;
        self.faults
            .push(("description-of-dead-root".into(), invalid));
        Ok(())
    }
    fn move_owner(&mut self, dst: &str, src: &str) -> R<()> {
        self.require_live(src)?;
        self.empty_destination(dst)?;
        self.set(&format!("s:{src}"), 2)?;
        self.set(&format!("s:{dst}"), 1)?;
        self.copy(&format!("f:{dst}"), &format!("f:{src}"))?;
        self.set(&format!("f:{src}"), 0)?;
        for t in &self.l.tokens {
            self.copy(&format!("i:{dst}:{t}"), &format!("i:{src}:{t}"))?;
            self.set(&format!("i:{src}:{t}"), 0)?;
        }
        Ok(())
    }
    fn retire(&mut self, src: &str, class: u32, conditional: bool) -> R<()> {
        let live = self.live(src)?;
        if !conditional {
            self.require_live(src)?;
            self.set(&format!("s:{src}"), 2)?;
        } else {
            let bits = self.bits(&format!("s:{src}"))?;
            let (at, _) = self.s.field(&format!("s:{src}"))?;
            for (i, bit) in bits.into_iter().enumerate() {
                let x = self.b.choose(live, 1, bit)?;
                self.changes.insert(at + i, (x, 0));
            }
        }
        self.set(&format!("f:{src}"), 0)?;
        for t in &self.l.tokens {
            let present = self.bit(&format!("i:{src}:{t}"))?;
            let old = self.changed_bits(&format!("r:{t}"))?;
            let (at, _) = self.s.field(&format!("r:{t}"))?;
            for (i, bit) in old.into_iter().enumerate() {
                let x = self.b.choose(present, (class >> i) & 1, bit)?;
                self.changes.insert(at + i, (x, 0));
            }
            self.set(&format!("i:{src}:{t}"), 0)?;
        }
        Ok(())
    }
    fn manufacture(&mut self, dst: &str, token: &str) -> R<()> {
        self.empty_destination(dst)?;
        let mut overlap = 0;
        for s in &self.l.owners {
            let x = self.bit(&format!("i:{s}:{token}"))?;
            overlap = self.b.or(overlap, x)?;
        }
        self.faults
            .push(("unsupported-overlapping-incarnation".into(), overlap));
        self.set(&format!("s:{dst}"), 1)?;
        self.set(&format!("f:{dst}"), 1)?;
        self.set(&format!("r:{token}"), 0)?;
        for t in &self.l.tokens {
            self.set(&format!("i:{dst}:{t}"), u32::from(t == token))?;
        }
        Ok(())
    }
    fn copy_description(&mut self, dst: &str, src: &str) -> R<()> {
        for r in &self.l.roots {
            self.copy(&format!("d:{dst}:{r}"), &format!("d:{src}:{r}"))?;
        }
        Ok(())
    }
    fn root_description(&mut self, dst: &str, root: &str) -> R<()> {
        for r in &self.l.roots {
            self.set(&format!("d:{dst}:{r}"), u32::from(r == root))?;
        }
        Ok(())
    }
    fn scalar_default(&mut self, key: &str, t: &Type) -> R<()> {
        if let Some((p, _)) = extract::scalar_field(t) {
            let k = format!("{p}:{key}");
            match p {
                "b" | "e" => self.unknown(&k)?,
                "n" => self.set(&k, 16)?,
                "v" => self.set(&k, 15)?,
                _ => unreachable!(),
            }
        }
        Ok(())
    }
    fn finish(self) -> R<Transfer> {
        let (factors, assignments, invariant_violation) = if self.direct {
            let (assignments, invalid) = self.e.assignments(self.changes, self.b)?;
            (vec![], assignments, invalid)
        } else {
            let (factors, invalid) = self.e.equations(self.changes, self.b)?;
            (factors, vec![], invalid)
        };
        Ok(Transfer {
            invariant_violation,
            faults: self.faults,
            aborts: self.aborts,
            factors,
            assignments,
        })
    }
}

#[allow(unreachable_patterns)] // A later schema must reject, not acquire guessed semantics.
#[allow(clippy::too_many_arguments)]
pub(crate) fn compile_ins(
    m: &TypedModule,
    f: &TypedFunction,
    l: &Layout,
    e: &Encoding,
    b: &mut Bdd,
    p: usize,
    fresh_give: bool,
    direct: bool,
) -> R<Transfer> {
    let (block, ordinal) = l.points[p];
    let ins = &f.blocks[block].instructions[ordinal];
    let dst = ins.result.map(|v| format!("V{}", v.0));
    let ty = ins.result.map(|v| &f.values[v.0 as usize]);
    if let InstructionKind::CallClosure { arguments, .. }
    | InstructionKind::CallClosureShared { arguments, .. }
    | InstructionKind::CallClosureMut { arguments, .. }
    | InstructionKind::CallClosureOnce { arguments, .. } = &ins.kind
        && (arguments
            .iter()
            .any(|v| extract::owned(m, &f.values[v.0 as usize]))
            || ty.is_some_and(|t| extract::owned(m, t)))
    {
        return Err("Unsupported: affine closure-call arguments/results".into());
    }
    let mut c = Build::new(l, e, b);
    c.direct = direct;
    if let (Some(dst), Some(ty)) = (&dst, ty) {
        c.scalar_default(dst, ty)?;
    }
    c.b.work.charge(
        extract::inputs(&ins.kind)
            .len()
            .saturating_mul(l.descriptions.len() + 1),
    )?;
    for v in extract::inputs(&ins.kind) {
        let k = format!("V{}", v.0);
        if l.descriptions.contains(&k) {
            c.use_description(&k)?;
        }
    }
    match &ins.kind {
        InstructionKind::ScopeEnter { .. }
        | InstructionKind::ScopeExit { .. }
        | InstructionKind::UnsafeEnter { .. }
        | InstructionKind::UnsafeExit { .. }
        | InstructionKind::FunctionRef { .. } => {}
        InstructionKind::Constant(v) => {
            let d = dst.as_ref().ok_or("Unsupported: constant without result")?;
            match v {
                Constant::Bool(v) => c.set(&format!("b:{d}"), u32::from(*v))?,
                Constant::I32(v) => c.set(
                    &format!("n:{d}"),
                    if (0..16).contains(v) { *v as u32 } else { 16 },
                )?,
                Constant::I64(v) => c.set(
                    &format!("n:{d}"),
                    if (0..16).contains(v) { *v as u32 } else { 16 },
                )?,
                Constant::Unit => {}
                _ => return Err("Unsupported: post-schema10 constant".into()),
            }
        }
        InstructionKind::MakeManaged { .. } => {
            let d = dst.as_ref().ok_or("Unsupported: manufacture result")?;
            c.manufacture(d, d)?;
        }
        InstructionKind::DescribeLocal { local } => {
            let root = format!("L{}", local.0);
            c.require_live(&root)?;
            c.root_description(
                dst.as_ref().ok_or("Unsupported: description result")?,
                &root,
            )?;
        }
        InstructionKind::DescribeCapture { index } => {
            let root = format!("C{index}");
            c.require_live(&root)?;
            c.root_description(
                dst.as_ref().ok_or("Unsupported: description result")?,
                &root,
            )?;
        }
        InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
            let s = format!("L{}", local.0);
            let d = dst.as_ref().ok_or("Unsupported: load result")?;
            let take = matches!(ins.kind, InstructionKind::TakeLocal { .. });
            if l.owners.contains(&s) {
                if !take {
                    return Err("Unsupported: raw copying owned local".into());
                }
                c.invalidate(&s, &l.after[p])?;
                c.move_owner(d, &s)?;
                c.forget_order(&s)?;
            }
            if l.descriptions.contains(&s) {
                c.use_description(&s)?;
                c.copy_description(d, &s)?;
                if take {
                    for r in &l.roots {
                        c.set(&format!("d:{s}:{r}"), 0)?;
                    }
                }
            }
            if let Some((prefix, _)) = ty.and_then(extract::scalar_field) {
                c.copy(&format!("{prefix}:{d}"), &format!("{prefix}:{s}"))?;
            }
        }
        InstructionKind::Store { local, value } => {
            let s = format!("V{}", value.0);
            let d = format!("L{}", local.0);
            if l.owners.contains(&d) {
                c.invalidate(&d, &l.after[p])?;
                c.move_owner(&d, &s)?;
                c.forget_order(&d)?;
                for other in &l.locals {
                    if other != &d {
                        let live = c.live(other)?;
                        c.put(&format!("o:{other}:{d}"), live)?;
                    }
                }
            }
            if l.descriptions.contains(&d) {
                c.copy_description(&d, &s)?;
            }
            if let Some((prefix, _)) = extract::scalar_field(&f.locals[local.0 as usize].ty) {
                c.copy(&format!("{prefix}:{d}"), &format!("{prefix}:{s}"))?;
            }
        }
        InstructionKind::StorageLive(local) | InstructionKind::StorageDead(local) => {
            let s = format!("L{}", local.0);
            if l.owners.contains(&s) {
                let bad = c.live(&s)?;
                c.faults.push(("storage-with-live-debt".into(), bad));
                c.invalidate(&s, &l.after[p])?;
                c.set(&format!("s:{s}"), 0)?;
                c.set(&format!("f:{s}"), 0)?;
                for t in &l.tokens {
                    c.set(&format!("i:{s}:{t}"), 0)?;
                }
                c.forget_order(&s)?;
            }
            if l.descriptions.contains(&s) {
                for r in &l.roots {
                    c.set(&format!("d:{s}:{r}"), 0)?;
                }
            }
        }
        InstructionKind::DropManaged { local } | InstructionKind::DropClosure { local } => {
            let s = format!("L{}", local.0);
            c.invalidate(&s, &l.after[p])?;
            let done = c.eq(&format!("s:{s}"), 3)?;
            c.faults.push(("duplicate-cleanup".into(), done));
            for other in &l.locals {
                if other != &s {
                    let edge = c.bit(&format!("o:{s}:{other}"))?;
                    let live = c.live(other)?;
                    let bad = c.b.and(edge, live)?;
                    c.faults.push(("cleanup-order".into(), bad));
                }
            }
            c.retire(&s, 1, true)?;
            c.forget_order(&s)?;
        }
        InstructionKind::ReadDesc { .. } | InstructionKind::WriteDesc { .. } => {}
        InstructionKind::Unary {
            op: UnaryOp::Not,
            operand,
        } => {
            let s = c.bit(&format!("b:V{}", operand.0))?;
            let x = c.b.not(s)?;
            c.put(
                &format!("b:{}", dst.as_ref().ok_or("Unsupported: unary result")?),
                x,
            )?;
        }
        InstructionKind::Unary {
            op: UnaryOp::Negate,
            ..
        } => {
            c.aborts.push(("potential-arithmetic-trap".into(), 1));
        }
        InstructionKind::Unary { .. } => {}
        InstructionKind::Binary { op, left, right } => {
            if matches!(ty, Some(Type::Bool)) {
                let d = format!("b:{}", dst.as_ref().ok_or("Unsupported: binary result")?);
                let mut free = 0;
                let result = if f.values[left.0 as usize] == Type::Bool {
                    let a = c.bit(&format!("b:V{}", left.0))?;
                    let b = c.bit(&format!("b:V{}", right.0))?;
                    match op {
                        BinaryOp::And | BinaryOp::BitAnd => c.b.and(a, b)?,
                        BinaryOp::Or | BinaryOp::BitOr => c.b.or(a, b)?,
                        BinaryOp::Equal => c.b.equal(a, b)?,
                        BinaryOp::NotEqual | BinaryOp::BitXor => c.b.xor(a, b)?,
                        _ => return Err("Unsupported: boolean binary".into()),
                    }
                } else {
                    let a = c.bits(&format!("n:V{}", left.0))?;
                    let b = c.bits(&format!("n:V{}", right.0))?;
                    free = c.b.or(a[4], b[4])?;
                    let mut eq = 1;
                    let mut lt = 0;
                    for i in (0..4).rev() {
                        let na = c.b.not(a[i])?;
                        let ab = c.b.and(na, b[i])?;
                        let x = c.b.and(eq, ab)?;
                        lt = c.b.or(lt, x)?;
                        let same = c.b.equal(a[i], b[i])?;
                        eq = c.b.and(eq, same)?;
                    }
                    match op {
                        BinaryOp::Equal => eq,
                        BinaryOp::NotEqual => c.b.not(eq)?,
                        BinaryOp::Less => lt,
                        BinaryOp::LessEqual => c.b.or(lt, eq)?,
                        BinaryOp::Greater => {
                            let x = c.b.or(lt, eq)?;
                            c.b.not(x)?
                        }
                        BinaryOp::GreaterEqual => c.b.not(lt)?,
                        _ => return Err("Unsupported: numeric predicate".into()),
                    }
                };
                let (at, _) = c.s.field(&d)?;
                c.changes.insert(at, (result, free));
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
                c.aborts.push(("potential-arithmetic-trap".into(), 1));
            }
        }
        InstructionKind::MakeErrorUnion { is_error, .. } => {
            c.set(
                &format!("e:{}", dst.as_ref().ok_or("Unsupported: error result")?),
                u32::from(*is_error),
            )?;
        }
        InstructionKind::ErrorTag { value } => {
            c.copy(
                &format!("b:{}", dst.as_ref().ok_or("Unsupported: tag result")?),
                &format!("e:V{}", value.0),
            )?;
        }
        InstructionKind::ReadErrorUnion { value, is_error } => {
            let valid = c.eq(&format!("e:V{}", value.0), u32::from(*is_error))?;
            let invalid = c.b.not(valid)?;
            c.aborts.push(("checked-error-tag-trap".into(), invalid));
            c.require("projection-excludes-invalid-tag", valid)?;
        }
        InstructionKind::MakeVariant { variant, .. } => {
            c.set(
                &format!("v:{}", dst.as_ref().ok_or("Unsupported: variant result")?),
                *variant,
            )?;
        }
        InstructionKind::VariantTag { value } => {
            let s = c.bits(&format!("v:V{}", value.0))?;
            let unknown = c.eq(&format!("v:V{}", value.0), 15)?;
            let (at, _) = c.s.field(&format!(
                "n:{}",
                dst.as_ref().ok_or("Unsupported: variant tag")?
            ))?;
            for (i, x) in s.into_iter().enumerate() {
                let bit = c.b.choose(unknown, 0, x)?;
                c.changes.insert(at + i, (bit, 0));
            }
            c.changes.insert(at + 4, (unknown, 0));
        }
        InstructionKind::ReadVariant { value, variant, .. } => {
            let valid = c.eq(&format!("v:V{}", value.0), *variant)?;
            let unknown = c.eq(&format!("v:V{}", value.0), 15)?;
            let can = c.b.or(valid, unknown)?;
            let invalid = c.b.not(valid)?;
            c.aborts.push(("checked-variant-trap".into(), invalid));
            c.require("projection-excludes-invalid-tag", can)?;
        }
        InstructionKind::MakeAggregate { .. }
        | InstructionKind::ExtractField { .. }
        | InstructionKind::UpdateField { .. } => {}
        InstructionKind::ExtractIndex { .. } | InstructionKind::UpdateIndex { .. } => {
            c.aborts.push(("potential-index-trap".into(), 1));
        }
        InstructionKind::MakeClosure { captures, .. } => {
            let d = dst.as_ref().ok_or("Unsupported: closure result")?;
            if l.owners.contains(d) {
                c.manufacture(d, d)?;
                c.set(&format!("f:{d}"), 0)?;
                let mut seen = BTreeSet::new();
                for v in captures {
                    let s = format!("V{}", v.0);
                    if l.owners.contains(&s) {
                        if !seen.insert(s.clone()) {
                            c.faults.push(("duplicate-transfer".into(), 1));
                        }
                        c.require_live(&s)?;
                        c.set(&format!("s:{s}"), 2)?;
                        c.set(&format!("f:{s}"), 0)?;
                    }
                }
                for t in &l.tokens {
                    let mut ident = u32::from(t == d);
                    for s in &seen {
                        let x = c.bit(&format!("i:{s}:{t}"))?;
                        ident = c.b.or(ident, x)?;
                        c.set(&format!("i:{s}:{t}"), 0)?;
                    }
                    c.put(&format!("i:{d}:{t}"), ident)?;
                }
            }
            if l.descriptions.contains(d) {
                for r in &l.roots {
                    let mut bits = 0;
                    for v in captures {
                        let s = format!("V{}", v.0);
                        if l.descriptions.contains(&s) {
                            let x = c.bit(&format!("d:{s}:{r}"))?;
                            bits = c.b.or(bits, x)?;
                        }
                    }
                    c.put(&format!("d:{d}:{r}"), bits)?;
                }
            }
        }
        InstructionKind::ReadCapture { index } => {
            let env = f
                .environment
                .ok_or("Unsupported: capture without environment")?;
            let t = &m.closures[env.0 as usize].captures[*index as usize];
            let d = dst.as_ref().ok_or("Unsupported: capture result")?;
            if matches!(t, Type::Desc(_)) {
                c.root_description(d, &format!("B{index}"))?;
            } else if let Some((prefix, _)) = extract::scalar_field(t) {
                c.copy(&format!("{prefix}:{d}"), &format!("{prefix}:C{index}"))?;
            }
        }
        InstructionKind::TakeCapture { index } => {
            let s = format!("C{index}");
            let d = dst.as_ref().ok_or("Unsupported: take capture result")?;
            c.invalidate(&s, &l.after[p])?;
            c.move_owner(d, &s)?;
        }
        InstructionKind::WriteCapture { index, value } => {
            let t = &f.values[value.0 as usize];
            if let Some((prefix, _)) = extract::scalar_field(t) {
                c.copy(
                    &format!("{prefix}:C{index}"),
                    &format!("{prefix}:V{}", value.0),
                )?;
            }
        }
        InstructionKind::CallClosureShared { callee, .. }
        | InstructionKind::CallClosureMut { callee, .. } => {
            let s = format!("L{}", callee.0);
            if l.owners.contains(&s) {
                c.require_live(&s)?;
            }
            if l.descriptions.contains(&s) {
                c.use_description(&s)?;
            }
        }
        InstructionKind::CallClosureOnce { callee, .. } => {
            c.retire(&format!("V{}", callee.0), 2, false)?;
        }
        InstructionKind::CallClosure { callee, .. } => {
            let s = format!("V{}", callee.0);
            if l.owners.contains(&s) {
                return Err("Unsupported: owned ordinary closure call".into());
            }
        }
        InstructionKind::Call {
            function,
            arguments,
        } => {
            let callee = &m.functions[function.0 as usize];
            if ty.is_some_and(|t| extract::owned(m, t) && !matches!(t, Type::Managed(_))) {
                return Err("Unsupported: ordinary call returns owned closure bundle".into());
            }
            let owned = arguments
                .iter()
                .filter(|v| extract::owned(m, &f.values[v.0 as usize]))
                .map(|v| format!("V{}", v.0))
                .collect::<Vec<_>>();
            if callee.result_mode == PassingMode::Give {
                let input_types = arguments
                    .iter()
                    .filter(|v| extract::owned(m, &f.values[v.0 as usize]))
                    .map(|v| &f.values[v.0 as usize])
                    .collect::<Vec<_>>();
                if input_types.len() != 1
                    || !matches!(input_types[0], Type::Managed(_))
                    || *input_types[0] != callee.return_type
                {
                    return Err("Unsupported: give result requires one Managed input of identical nominal/payload type".into());
                }
            }
            let set = owned.iter().collect::<BTreeSet<_>>();
            if set.len() != owned.len() {
                c.faults.push(("duplicate-transfer".into(), 1));
            }
            for src in &owned {
                c.retire(src, 2, false)?;
            }
            if let Some(d) = &dst
                && l.owners.contains(d)
            {
                if callee.result_mode == PassingMode::Give && !fresh_give {
                    let src = &owned[0];
                    c.empty_destination(d)?;
                    c.set(&format!("s:{d}"), 1)?;
                    c.copy(&format!("f:{d}"), &format!("f:{src}"))?;
                    for t in &l.tokens {
                        let present = c.bit(&format!("i:{src}:{t}"))?;
                        c.put(&format!("i:{d}:{t}"), present)?;
                        let old = c.changed_bits(&format!("r:{t}"))?;
                        let (at, _) = c.s.field(&format!("r:{t}"))?;
                        for (i, bit) in old.into_iter().enumerate() {
                            let x = c.b.choose(present, 0, bit)?;
                            c.changes.insert(at + i, (x, 0));
                        }
                    }
                } else {
                    c.manufacture(d, d)?;
                }
            }
        }
        InstructionKind::ForeignCall { .. } | InstructionKind::CallIndirect { .. } => {}
        InstructionKind::BorrowLocal { .. }
        | InstructionKind::BorrowLocalMut { .. }
        | InstructionKind::ReborrowAddress { .. }
        | InstructionKind::ReadAddress { .. }
        | InstructionKind::WriteAddress { .. }
        | InstructionKind::AddressField { .. }
        | InstructionKind::AddressIndex { .. }
        | InstructionKind::Unsupported(_) => {
            return Err("Unsupported: address/unknown instruction".into());
        }
        _ => return Err("Unsupported: instruction outside frozen schema 8".into()),
    }
    c.finish()
}

pub(crate) fn initial(
    m: &TypedModule,
    f: &TypedFunction,
    l: &Layout,
    e: &Encoding,
    b: &mut Bdd,
) -> R<Id> {
    let c = Build::new(l, e, b);
    c.b.work.charge(l.schema.bits.saturating_mul(2))?;
    let mut values = BTreeMap::<usize, Option<bool>>::new();
    for bit in 0..l.schema.bits {
        values.insert(bit, Some(false));
    }
    let mut set = |key: &str, v: Option<u32>| -> R<()> {
        let (at, n) = l.schema.field(key)?;
        for i in 0..n {
            values.insert(at + i, v.map(|v| v >> i & 1 != 0));
        }
        Ok(())
    };
    let mut prior = vec![];
    for (i, local) in f.parameters.iter().enumerate() {
        let k = format!("L{}", local.0);
        let t = &f.locals[local.0 as usize].ty;
        if extract::owned(m, t) {
            set(&format!("s:{k}"), Some(1))?;
            set(&format!("i:{k}:P{i}"), Some(1))?;
            for earlier in &prior {
                set(&format!("o:{earlier}:{k}"), Some(1))?;
            }
            prior.push(k.clone());
        }
        if extract::described(m, t) {
            set(&format!("d:{k}:P{i}"), Some(1))?;
        }
        if let Some((p, _)) = extract::scalar_field(t) {
            set(
                &format!("{p}:{k}"),
                match p {
                    "n" => Some(16),
                    "v" => Some(15),
                    _ => None,
                },
            )?;
        }
    }
    if let Some(env) = f.environment {
        for (i, t) in m.closures[env.0 as usize].captures.iter().enumerate() {
            if matches!(t, Type::Managed(_)) {
                set(&format!("s:C{i}"), Some(1))?;
                set(&format!("i:C{i}:C{i}"), Some(1))?;
            } else if let Some((p, _)) = extract::scalar_field(t) {
                set(
                    &format!("{p}:C{i}"),
                    match p {
                        "n" => Some(16),
                        "v" => Some(15),
                        _ => None,
                    },
                )?;
            }
        }
    }
    e.initial(&values, c.b)
}

type TerminalTransfer = (Transfer, Vec<(String, Id)>, Vec<(usize, Id)>);
pub(crate) fn terminal(
    m: &TypedModule,
    f: &TypedFunction,
    l: &Layout,
    e: &Encoding,
    b: &mut Bdd,
    p: usize,
    direct: bool,
) -> R<TerminalTransfer> {
    let (block, _) = l.points[p];
    let mut c = Build::new(l, e, b);
    c.direct = direct;
    let mut next = vec![];
    let terminal = match f.blocks[block].terminator {
        Terminator::Jump(t) => {
            next.push((l.starts[t.0 as usize], 1));
            vec![]
        }
        Terminator::Branch {
            condition,
            then_block,
            else_block,
        } => {
            let yes = c.bit(&format!("b:V{}", condition.0))?;
            let no = c.b.not(yes)?;
            next.push((l.starts[then_block.0 as usize], yes));
            next.push((l.starts[else_block.0 as usize], no));
            vec![]
        }
        Terminator::Trap => vec![("abort".into(), 1)],
        Terminator::Unreachable => {
            c.faults.push(("reachable-unreachable".into(), 1));
            vec![("unreachable".into(), 1)]
        }
        Terminator::Return(value) => {
            let returned = value.map(|v| format!("V{}", v.0));
            if let Some(k) = &returned
                && l.owners.contains(k)
            {
                if f.result_mode == PassingMode::Create {
                    let fresh = c.bit(&format!("f:{k}"))?;
                    c.require("nonfresh-create-return", fresh)?;
                }
                c.retire(k, 3, false)?;
            }
            for k in &l.owners {
                if !k.starts_with('C') && Some(k) != returned.as_ref() {
                    let bad = c.live(k)?;
                    c.faults.push(("normal-exit-debt".into(), bad));
                }
            }
            if let Some(env) = f.environment
                && m.closures[env.0 as usize].kind != CallableKind::FnOnce
            {
                for k in l.owners.iter().filter(|k| k.starts_with('C')) {
                    let live = c.live(k)?;
                    c.require("repeatable-environment-consumed", live)?;
                }
            }
            if let Some(v) = value
                && matches!(f.return_type, Type::ErrorUnion { .. })
            {
                let error = c.bit(&format!("e:V{}", v.0))?;
                let success = c.b.not(error)?;
                vec![
                    ("error-return".into(), error),
                    ("success-return".into(), success),
                ]
            } else {
                vec![("return".into(), 1)]
            }
        }
    };
    Ok((c.finish()?, terminal, next))
}

#[cfg(test)]
pub fn solve(m: &TypedModule, f: &TypedFunction, method: Method, limit: usize) -> R<Solution> {
    solve_encoded(
        m,
        f,
        method,
        limit,
        &mut crate::telemetry::Telemetry::disabled(),
        "canonical",
        false,
        Representation::Location,
    )
}

#[cfg(test)]
pub fn solve_diagnostic(
    m: &TypedModule,
    f: &TypedFunction,
    method: Method,
    limit: usize,
    diagnostic: &mut crate::telemetry::Telemetry,
    order: &str,
    gc: bool,
) -> R<Solution> {
    solve_encoded(
        m,
        f,
        method,
        limit,
        diagnostic,
        order,
        gc,
        Representation::Dense,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn solve_encoded(
    m: &TypedModule,
    f: &TypedFunction,
    method: Method,
    limit: usize,
    diagnostic: &mut crate::telemetry::Telemetry,
    order: &str,
    gc: bool,
    representation: Representation,
) -> R<Solution> {
    let began = std::time::Instant::now();
    let mut work = crate::budget::Work::new(limit);
    let layout = extract::extract(m, f, &mut work)
        .inspect_err(|error| diagnostic.extraction_failure(error))?;
    let encoding = Encoding::from_function(&layout, m, f, representation, &mut work)?;
    let extract_ns = began.elapsed().as_nanos();
    let began = std::time::Instant::now();
    let mut bdd = Bdd::new(encoding.physical.bits, limit);
    bdd.work = work;
    let permutation = crate::ordering::make(&encoding.physical, order, &mut bdd.work)?;
    bdd.set_order(permutation)?;
    let solved = (|| {
        let mut events = Vec::new();
        for p in 0..layout.points.len() {
            bdd.set_context("event", Some(p));
            let (b, i) = layout.points[p];
            if i == f.blocks[b].instructions.len() {
                let (t, terminal, next) = terminal(m, f, &layout, &encoding, &mut bdd, p, false)?;
                events.push(Event {
                    transfers: vec![t],
                    next,
                    terminal,
                });
            } else {
                let ins = &f.blocks[b].instructions[i];
                let mut transfers = vec![compile_ins(
                    m, f, &layout, &encoding, &mut bdd, p, false, false,
                )?];
                if let InstructionKind::Call { function, .. } = ins.kind
                    && m.functions[function.0 as usize].result_mode == PassingMode::Give
                {
                    transfers.push(compile_ins(
                        m, f, &layout, &encoding, &mut bdd, p, true, false,
                    )?);
                }
                events.push(Event {
                    transfers,
                    next: vec![(p + 1, 1)],
                    terminal: vec![],
                });
            }
            collect_boundary(&mut bdd, &events, &[], &BTreeMap::new(), gc)?;
            diagnostic.sample(&bdd, &events, &[], &BTreeMap::new())?;
        }
        let compile_ns = began.elapsed().as_nanos();
        let began = std::time::Instant::now();
        bdd.set_context("planning", None);
        let plans = events
            .iter()
            .map(|event| {
                event
                    .transfers
                    .iter()
                    .map(|t| bdd.plan(&t.factors, method))
                    .collect::<R<Vec<_>>>()
            })
            .collect::<R<Vec<_>>>()?;
        let plan_ns = began.elapsed().as_nanos();
        diagnostic.sample(&bdd, &events, &[], &BTreeMap::new())?;
        bdd.set_context("initial", None);
        let began = std::time::Instant::now();
        let start = initial(m, f, &layout, &encoding, &mut bdd)?;
        let n = events.len();
        bdd.work.charge(n.saturating_mul(4))?;
        let mut states = vec![0; n];
        let entry = layout.starts[f.entry.0 as usize];
        states[entry] = start;
        let mut q = VecDeque::from([entry]);
        let mut queued = vec![false; n];
        queued[entry] = true;
        let mut relations = BTreeMap::new();
        let mut status = QueryStatus::Complete;
        diagnostic.sample(&bdd, &events, &states, &relations)?;
        while let Some(p) = q.pop_front() {
            bdd.set_context("fixed-point", Some(p));
            queued[p] = false;
            let input = states[p];
            let mut output = 0;
            for (variant, t) in events[p].transfers.iter().enumerate() {
                let mut valid = input;
                for (label, condition) in &t.aborts {
                    let r = bdd.and(input, *condition)?;
                    record(&mut relations, &mut bdd, "abort", p, label, r)?;
                }
                for (label, condition) in &t.faults {
                    let r = bdd.and(input, *condition)?;
                    if label == "unsupported-overlapping-incarnation" {
                        if r != 0 {
                            status = QueryStatus::UnsupportedMultiplicity;
                        }
                        record(
                            &mut relations,
                            &mut bdd,
                            "unsupported",
                            p,
                            "overlapping-incarnation",
                            r,
                        )?;
                    } else if label != "projection-excludes-invalid-tag" {
                        record(&mut relations, &mut bdd, "fault", p, label, r)?;
                    }
                    let good = bdd.not(*condition)?;
                    valid = bdd.and(valid, good)?;
                }
                // A broken representation invariant must fail the experiment;
                // it is never treated as another program fault or deleted row.
                if bdd.and(valid, t.invariant_violation)? != 0 {
                    return Err(format!(
                        "Mismatch: normal transition loses unique token location at point {p}"
                    ));
                }
                bdd.context.phase = "image";
                let image = bdd.image(valid, &plans[p][variant])?;
                bdd.context.phase = "fixed-point";
                output = bdd.or(output, image)?;
            }
            for (label, guard) in &events[p].terminal {
                let exit = bdd.and(output, *guard)?;
                record(&mut relations, &mut bdd, "exit", p, label, exit)?;
            }
            for (target, guard) in &events[p].next {
                let incoming = bdd.and(output, *guard)?;
                let merged = bdd.or(states[*target], incoming)?;
                if merged != states[*target] {
                    states[*target] = merged;
                    if !queued[*target] {
                        q.push_back(*target);
                        queued[*target] = true;
                    }
                }
            }
            collect_boundary(&mut bdd, &events, &states, &relations, gc)?;
            diagnostic.sample(&bdd, &events, &states, &relations)?;
        }
        bdd.set_context("result", None);
        for (p, r) in states.into_iter().enumerate() {
            bdd.work.charge(32)?;
            if relations.len() >= MAX_RELATIONS {
                return Err("ResourceLimit: result relations".into());
            }
            relations.insert(format!("point:{p}"), r);
        }
        // Include the event/plan temporary destruction in solver time. Persistent
        // result/layout/BDD destruction is measured by the executable's caller.
        drop(plans);
        drop(events);
        let solve_ns = began.elapsed().as_nanos();
        Ok((
            relations,
            n,
            status,
            [extract_ns, compile_ns, plan_ns, solve_ns],
        ))
    })();
    diagnostic.finish(&bdd, solved.as_ref().err());
    let (relations, events, status, phase_ns) = solved?;
    Ok(Solution {
        encoding,
        layout,
        bdd,
        relations,
        explicit: None,
        events,
        status,
        phase_ns,
    })
}

pub(crate) fn collect_boundary(
    bdd: &mut Bdd,
    events: &[Event],
    states: &[Id],
    relations: &BTreeMap<String, Id>,
    enabled: bool,
) -> R<()> {
    // At most 300k physical arena slots in every mode. Recycling has a separate
    // 100M total-work cap; it does not grant an unbounded construction budget.
    if enabled && bdd.occupied_nodes() >= crate::budget::MAX_NODES * 2 / 3 {
        let started = bdd.work.used;
        let roots = crate::roots::persistent(events, states, relations, &mut bdd.work)?;
        let root_work = bdd.work.used - started;
        bdd.collect(&roots)?;
        bdd.gc_work += root_work;
    }
    Ok(())
}

fn record(
    relations: &mut BTreeMap<String, Id>,
    bdd: &mut Bdd,
    category: &str,
    p: usize,
    label: &str,
    r: Id,
) -> R<()> {
    if r == 0 {
        return Ok(());
    }
    bdd.work.charge(
        32usize
            .saturating_add(category.len())
            .saturating_add(label.len()),
    )?;
    let key = format!("{category}:{p}:{label}");
    let previous = *relations.get(&key).unwrap_or(&0);
    if previous == 0 && relations.len() >= MAX_RELATIONS {
        return Err("ResourceLimit: result relations".into());
    }
    let merged = bdd.or(previous, r)?;
    relations.insert(key, merged);
    Ok(())
}
