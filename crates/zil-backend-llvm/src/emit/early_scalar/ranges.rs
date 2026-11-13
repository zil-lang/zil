use super::ssa::Cfg;
use super::*;

// Conditional bounds: these describe every NON-POISON result, never establish
// noundef. The only transformation they authorize removes provably false
// overflow flags. It is a refinement for poison inputs; the plain arithmetic
// still propagates poison. In particular Freeze must not inherit these bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Range {
    bits: u32,
    s0: i128,
    s1: i128,
    u0: i128,
    u1: i128,
}
impl Range {
    fn full(bits: u32) -> Self {
        Self {
            bits,
            s0: -(1i128 << (bits - 1)),
            s1: (1i128 << (bits - 1)) - 1,
            u0: 0,
            u1: (1i128 << bits) - 1,
        }
    }
    fn constant(bits: u32, value: i128) -> Self {
        let mask = (1u128 << bits) - 1;
        let u = ((value as u128) & mask) as i128;
        let s = if u >= 1i128 << (bits - 1) {
            u - (1i128 << bits)
        } else {
            u
        };
        Self {
            bits,
            s0: s,
            s1: s,
            u0: u,
            u1: u,
        }
    }
    fn signed(bits: u32, lo: i128, hi: i128) -> Self {
        let mut r = Self::full(bits);
        r.s0 = lo;
        r.s1 = hi;
        if lo >= 0 {
            r.u0 = lo;
            r.u1 = hi;
        } else if hi < 0 {
            r.u0 = lo + (1i128 << bits);
            r.u1 = hi + (1i128 << bits);
        }
        r
    }
    fn unsigned(bits: u32, lo: i128, hi: i128) -> Self {
        let mut r = Self::full(bits);
        r.u0 = lo;
        r.u1 = hi;
        let sign = 1i128 << (bits - 1);
        if hi < sign {
            r.s0 = lo;
            r.s1 = hi;
        } else if lo >= sign {
            r.s0 = lo - (1i128 << bits);
            r.s1 = hi - (1i128 << bits);
        }
        r
    }
    fn join(self, rhs: Self) -> Option<Self> {
        (self.bits == rhs.bits).then_some(Self {
            bits: self.bits,
            s0: self.s0.min(rhs.s0),
            s1: self.s1.max(rhs.s1),
            u0: self.u0.min(rhs.u0),
            u1: self.u1.max(rhs.u1),
        })
    }
}
fn bits(ty: &m::Ty) -> Option<u32> {
    match ty {
        m::Ty::Int(n @ 1..=64) => Some(*n),
        _ => None,
    }
}
fn value(v: &m::Val, width: u32, facts: &BTreeMap<String, Range>) -> Option<Range> {
    match v {
        m::Val::Int(n) => Some(Range::constant(width, *n)),
        m::Val::Bool(b) if width == 1 => Some(Range::constant(1, i128::from(*b))),
        m::Val::Zero => Some(Range::constant(width, 0)),
        m::Val::Local(n) => facts.get(n).copied().filter(|r| r.bits == width),
        // Explicit undef/poison, loads, unknown producers and non-integers do
        // not gain a range. This is deliberately more conservative than LLVM.
        _ => None,
    }
}
#[derive(Clone, Copy, Debug)]
struct Checked {
    width: u32,
    signed: bool,
    opcode: &'static str,
}
pub(super) fn canonical_checked_call(op: &m::Op) -> bool {
    checked(op).is_some()
}
fn checked(op: &m::Op) -> Option<Checked> {
    let m::Op::Call {
        result,
        result_attrs,
        callee: m::Val::Global(name),
        args,
        vararg,
    } = op
    else {
        return None;
    };
    if !result_attrs.is_empty()
        || vararg.is_some()
        || args.len() != 2
        || args.iter().any(|a| !a.attrs.is_empty())
    {
        return None;
    }
    let (kind, width) = name.strip_prefix("llvm.")?.split_once(".with.overflow.i")?;
    let width: u32 = width.parse().ok()?;
    if !(1..=64).contains(&width) || name != &format!("llvm.{kind}.with.overflow.i{width}") {
        return None;
    }
    if *result != m::Ty::Struct(vec![m::Ty::Int(width), m::Ty::Int(1)])
        || args.iter().any(|a| a.ty != m::Ty::Int(width))
    {
        return None;
    }
    let (signed, opcode) = match kind {
        "sadd" => (true, "add"),
        "ssub" => (true, "sub"),
        "smul" => (true, "mul"),
        "uadd" => (false, "add"),
        "usub" => (false, "sub"),
        "umul" => (false, "mul"),
        _ => return None,
    };
    Some(Checked {
        width,
        signed,
        opcode,
    })
}
fn endpoints(op: &str, a: (i128, i128), b: (i128, i128)) -> Option<(i128, i128)> {
    let eval = |x: i128, y: i128| match op {
        "add" => x.checked_add(y),
        "sub" => x.checked_sub(y),
        "mul" => x.checked_mul(y),
        _ => None,
    };
    let mut lo = i128::MAX;
    let mut hi = i128::MIN;
    for x in [a.0, a.1] {
        for y in [b.0, b.1] {
            let n = eval(x, y)?;
            lo = lo.min(n);
            hi = hi.max(n);
        }
    }
    Some((lo, hi))
}
fn safe_result(c: Checked, a: Range, b: Range) -> Option<Range> {
    let full = Range::full(c.width);
    let (lo, hi) = if c.signed {
        endpoints(c.opcode, (a.s0, a.s1), (b.s0, b.s1))?
    } else {
        endpoints(c.opcode, (a.u0, a.u1), (b.u0, b.u1))?
    };
    if c.signed {
        (lo >= full.s0 && hi <= full.s1).then(|| Range::signed(c.width, lo, hi))
    } else {
        (lo >= 0 && hi <= full.u1).then(|| Range::unsigned(c.width, lo, hi))
    }
}
fn binary(opcode: &str, a: Range, b: Range) -> Option<Range> {
    let width = a.bits;
    if b.bits != width {
        return None;
    }
    match opcode {
        "add" | "sub" | "mul" => {
            for signed in [true, false] {
                if let Some(r) = safe_result(
                    Checked {
                        width,
                        signed,
                        opcode: match opcode {
                            "add" => "add",
                            "sub" => "sub",
                            _ => "mul",
                        },
                    },
                    a,
                    b,
                ) {
                    return Some(r);
                }
            }
            Some(Range::full(width)) // Defined wrapping results, without flags.
        }
        "srem" if b.s0 == b.s1 && b.s0 > 0 => {
            let max = b.s0 - 1;
            Some(Range::signed(
                width,
                if a.s0 >= 0 { 0 } else { a.s0.max(-max) },
                if a.s1 <= 0 { 0 } else { a.s1.min(max) },
            ))
        }
        "urem" if b.u0 == b.u1 && b.u0 > 0 => Some(Range::unsigned(width, 0, a.u1.min(b.u0 - 1))),
        "sdiv" if b.s0 == b.s1 && b.s0 > 0 => Some(Range::signed(width, a.s0 / b.s0, a.s1 / b.s0)),
        "udiv" if b.u0 == b.u1 && b.u0 > 0 => {
            Some(Range::unsigned(width, a.u0 / b.u0, a.u1 / b.u0))
        }
        "and" if b.u0 == b.u1 => Some(Range::unsigned(width, 0, b.u1)),
        "and" if a.u0 == a.u1 => Some(Range::unsigned(width, 0, a.u1)),
        "and" | "or" | "xor" => Some(Range::full(width)),
        _ => None,
    }
}
fn infer(
    op: &m::Op,
    block: usize,
    cfg: &Cfg,
    facts: &BTreeMap<String, Range>,
    pairs: &BTreeMap<String, (Range, bool)>,
) -> Option<Range> {
    match op {
        m::Op::Binary {
            opcode,
            ty,
            left,
            right,
        } => {
            let n = bits(ty)?;
            binary(opcode, value(left, n, facts)?, value(right, n, facts)?)
        }
        m::Op::Compare {
            predicate,
            ty,
            left,
            right,
        } if matches!(
            predicate.as_str(),
            "eq" | "ne" | "slt" | "sle" | "sgt" | "sge" | "ult" | "ule" | "ugt" | "uge"
        ) =>
        {
            let n = bits(ty)?;
            value(left, n, facts)?;
            value(right, n, facts)?;
            Some(Range::full(1))
        }
        m::Op::Cast { opcode, arg, to } if arg.attrs.is_empty() => {
            let (from, to) = (bits(&arg.ty)?, bits(to)?);
            let r = value(&arg.value, from, facts)?;
            match opcode.as_str() {
                "zext" if to > from => Some(Range::unsigned(to, r.u0, r.u1)),
                "sext" if to > from => Some(Range::signed(to, r.s0, r.s1)),
                "trunc" if to < from => {
                    let max = Range::full(to);
                    if r.u1 <= max.u1 {
                        Some(Range::unsigned(to, r.u0, r.u1))
                    } else if r.s0 >= max.s0 && r.s1 <= max.s1 {
                        Some(Range::signed(to, r.s0, r.s1))
                    } else {
                        Some(max)
                    }
                }
                _ => None,
            }
        }
        m::Op::Extract { arg, index } if arg.attrs.is_empty() => {
            let m::Val::Local(name) = &arg.value else {
                return None;
            };
            let (r, safe) = pairs.get(name)?;
            if arg.ty != m::Ty::Struct(vec![m::Ty::Int(r.bits), m::Ty::Int(1)]) {
                return None;
            }
            match index {
                0 => Some(*r),
                1 => Some(if *safe {
                    Range::constant(1, 0)
                } else {
                    Range::full(1)
                }),
                _ => None,
            }
        }
        m::Op::Phi { ty, incoming } => {
            // Widen any backedge-bearing phi to unknown. This also breaks range
            // cycles in irreducible graphs; no finite optimistic loop bound used.
            if incoming.is_empty()
                || incoming
                    .iter()
                    .any(|(_, p)| cfg.rank[cfg.labels[p]] >= cfg.rank[block])
            {
                return None;
            }
            let n = bits(ty)?;
            let mut joined = None;
            for (v, _) in incoming {
                let r = value(v, n, facts)?;
                joined = Some(match joined {
                    None => r,
                    Some(old) => r.join(old)?,
                });
            }
            joined
        }
        // In particular Freeze(conditional-range) is NOT that same range.
        _ => None,
    }
}

pub(super) fn simplify(machine: &mut m::Function, cfg: &Cfg, fuel: &mut Fuel) -> Option<usize> {
    let blocks = machine.blocks.as_ref()?;
    let mut facts = BTreeMap::new();
    for p in &machine.signature.params {
        if let (Some(name), Some(n)) = (&p.name, bits(&p.ty)) {
            facts.insert(name.clone(), Range::full(n));
        }
    }
    let mut pairs = BTreeMap::new();
    loop {
        let mut changed = false;
        for &b in &cfg.order {
            for inst in &blocks[b].instructions {
                fuel.take(1)?;
                let Some(name) = &inst.result else {
                    continue;
                };
                if let Some(c) = checked(&inst.op) {
                    let m::Op::Call { args, .. } = &inst.op else {
                        unreachable!()
                    };
                    if let (Some(a), Some(b)) = (
                        value(&args[0].value, c.width, &facts),
                        value(&args[1].value, c.width, &facts),
                    ) {
                        let safe = safe_result(c, a, b);
                        let next = (safe.unwrap_or_else(|| Range::full(c.width)), safe.is_some());
                        if pairs.get(name) != Some(&next) {
                            pairs.insert(name.clone(), next);
                            changed = true;
                        }
                    }
                }
                if let Some(next) = infer(&inst.op, b, cfg, &facts, &pairs)
                    && facts.get(name) != Some(&next)
                {
                    facts.insert(name.clone(), next);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let mut safe = BTreeMap::new();
    for &b in &cfg.order {
        for inst in &blocks[b].instructions {
            fuel.take(1)?;
            if let (Some(name), Some(c)) = (&inst.result, checked(&inst.op))
                && pairs.get(name).is_some_and(|(_, safe)| *safe)
            {
                safe.insert(name.clone(), c);
            }
        }
    }
    let mut allowed = safe.keys().cloned().collect::<BTreeSet<_>>();
    let mut uses = BTreeSet::new();
    for block in blocks {
        for inst in &block.instructions {
            fuel.take(1)?;
            for v in inst.op.values() {
                visit_local(v, &mut |name| {
                    if let Some(c) = safe.get(name) {
                        let valid = matches!(&inst.op,m::Op::Extract { arg,index:0|1 }
                    if inst.result.is_some() && arg.attrs.is_empty() && arg.value==m::local(name.to_owned())
                    && arg.ty==m::Ty::Struct(vec![m::Ty::Int(c.width),m::Ty::Int(1)]));
                        if !valid {
                            allowed.remove(name);
                        } else {
                            uses.insert(name.to_owned());
                        }
                    }
                });
            }
        }
    }
    allowed.retain(|n| uses.contains(n));
    if allowed.is_empty() {
        return Some(0);
    }
    let mut aliases = BTreeMap::new();
    let output = machine.blocks.as_mut()?;
    for block in output.iter_mut() {
        let old = std::mem::take(&mut block.instructions);
        let mut new = Vec::with_capacity(old.len());
        for mut inst in old {
            if let Some(name) = inst.result.as_ref().filter(|name| allowed.contains(*name)) {
                let c = safe[name];
                let m::Op::Call { args, .. } = inst.op else {
                    return None;
                };
                inst.op = m::Op::Binary {
                    opcode: c.opcode.into(),
                    ty: m::Ty::Int(c.width),
                    left: args[0].value.clone(),
                    right: args[1].value.clone(),
                };
            } else if let m::Op::Extract { arg, index } = &inst.op
                && let m::Val::Local(name) = &arg.value
                && allowed.contains(name)
            {
                aliases.insert(
                    inst.result.as_ref()?.clone(),
                    if *index == 0 {
                        m::local(name.clone())
                    } else {
                        m::Val::Bool(false)
                    },
                );
                continue;
            }
            new.push(inst);
        }
        block.instructions = new;
    }
    rewrite_aliases(machine, &aliases, fuel)?;
    Some(allowed.len())
}

#[cfg(test)]
pub(super) fn checked_for_test(op: &m::Op) -> bool {
    checked(op).is_some()
}
