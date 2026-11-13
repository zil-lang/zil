use super::*;
use zil_ir::InstructionKind as I;

pub(super) struct Cfg {
    pub(super) labels: BTreeMap<String, usize>,
    pub(super) successors: Vec<Vec<usize>>,
    pub(super) predecessors: Vec<Vec<usize>>,
    pub(super) order: Vec<usize>,
    pub(super) rank: Vec<usize>,
    intervals: Vec<(usize, usize)>,
    children: Vec<Vec<usize>>,
    frontier: Vec<BTreeSet<usize>>,
}
impl Cfg {
    pub(super) fn new(machine: &m::Function, fuel: &mut Fuel) -> Option<Self> {
        let blocks = machine.blocks.as_ref()?;
        if blocks.is_empty() {
            return None;
        }
        let mut labels = BTreeMap::new();
        let mut definitions = BTreeMap::new();
        for p in &machine.signature.params {
            fuel.take(1)?;
            if let Some(name) = &p.name
                && definitions.insert(name.clone(), None).is_some()
            {
                return None;
            }
        }
        for (b, block) in blocks.iter().enumerate() {
            fuel.take(1 + block.instructions.len())?;
            if labels.insert(block.name.clone(), b).is_some() {
                return None;
            }
            for (i, instruction) in block.instructions.iter().enumerate() {
                if instruction.op.terminator() && i + 1 != block.instructions.len() {
                    return None;
                }
                if let Some(name) = &instruction.result
                    && definitions.insert(name.clone(), Some((b, i))).is_some()
                {
                    return None;
                }
            }
        }
        let mut successors = vec![vec![]; blocks.len()];
        let mut predecessors = vec![vec![]; blocks.len()];
        for (b, block) in blocks.iter().enumerate() {
            let names = match &block.instructions.last()?.op {
                m::Op::Branch(n) => vec![n],
                m::Op::CondBranch { yes, no, .. } => vec![yes, no],
                m::Op::Return(_) | m::Op::Unreachable => vec![],
                _ => return None,
            };
            for name in names {
                let target = *labels.get(name)?;
                successors[b].push(target);
                predecessors[target].push(b); // Keep the complete edge multiset.
            }
        }
        if !predecessors[0].is_empty() {
            return None;
        }
        let mut seen = vec![false; blocks.len()];
        let mut postorder = vec![];
        let mut stack = vec![(0usize, false)];
        while let Some((b, exit)) = stack.pop() {
            fuel.take(1)?;
            if exit {
                postorder.push(b);
                continue;
            }
            if std::mem::replace(&mut seen[b], true) {
                continue;
            }
            stack.push((b, true));
            for &s in successors[b].iter().rev() {
                if !seen[s] {
                    stack.push((s, false));
                }
            }
        }
        postorder.reverse();
        let order = postorder;
        let mut rank = vec![usize::MAX; blocks.len()];
        for (r, &b) in order.iter().enumerate() {
            rank[b] = r;
        }
        let mut idom = vec![None; blocks.len()];
        idom[0] = Some(0);
        loop {
            let mut changed = false;
            for &b in order.iter().skip(1) {
                let mut incoming = predecessors[b]
                    .iter()
                    .copied()
                    .filter(|&p| idom[p].is_some());
                let mut parent = incoming.next()?;
                for mut other in incoming {
                    while parent != other {
                        fuel.take(1)?;
                        if rank[parent] > rank[other] {
                            parent = idom[parent]?;
                        } else {
                            other = idom[other]?;
                        }
                    }
                }
                if idom[b] != Some(parent) {
                    idom[b] = Some(parent);
                    changed = true;
                }
                fuel.take(1)?;
            }
            if !changed {
                break;
            }
        }
        let mut children = vec![vec![]; blocks.len()];
        for &b in order.iter().skip(1) {
            children[idom[b]?].push(b);
        }
        let mut frontier = vec![BTreeSet::new(); blocks.len()];
        for &b in &order {
            let preds = predecessors[b]
                .iter()
                .copied()
                .filter(|&p| rank[p] != usize::MAX)
                .collect::<BTreeSet<_>>();
            if preds.len() < 2 {
                continue;
            }
            for mut p in preds {
                while p != idom[b]? {
                    fuel.take(1)?;
                    frontier[p].insert(b);
                    p = idom[p]?;
                }
            }
        }
        let mut intervals = vec![(usize::MAX, usize::MAX); blocks.len()];
        let mut visits = vec![(0usize, false)];
        let mut clock = 0;
        while let Some((b, exit)) = visits.pop() {
            fuel.take(1)?;
            if exit {
                intervals[b].1 = clock;
                clock += 1;
            } else {
                intervals[b].0 = clock;
                clock += 1;
                visits.push((b, true));
                for &child in children[b].iter().rev() {
                    visits.push((child, false));
                }
            }
        }
        let cfg = Self {
            labels,
            successors,
            predecessors,
            order,
            rank,
            intervals,
            children,
            frontier,
        };
        // Existing SSA operands must remain well scoped. Unreachable components
        // are kept unchanged, but even there an absent definition is not guessed.
        for (b, block) in blocks.iter().enumerate() {
            for (i, inst) in block.instructions.iter().enumerate() {
                fuel.take(1)?;
                if let m::Op::Phi { incoming, .. } = &inst.op {
                    let mut actual = vec![];
                    let mut duplicate_values = BTreeMap::new();
                    for (value, pred) in incoming {
                        if let Some(previous) = duplicate_values.insert(pred, value)
                            && previous != value
                        {
                            return None;
                        }
                        let p = *cfg.labels.get(pred)?;
                        actual.push(p);
                        let mut valid = true;
                        visit_local(value, &mut |name| {
                            if !cfg.available(&definitions, name, p, usize::MAX) {
                                valid = false;
                            }
                        });
                        if !valid {
                            return None;
                        }
                    }
                    actual.sort_unstable();
                    let mut expected = cfg.predecessors[b].clone();
                    expected.sort_unstable();
                    if actual != expected {
                        return None;
                    }
                } else {
                    for value in inst.op.values() {
                        let mut valid = true;
                        visit_local(value, &mut |name| {
                            if !cfg.available(&definitions, name, b, i) {
                                valid = false;
                            }
                        });
                        if !valid {
                            return None;
                        }
                    }
                }
            }
        }
        Some(cfg)
    }
    fn dominates(&self, a: usize, b: usize) -> bool {
        self.rank[a] != usize::MAX
            && self.rank[b] != usize::MAX
            && self.intervals[a].0 <= self.intervals[b].0
            && self.intervals[b].1 <= self.intervals[a].1
    }
    fn available(
        &self,
        defs: &BTreeMap<String, Option<(usize, usize)>>,
        name: &str,
        b: usize,
        pos: usize,
    ) -> bool {
        match defs.get(name) {
            None => false,
            Some(None) => true,
            Some(Some((d, i))) => {
                self.rank[b] == usize::MAX || (self.dominates(*d, b) && (*d != b || *i < pos))
            }
        }
    }
}

pub(super) struct Slot {
    ty: m::Ty,
    eligible: bool,
    expected_loads: usize,
    expected_stores: usize,
}
pub(super) fn source_slots(source: &TypedFunction, fuel: &mut Fuel) -> Option<Vec<Slot>> {
    let mut slots = vec![];
    for (id, local) in source.locals.iter().enumerate() {
        fuel.take(1)?;
        if local.id.0 as usize != id {
            return None;
        }
        let ty = super::super::llvm_type(&local.ty).ok()?;
        let scalar = matches!(
            ty,
            m::Ty::Int(1..=64) | m::Ty::F32 | m::Ty::F64 | m::Ty::Ptr
        );
        slots.push(Slot {
            ty,
            eligible: scalar,
            expected_loads: 0,
            expected_stores: 0,
        });
    }
    for id in &source.parameters {
        slots.get_mut(id.0 as usize)?.expected_stores += 1;
    }
    for block in &source.blocks {
        for inst in &block.instructions {
            fuel.take(1)?;
            match &inst.kind {
                I::Load(id) => slots.get_mut(id.0 as usize)?.expected_loads += 1,
                I::Store { local, .. } => slots.get_mut(local.0 as usize)?.expected_stores += 1,
                I::BorrowLocal { local } | I::BorrowLocalMut { local } => {
                    slots.get_mut(local.0 as usize)?.eligible = false
                }
                // Scalar source lifetime fences do not emit machine lifetime
                // operations. Unknown machine lifetime/address uses still reject.
                I::StorageLive(_)
                | I::StorageDead(_)
                | I::Constant(_)
                | I::ConvertInteger { .. }
                | I::Unary { .. }
                | I::Binary { .. }
                | I::Call { .. }
                | I::ForeignCall { .. }
                | I::FunctionRef { .. }
                | I::ForeignFunctionRef { .. }
                | I::CallIndirect { .. }
                | I::UnsafeEnter { .. }
                | I::UnsafeExit { .. }
                | I::ScopeEnter { .. }
                | I::ScopeExit { .. } => {}
                _ => return None,
            }
        }
    }
    Some(slots)
}

pub(super) fn promote(
    machine: &mut m::Function,
    cfg: &Cfg,
    mut slots: Vec<Slot>,
    fuel: &mut Fuel,
) -> Option<usize> {
    let blocks = machine.blocks.as_ref()?;
    let mut counts = vec![(0usize, 0usize, 0usize); slots.len()];
    let mut names = BTreeSet::new();
    let mut load_slots = BTreeMap::new();
    for p in &machine.signature.params {
        if let Some(n) = &p.name {
            names.insert(n.clone());
        }
    }
    for (b, block) in blocks.iter().enumerate() {
        for inst in &block.instructions {
            fuel.take(1)?;
            if let Some(name) = &inst.result {
                names.insert(name.clone());
                if let Some(s) = canonical_slot(name).filter(|&s| s < slots.len()) {
                    counts[s].0 += 1;
                    if b != 0 || !matches!(&inst.op, m::Op::Alloca { ty, .. } if *ty == slots[s].ty)
                    {
                        slots[s].eligible = false;
                    }
                }
            }
            match &inst.op {
                m::Op::Load {
                    ty,
                    ptr: m::Val::Local(name),
                    ..
                } if canonical_slot(name).is_some_and(|s| s < slots.len()) => {
                    let s = canonical_slot(name)?;
                    counts[s].1 += 1;
                    if *ty != slots[s].ty || cfg.rank[b] == usize::MAX {
                        slots[s].eligible = false;
                    }
                    load_slots.insert(inst.result.as_ref()?.clone(), s);
                }
                m::Op::Store {
                    arg,
                    ptr: m::Val::Local(name),
                    ..
                } if canonical_slot(name).is_some_and(|s| s < slots.len()) => {
                    let s = canonical_slot(name)?;
                    counts[s].2 += 1;
                    if arg.ty != slots[s].ty || !arg.attrs.is_empty() || cfg.rank[b] == usize::MAX {
                        slots[s].eligible = false;
                    }
                    visit_local(&arg.value, &mut |n| {
                        if let Some(s) = canonical_slot(n).filter(|&s| s < slots.len()) {
                            slots[s].eligible = false;
                        }
                    });
                }
                _ => {
                    for v in inst.op.values() {
                        visit_local(v, &mut |n| {
                            if let Some(s) = canonical_slot(n).filter(|&s| s < slots.len()) {
                                slots[s].eligible = false;
                            }
                        });
                    }
                }
            }
        }
    }
    // Do not remove a definition that an untouched unreachable component uses.
    for (b, block) in blocks.iter().enumerate() {
        if cfg.rank[b] != usize::MAX {
            continue;
        }
        for inst in &block.instructions {
            for v in inst.op.values() {
                visit_local(v, &mut |n| {
                    if let Some(&s) = load_slots.get(n) {
                        slots[s].eligible = false;
                    }
                });
            }
        }
    }
    for (s, &(a, l, st)) in slots.iter_mut().zip(&counts) {
        s.eligible &= a == 1 && l == s.expected_loads && st == s.expected_stores;
    }
    let n = blocks.len();
    let mut phis = vec![BTreeMap::<usize, String>::new(); n];
    for (s, slot) in slots.iter_mut().enumerate() {
        if !slot.eligible {
            continue;
        }
        let mut defs = vec![false; n];
        let mut before = vec![false; n];
        for &b in &cfg.order {
            for inst in &blocks[b].instructions {
                fuel.take(1)?;
                match &inst.op {
                    m::Op::Store {
                        ptr: m::Val::Local(name),
                        ..
                    } if canonical_slot(name) == Some(s) => defs[b] = true,
                    m::Op::Load {
                        ptr: m::Val::Local(name),
                        ..
                    } if canonical_slot(name) == Some(s) && !defs[b] => before[b] = true,
                    _ => {}
                }
            }
        }
        // Must-initialization is a greatest fixed point rooted at entry=false.
        // It excludes loads with any path lacking a prior store; no undef added.
        let mut out = vec![true; n];
        out[0] = defs[0];
        loop {
            let mut changed = false;
            for &b in &cfg.order {
                fuel.take(1)?;
                let input = b != 0
                    && cfg.predecessors[b]
                        .iter()
                        .all(|&p| cfg.rank[p] == usize::MAX || out[p]);
                let next = input || defs[b];
                if next != out[b] {
                    out[b] = next;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        if cfg.order.iter().any(|&b| {
            before[b]
                && (b == 0
                    || !cfg.predecessors[b]
                        .iter()
                        .all(|&p| cfg.rank[p] == usize::MAX || out[p]))
        }) {
            slot.eligible = false;
            continue;
        }
        let mut live = before.clone();
        let mut queue = cfg
            .order
            .iter()
            .copied()
            .filter(|&b| before[b])
            .collect::<Vec<_>>();
        while let Some(b) = queue.pop() {
            for &p in &cfg.predecessors[b] {
                fuel.take(1)?;
                if cfg.rank[p] != usize::MAX && !defs[p] && !live[p] {
                    live[p] = true;
                    queue.push(p);
                }
            }
        }
        let mut queued = defs.clone();
        let mut queue = cfg
            .order
            .iter()
            .copied()
            .filter(|&b| defs[b])
            .collect::<Vec<_>>();
        while let Some(b) = queue.pop() {
            for &join in &cfg.frontier[b] {
                fuel.take(1)?;
                if !live[join] || phis[join].contains_key(&s) {
                    continue;
                }
                let base = format!("zil.scalar.phi.{s}.{join}");
                let mut name = base.clone();
                let mut suffix = 0usize;
                while names.contains(&name) {
                    fuel.take(1)?;
                    suffix += 1;
                    name = format!("{base}.{suffix}");
                }
                names.insert(name.clone());
                phis[join].insert(s, name);
                if !queued[join] {
                    queued[join] = true;
                    queue.push(join);
                }
            }
        }
    }
    let promoted = slots.iter().filter(|s| s.eligible).count();
    if promoted == 0 {
        return Some(0);
    }
    let mut current = vec![None::<m::Val>; slots.len()];
    let mut aliases = BTreeMap::new();
    let mut incoming = phis
        .iter()
        .map(|p| {
            p.keys()
                .map(|&s| (s, vec![]))
                .collect::<BTreeMap<_, Vec<(m::Val, String)>>>()
        })
        .collect::<Vec<_>>();
    enum Visit {
        Enter(usize),
        Exit(Vec<(usize, Option<m::Val>)>),
    }
    let mut visits = vec![Visit::Enter(0)];
    while let Some(visit) = visits.pop() {
        match visit {
            Visit::Exit(saved) => {
                for (s, old) in saved.into_iter().rev() {
                    current[s] = old;
                }
            }
            Visit::Enter(b) => {
                let mut saved = vec![];
                for (&s, name) in &phis[b] {
                    saved.push((s, current[s].replace(m::local(name.clone()))));
                }
                for inst in &blocks[b].instructions {
                    fuel.take(1)?;
                    match &inst.op {
                        m::Op::Store {
                            arg,
                            ptr: m::Val::Local(name),
                            ..
                        } => {
                            if let Some(s) = canonical_slot(name)
                                .filter(|&s| s < slots.len() && slots[s].eligible)
                            {
                                saved.push((s, current[s].replace(arg.value.clone())));
                            }
                        }
                        m::Op::Load {
                            ptr: m::Val::Local(name),
                            ..
                        } => {
                            if let Some(s) = canonical_slot(name)
                                .filter(|&s| s < slots.len() && slots[s].eligible)
                            {
                                aliases.insert(inst.result.as_ref()?.clone(), current[s].clone()?);
                            }
                        }
                        _ => {}
                    }
                }
                for &succ in &cfg.successors[b] {
                    for &s in phis[succ].keys() {
                        incoming[succ]
                            .get_mut(&s)?
                            .push((current[s].clone()?, blocks[b].name.clone()));
                    }
                }
                visits.push(Visit::Exit(saved));
                for &child in cfg.children[b].iter().rev() {
                    visits.push(Visit::Enter(child));
                }
            }
        }
    }
    let output = machine.blocks.as_mut()?;
    for (b, block) in output.iter_mut().enumerate() {
        let mut prefix = vec![];
        for (&s, name) in &phis[b] {
            let mut values = std::mem::take(incoming[b].get_mut(&s)?);
            values.sort_by_key(|(_, pred)| cfg.labels[pred]);
            if values.len() != cfg.predecessors[b].len() {
                return None;
            }
            prefix.push(m::Inst::new(
                Some(name.clone()),
                m::Op::Phi {
                    ty: slots[s].ty.clone(),
                    incoming: values,
                },
            ));
        }
        let old = std::mem::take(&mut block.instructions);
        for inst in old {
            let slot = match &inst.op {
                m::Op::Alloca { .. } => inst.result.as_deref().and_then(canonical_slot),
                m::Op::Load {
                    ptr: m::Val::Local(name),
                    ..
                }
                | m::Op::Store {
                    ptr: m::Val::Local(name),
                    ..
                } => canonical_slot(name),
                _ => None,
            };
            if slot.is_some_and(|s| s < slots.len() && slots[s].eligible) {
                continue;
            }
            prefix.push(inst);
        }
        block.instructions = prefix;
    }
    rewrite_aliases(machine, &aliases, fuel)?;
    Some(promoted)
}
