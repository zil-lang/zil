//! Transactional cleanup of the produced scalar SSA graph. Only literal integer
//! facts fold control flow. If conversion moves total integer operations, never
//! calls, memory operations, checked operations, shifts, divisions or freezes.
use super::*;
use std::collections::VecDeque;

const MAX_ROUNDS: usize = 256;

struct Graph {
    labels: BTreeMap<String, usize>,
    successors: Vec<Vec<usize>>,
    predecessors: Vec<Vec<usize>>,
    order: Vec<usize>,
    rank: Vec<usize>,
}
impl Graph {
    fn new(machine: &m::Function, fuel: &mut Fuel) -> Option<Self> {
        let blocks = machine.blocks.as_ref()?;
        if blocks.is_empty() { return None; }
        let mut labels = BTreeMap::new();
        for (b, block) in blocks.iter().enumerate() {
            fuel.take(1)?;
            if labels.insert(block.name.clone(), b).is_some() { return None; }
        }
        let mut successors = vec![vec![]; blocks.len()];
        let mut predecessors = vec![vec![]; blocks.len()];
        for (b, block) in blocks.iter().enumerate() {
            fuel.take(1)?;
            let names = match &block.instructions.last()?.op {
                m::Op::Branch(name) => vec![name],
                m::Op::CondBranch { yes, no, .. } => vec![yes, no],
                m::Op::Return(_) | m::Op::Unreachable => vec![],
                _ => return None,
            };
            for name in names {
                let target = *labels.get(name)?;
                successors[b].push(target);
                predecessors[target].push(b);
            }
        }
        if !predecessors[0].is_empty() { return None; }
        let mut visited = vec![false; blocks.len()];
        let mut stack = vec![(0usize, false)];
        let mut order = vec![];
        while let Some((b, exit)) = stack.pop() {
            fuel.take(1)?;
            if exit { order.push(b); continue; }
            if std::mem::replace(&mut visited[b], true) { continue; }
            stack.push((b, true));
            for &next in successors[b].iter().rev() {
                if !visited[next] { stack.push((next, false)); }
            }
        }
        order.reverse();
        let mut rank = vec![usize::MAX; blocks.len()];
        for (position, &b) in order.iter().enumerate() { rank[b] = position; }
        Some(Self { labels, successors, predecessors, order, rank })
    }
}
fn charge_value(value: &m::Val, fuel: &mut Fuel) -> Option<()> {
    fuel.take(1)?;
    match value {
        m::Val::Array(args) => for arg in args { charge_value(&arg.value,fuel)?; },
        m::Val::Gep { ptr,indices,.. } => { charge_value(ptr,fuel)?; for arg in indices { charge_value(&arg.value,fuel)?; } },
        _ => {}
    }
    Some(())
}
fn charge_op(op: &m::Op, fuel: &mut Fuel) -> Option<()> {
    fuel.take(1)?;
    for value in op.values() { charge_value(value,fuel)?; }
    Some(())
}
fn width(ty: &m::Ty) -> Option<u32> {
    match ty { m::Ty::Int(n @ 1..=64) => Some(*n), _ => None }
}
fn integer(value: &m::Val, bits: u32) -> Option<u128> {
    let value = match value {
        m::Val::Int(value) if *value >= -(1i128 << (bits-1)) && *value <= (1i128 << bits)-1 => *value as u128,
        m::Val::Bool(value) if bits == 1 => u128::from(*value),
        m::Val::Zero => 0,
        _ => return None,
    };
    Some(value & ((1u128 << bits) - 1))
}
fn signed(value: u128, bits: u32) -> i128 {
    let n = value as i128;
    if value & (1u128 << (bits - 1)) != 0 { n - (1i128 << bits) } else { n }
}
fn literal(value: u128, bits: u32) -> m::Val {
    let n = value & ((1u128 << bits) - 1);
    if bits == 1 { m::Val::Bool(n != 0) } else { m::Val::Int(signed(n, bits)) }
}
fn boolean(value: &m::Val) -> Option<bool> {
    match value {
        m::Val::Bool(value) => Some(*value),
        m::Val::Int(0) | m::Val::Zero => Some(false),
        m::Val::Int(1) => Some(true),
        _ => None,
    }
}
fn integer_compare(predicate: &str) -> bool {
    matches!(predicate, "eq" | "ne" | "slt" | "sle" | "sgt" | "sge" | "ult" | "ule" | "ugt" | "uge")
}
fn total(op: &m::Op) -> bool {
    match op {
        m::Op::Binary { opcode, ty, .. } => width(ty).is_some()
            && matches!(opcode.as_str(), "add" | "sub" | "mul" | "and" | "or" | "xor"),
        m::Op::Compare { predicate, ty, .. } => width(ty).is_some() && integer_compare(predicate),
        m::Op::Cast { opcode, arg, to } if arg.attrs.is_empty() => {
            let (Some(from), Some(to)) = (width(&arg.ty), width(to)) else { return false; };
            match opcode.as_str() { "zext" | "sext" => to > from, "trunc" => to < from, _ => false }
        }
        m::Op::Select { ty, .. } => width(ty).is_some(),
        _ => false,
    }
}
fn constant(op: &m::Op) -> Option<m::Val> {
    match op {
        m::Op::Binary { opcode, ty, left, right } if total(op) => {
            let bits = width(ty)?;
            let (a, b) = (integer(left, bits)?, integer(right, bits)?);
            let value = match opcode.as_str() {
                "add" => a.checked_add(b)?, "sub" => a.wrapping_sub(b),
                "mul" => a.checked_mul(b)?, "and" => a & b, "or" => a | b, "xor" => a ^ b,
                _ => return None,
            };
            Some(literal(value, bits))
        }
        m::Op::Compare { predicate, ty, left, right } if total(op) => {
            let bits = width(ty)?;
            let (a, b) = (integer(left, bits)?, integer(right, bits)?);
            let value = match predicate.as_str() {
                "eq" => a == b, "ne" => a != b,
                "slt" => signed(a,bits) < signed(b,bits), "sle" => signed(a,bits) <= signed(b,bits),
                "sgt" => signed(a,bits) > signed(b,bits), "sge" => signed(a,bits) >= signed(b,bits),
                "ult" => a < b, "ule" => a <= b, "ugt" => a > b, "uge" => a >= b,
                _ => return None,
            };
            Some(m::Val::Bool(value))
        }
        m::Op::Cast { opcode, arg, to } if total(op) => {
            let (from, to) = (width(&arg.ty)?, width(to)?);
            let n = integer(&arg.value, from)?;
            Some(literal(if opcode == "sext" { signed(n,from) as u128 } else { n }, to))
        }
        // Select only evaluates its selected value; do not infer anything from
        // an unknown condition, even when its two arms happen to be equal.
        m::Op::Select { ty, condition, yes, no } if width(ty).is_some() => {
            let selected = if boolean(condition)? { yes } else { no };
            Some(selected.clone())
        }
        _ => None,
    }
}
fn replace_known(value: &mut m::Val, known: &BTreeMap<String,m::Val>) {
    match value {
        m::Val::Local(name) => if let Some(next) = known.get(name) { *value = next.clone(); },
        m::Val::Array(args) => for arg in args { replace_known(&mut arg.value,known); },
        m::Val::Gep { ptr, indices, .. } => {
            replace_known(ptr,known);
            for arg in indices { replace_known(&mut arg.value,known); }
        }
        _ => {}
    }
}
fn propagate(machine: &mut m::Function, graph: &Graph, fuel: &mut Fuel) -> Option<usize> {
    let mut known = BTreeMap::new();
    loop {
        let mut changed = false;
        for &b in &graph.order {
            for inst in &mut machine.blocks.as_mut()?[b].instructions {
                charge_op(&inst.op,fuel)?;
                map_operands(&mut inst.op,&mut |value| replace_known(value,&known));
                let Some(name) = &inst.result else { continue; };
                if known.contains_key(name) { continue; }
                if let Some(value) = constant(&inst.op) {
                    // Nonliteral selected values are still exact aliases. The
                    // common resolver detects self references/cycles and rolls
                    // back the whole optional pass, never inventing an undef.
                    known.insert(name.clone(),value);
                    changed = true;
                }
            }
        }
        if !changed { break; }
    }
    if known.is_empty() { return Some(0); }
    rewrite_aliases(machine,&known,fuel)?;
    for block in machine.blocks.as_mut()?.iter_mut() {
        block.instructions.retain(|inst| !inst.result.as_ref().is_some_and(|name| known.contains_key(name)));
    }
    Some(known.len())
}
fn fold_branches(machine: &mut m::Function, fuel: &mut Fuel) -> Option<usize> {
    let mut changed = 0;
    for block in machine.blocks.as_mut()?.iter_mut() {
        fuel.take(1)?;
        let last = block.instructions.last_mut()?;
        if let m::Op::CondBranch { condition, yes, no } = &last.op
            && let Some(value) = boolean(condition)
        {
            last.op = m::Op::Branch(if value { yes.clone() } else { no.clone() });
            changed += 1;
        }
    }
    Some(changed)
}
fn prune(machine: &mut m::Function, graph: &Graph, fuel: &mut Fuel) -> Option<usize> {
    let blocks = machine.blocks.as_mut()?;
    let mut changed = blocks.len() - graph.order.len();
    for &b in &graph.order {
        let mut expected = BTreeMap::<String,usize>::new();
        for &p in &graph.predecessors[b] {
            fuel.take(1)?;
            if graph.rank[p] != usize::MAX { *expected.entry(blocks[p].name.clone()).or_default() += 1; }
        }
        for inst in &mut blocks[b].instructions {
            fuel.take(1)?;
            let m::Op::Phi { incoming, .. } = &mut inst.op else { continue; };
            let mut original = BTreeMap::<String,m::Val>::new();
            for (value,pred) in incoming.iter() {
                fuel.take(1)?;
                if !graph.labels.contains_key(pred) { return None; }
                if let Some(previous) = original.insert(pred.clone(),value.clone())
                    && previous != *value { return None; }
            }
            let mut next = vec![];
            for (pred,count) in &expected {
                let value = original.get(pred)?;
                for _ in 0..*count { next.push((value.clone(),pred.clone())); }
            }
            if next.is_empty() { return None; }
            // Preserve the original order when the edge multiset did not
            // change. No pointer/address identity enters this decision.
            let mut old_counts = BTreeMap::<String,usize>::new();
            for (_,pred) in incoming.iter() { *old_counts.entry(pred.clone()).or_default() += 1; }
            if old_counts != expected { changed += 1; *incoming = next; }
        }
    }
    let mut index = 0;
    blocks.retain(|_| { let keep = graph.rank[index] != usize::MAX; index += 1; keep });
    Some(changed)
}
fn eliminate_phis(machine: &mut m::Function, fuel: &mut Fuel) -> Option<usize> {
    let mut aliases = BTreeMap::new();
    for block in machine.blocks.as_ref()? {
        for inst in &block.instructions {
            charge_op(&inst.op,fuel)?;
            let (Some(name),m::Op::Phi { incoming,.. }) = (&inst.result,&inst.op) else { continue; };
            let first = &incoming.first()?.0;
            let mut self_reference = false;
            visit_local(first,&mut |n| { self_reference |= n == name; });
            if !self_reference && incoming.iter().all(|(value,_)| value == first) {
                aliases.insert(name.clone(),first.clone());
            }
        }
    }
    if aliases.is_empty() { return Some(0); }
    rewrite_aliases(machine,&aliases,fuel)?;
    for block in machine.blocks.as_mut()?.iter_mut() {
        block.instructions.retain(|inst| !inst.result.as_ref().is_some_and(|name| aliases.contains_key(name)));
    }
    Some(aliases.len())
}
fn dead_pure(machine: &mut m::Function, fuel: &mut Fuel) -> Option<usize> {
    let blocks = machine.blocks.as_ref()?;
    let mut definitions = BTreeMap::new();
    let mut uses = BTreeMap::<String,usize>::new();
    for (b,block) in blocks.iter().enumerate() {
        for (i,inst) in block.instructions.iter().enumerate() {
            charge_op(&inst.op,fuel)?;
            if let Some(name) = &inst.result { definitions.insert(name.clone(),(b,i)); }
            for value in inst.op.values() { visit_local(value,&mut |name| { *uses.entry(name.into()).or_default() += 1; }); }
        }
    }
    let removable = |inst: &m::Inst| inst.result.is_some() && (total(&inst.op) || matches!(&inst.op,m::Op::Phi { ty,.. } if width(ty).is_some()));
    let mut queue = VecDeque::new();
    for (name,&(b,i)) in &definitions {
        if uses.get(name).copied().unwrap_or(0) == 0 && removable(&blocks[b].instructions[i]) { queue.push_back(name.clone()); }
    }
    let mut dead = BTreeSet::new();
    while let Some(name) = queue.pop_front() {
        fuel.take(1)?;
        if !dead.insert(name.clone()) { continue; }
        let &(b,i) = definitions.get(&name)?;
        charge_op(&blocks[b].instructions[i].op,fuel)?;
        for value in blocks[b].instructions[i].op.values() {
            let mut referenced = vec![];
            visit_local(value,&mut |name| referenced.push(name.to_owned()));
            for reference in referenced {
                let count = uses.get_mut(&reference)?;
                *count = count.checked_sub(1)?;
                if *count == 0 && let Some(&(b,i)) = definitions.get(&reference)
                    && removable(&blocks[b].instructions[i]) { queue.push_back(reference); }
            }
        }
    }
    if dead.is_empty() { return Some(0); }
    for block in machine.blocks.as_mut()?.iter_mut() {
        block.instructions.retain(|inst| !inst.result.as_ref().is_some_and(|name| dead.contains(name)));
    }
    Some(dead.len())
}
fn linear(machine: &mut m::Function, graph: &Graph, fuel: &mut Fuel) -> Option<usize> {
    let blocks = machine.blocks.as_ref()?;
    let mut parent = vec![None; blocks.len()];
    let mut heads = (0..blocks.len()).collect::<Vec<_>>();
    for &b in graph.order.iter().skip(1) {
        fuel.take(1)?;
        if graph.predecessors[b].len() != 1 || blocks[b].instructions.iter().any(|i| matches!(i.op,m::Op::Phi { .. })) { continue; }
        let p = graph.predecessors[b][0];
        if p == b || graph.rank[p] >= graph.rank[b] { continue; }
        if matches!(&blocks[p].instructions.last()?.op,m::Op::Branch(target) if target == &blocks[b].name) {
            parent[b] = Some(p);
            heads[b] = heads[p];
        }
    }
    let changed = parent.iter().filter(|p| p.is_some()).count();
    if changed == 0 { return Some(0); }
    let names = blocks.iter().map(|b|b.name.clone()).collect::<Vec<_>>();
    let mut old = std::mem::take(machine.blocks.as_mut()?);
    let mut output = Vec::with_capacity(old.len()-changed);
    for head in 0..old.len() {
        if parent[head].is_some() { continue; }
        let mut instructions = vec![];
        let mut current = head;
        loop {
            fuel.take(1)?;
            let next = graph.successors[current].first().copied().filter(|&next| parent[next] == Some(current));
            let mut body = std::mem::take(&mut old[current].instructions);
            if next.is_some() { body.pop()?; }
            instructions.extend(body);
            if let Some(next) = next { current = next; } else { break; }
        }
        for inst in &mut instructions {
            charge_op(&inst.op,fuel)?;
            match &mut inst.op {
                m::Op::Phi { incoming,.. } => for (_,pred) in incoming { *pred = names[heads[*graph.labels.get(pred)?]].clone(); },
                m::Op::Branch(target) => *target = names[heads[*graph.labels.get(target)?]].clone(),
                m::Op::CondBranch { yes,no,.. } => {
                    *yes = names[heads[*graph.labels.get(yes)?]].clone();
                    *no = names[heads[*graph.labels.get(no)?]].clone();
                }
                _ => {}
            }
        }
        output.push(m::Block { name:names[head].clone(),instructions });
    }
    *machine.blocks.as_mut()? = output.into();
    Some(changed)
}

struct RemainderTail {
    a: String,
    b: String,
    ty: m::Ty,
    opcode: String,
    yes: m::Val,
    no: m::Val,
    divisor: m::Val,
}
fn remainder(op: &m::Op) -> Option<(&str,&m::Ty,&m::Val,&m::Val,u128)> {
    let m::Op::Binary { opcode,ty,left,right } = op else { return None; };
    let bits = width(ty)?;
    let divisor = integer(right,bits)?;
    let positive = match opcode.as_str() {
        "srem" => signed(divisor,bits) > 0,
        "urem" => divisor > 0,
        _ => false,
    };
    positive.then_some((opcode.as_str(),ty,left,right,divisor))
}
fn plain_value(value: &m::Val) -> bool {
    matches!(value,m::Val::Local(_) | m::Val::Int(_) | m::Val::Bool(_) | m::Val::Zero)
}
fn fresh_name(machine: &m::Function, base: String, fuel: &mut Fuel) -> Option<String> {
    let mut names = BTreeSet::new();
    for param in &machine.signature.params { if let Some(name) = &param.name { names.insert(name.clone()); } }
    for block in machine.blocks.as_ref()? {
        names.insert(block.name.clone());
        for inst in &block.instructions {
            fuel.take(1)?;
            if let Some(name) = &inst.result { names.insert(name.clone()); }
        }
    }
    let mut name = base.clone(); let mut suffix = 0usize;
    while names.contains(&name) { fuel.take(1)?; suffix += 1; name = format!("{base}.{suffix}"); }
    Some(name)
}

// If conversion is deliberately a closed CFG pattern, not an instruction
// scheduling heuristic. Every arm definition and every original phi is checked
// before any instruction moves. The two original branch edges choose the same
// two values as the new Select; no condition is cloned or recomputed.
fn diamond(machine: &mut m::Function, graph: &Graph, fuel: &mut Fuel) -> Option<usize> {
    let blocks = machine.blocks.as_ref()?;
    for &p in &graph.order {
        fuel.take(1)?;
        let m::Op::CondBranch { condition,yes,no } = &blocks[p].instructions.last()?.op else { continue; };
        if !matches!(condition,m::Val::Local(_)) { continue; }
        let (a,b) = (*graph.labels.get(yes)?,*graph.labels.get(no)?);
        if a == b || a == p || b == p || graph.predecessors[a] != [p] || graph.predecessors[b] != [p] { continue; }
        let (m::Op::Branch(aj),m::Op::Branch(bj)) = (&blocks[a].instructions.last()?.op,&blocks[b].instructions.last()?.op) else { continue; };
        if aj != bj { continue; }
        let j = *graph.labels.get(aj)?;
        if j == p || j == a || j == b || graph.rank[p] >= graph.rank[a] || graph.rank[p] >= graph.rank[b]
            || graph.rank[a] >= graph.rank[j] || graph.rank[b] >= graph.rank[j] { continue; }
        let mut expected = vec![a,b]; expected.sort_unstable();
        let mut actual = graph.predecessors[j].clone(); actual.sort_unstable();
        if actual != expected { continue; }
        let arm_a = &blocks[a].instructions[..blocks[a].instructions.len()-1];
        let arm_b = &blocks[b].instructions[..blocks[b].instructions.len()-1];
        let (prefix_a,prefix_b,tail) = if arm_a.iter().chain(arm_b).all(|inst| inst.result.is_some() && total(&inst.op)) {
            (arm_a,arm_b,None)
        } else {
            let (Some(last_a),Some(last_b)) = (arm_a.last(),arm_b.last()) else { continue; };
            let (Some((ao,at,av,ad,abits)),Some((bo,bt,bv,_,bbits))) = (remainder(&last_a.op),remainder(&last_b.op)) else { continue; };
            if ao != bo || at != bt || abits != bbits { continue; }
            let (Some(an),Some(bn)) = (&last_a.result,&last_b.result) else { continue; };
            let (ap,bp) = (&arm_a[..arm_a.len()-1],&arm_b[..arm_b.len()-1]);
            if !ap.iter().chain(bp).all(|inst| inst.result.is_some() && total(&inst.op)) { continue; }
            (ap,bp,Some(RemainderTail { a:an.clone(),b:bn.clone(),ty:at.clone(),opcode:ao.into(),yes:av.clone(),no:bv.clone(),divisor:ad.clone() }))
        };
        let mut definitions = BTreeMap::new();
        let mut explicit_unknown = false;
        for (arm,instructions) in [(a,arm_a),(b,arm_b)] {
            for inst in instructions {
                charge_op(&inst.op,fuel)?;
                definitions.insert(inst.result.as_ref()?.clone(),arm);
                for value in inst.op.values() { explicit_unknown |= !plain_value(value) || has_unknown(value); }
            }
        }
        if explicit_unknown { continue; }
        let mut replacements = vec![];
        let mut aliases = BTreeMap::new();
        let mut tail_used = false;
        let mut valid = true;
        for inst in &blocks[j].instructions {
            charge_op(&inst.op,fuel)?;
            if let m::Op::Phi { ty,incoming } = &inst.op {
                if width(ty).is_none() || incoming.len() != 2 { valid=false;break; }
                let av = incoming.iter().find(|(_,name)| name == &blocks[a].name).map(|(v,_)|v);
                let bv = incoming.iter().find(|(_,name)| name == &blocks[b].name).map(|(v,_)|v);
                let (Some(av),Some(bv)) = (av,bv) else { valid=false;break; };
                if !plain_value(av) || !plain_value(bv) || has_unknown(av) || has_unknown(bv) { valid=false;break; }
                if let Some(tail) = &tail {
                    if av == &m::local(tail.a.clone()) && bv == &m::local(tail.b.clone()) && ty == &tail.ty {
                        aliases.insert(inst.result.as_ref()?.clone(),m::local(tail.a.clone()));
                        tail_used = true;
                        continue;
                    }
                    let mut references_tail = false;
                    for value in [av,bv] { visit_local(value,&mut |name| { references_tail |= name == tail.a || name == tail.b; }); }
                    if references_tail { valid=false;break; }
                }
                replacements.push(m::Inst::new(inst.result.clone(),m::Op::Select { ty:ty.clone(),condition:condition.clone(),yes:av.clone(),no:bv.clone() }));
            }
        }
        if !valid || (tail.is_some() && !tail_used) { continue; }
        for (at,block) in blocks.iter().enumerate() {
            for inst in &block.instructions {
                charge_op(&inst.op,fuel)?;
                match &inst.op {
                    m::Op::Phi { incoming,.. } if at == j => {
                        for (value,pred) in incoming {
                            visit_local(value,&mut |name| {
                                if let Some(&arm) = definitions.get(name) { valid &= pred == &blocks[arm].name; }
                            });
                        }
                    }
                    _ => for value in inst.op.values() {
                        visit_local(value,&mut |name| {
                            if let Some(&arm) = definitions.get(name) {
                                let terminal_rem = tail.as_ref().is_some_and(|tail| name == tail.a || name == tail.b);
                                valid &= at == arm && !terminal_rem;
                            }
                        });
                    },
                }
            }
        }
        if !valid { continue; }
        let mut moved = prefix_a.to_vec(); moved.extend_from_slice(prefix_b);
        if let Some(tail) = tail {
            let name = fresh_name(machine,format!("zil.scalar.select.{p}.{j}"),fuel)?;
            moved.push(m::Inst::new(Some(name.clone()),m::Op::Select { ty:tail.ty.clone(),condition:condition.clone(),yes:tail.yes,no:tail.no }));
            // Both original P->J paths executed exactly one terminal remainder.
            // Selecting its numerator and executing one shared remainder never
            // evaluates the unchosen remainder, including poison operands.
            moved.push(m::Inst::new(Some(tail.a),m::Op::Binary { opcode:tail.opcode,ty:tail.ty,left:m::local(name),right:tail.divisor }));
        }
        moved.extend(replacements);
        let join_name = blocks[j].name.clone();
        rewrite_aliases(machine,&aliases,fuel)?;
        let output = machine.blocks.as_mut()?;
        output[p].instructions.pop()?;
        output[p].instructions.extend(moved);
        output[p].instructions.push(m::Inst::new(None,m::Op::Branch(join_name)));
        output[j].instructions.retain(|inst| !matches!(inst.op,m::Op::Phi { .. }));
        let mut index = 0;
        output.retain(|_| { let keep = index != a && index != b; index += 1; keep });
        return Some(1);
    }
    Some(0)
}
fn has_unknown(value: &m::Val) -> bool {
    match value {
        m::Val::Poison | m::Val::Undef => true,
        m::Val::Array(args) => args.iter().any(|a|has_unknown(&a.value)),
        m::Val::Gep { ptr,indices,.. } => has_unknown(ptr) || indices.iter().any(|a|has_unknown(&a.value)),
        _ => false,
    }
}

pub(super) fn simplify(machine: &mut m::Function, fuel: &mut Fuel) -> Option<usize> {
    let mut changed = 0;
    for _ in 0..MAX_ROUNDS {
        let mut round = 0;
        let graph = Graph::new(machine,fuel)?;
        round += propagate(machine,&graph,fuel)?;
        round += fold_branches(machine,fuel)?;
        let graph = Graph::new(machine,fuel)?;
        round += prune(machine,&graph,fuel)?;
        round += eliminate_phis(machine,fuel)?;
        round += dead_pure(machine,fuel)?;
        let graph = Graph::new(machine,fuel)?;
        round += diamond(machine,&graph,fuel)?;
        let graph = Graph::new(machine,fuel)?;
        round += linear(machine,&graph,fuel)?;
        changed += round;
        if round == 0 {
            if changed != 0 { let _ = ssa::Cfg::new(machine,fuel)?; }
            return Some(changed);
        }
    }
    None // No partially cleaned graph escapes the enclosing transaction.
}
