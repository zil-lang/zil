//! Preflight for LLVMBuilder inputs. This checks machine types, symbols, SSA,
//! CFG and supported operation shapes; it does NOT prove ZIL memory safety.
use super::model::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

type Result<T> = std::result::Result<T, String>;
#[derive(Clone, Debug)]
pub(crate) struct Schedule {
    /// Per-function positions in dependency order, preserving each block's order.
    #[cfg_attr(not(feature = "native"), allow(dead_code))]
    pub functions: BTreeMap<String, Vec<(usize, usize)>>,
}
#[derive(Clone, Debug)]
struct Definition {
    ty: Ty,
    place: Option<(usize, usize)>,
}
fn fail<T>(s: impl Into<String>) -> Result<T> {
    Err(s.into())
}
fn identifier(s: &str) -> bool {
    let mut b = s.bytes();
    let Some(first) = b.next() else { return false };
    (first.is_ascii_alphabetic() || b"._$-".contains(&first))
        && b.all(|x| x.is_ascii_alphanumeric() || b"._$-".contains(&x))
}
fn attrs(a: &[Attr], ty: Option<&Ty>) -> Result<()> {
    let mut seen = BTreeSet::new();
    for x in a {
        if !seen.insert(x.name()) {
            return fail("duplicate machine ABI attribute");
        }
        match (x, ty) {
            (Attr::ZeroExt, Some(Ty::Int(1 | 8 | 16 | 32 | 64))) => {}
            (Attr::NonLazyBind, None) => {}
            _ => return fail("attribute is invalid for its machine ABI position"),
        }
    }
    Ok(())
}
fn alignment(a: u32) -> Result<()> {
    if a == 0 || (a.is_power_of_two() && a <= 1 << 29) {
        Ok(())
    } else {
        fail("invalid machine alignment")
    }
}
struct World<'a> {
    named: BTreeMap<&'a str, &'a Vec<Ty>>,
    symbols: BTreeMap<&'a str, &'a Item>,
}
impl<'a> World<'a> {
    fn ty(
        &self,
        t: &Ty,
        allow_void: bool,
        stack: &mut BTreeSet<String>,
        depth: usize,
    ) -> Result<()> {
        if depth > 128 {
            return fail("machine type nesting exceeds supported limit");
        }
        match t {
            Ty::Void if allow_void => {}
            Ty::Void => return fail("void is not a stored/parameter machine type"),
            Ty::Int(1 | 8 | 16 | 32 | 64) | Ty::Ptr => {}
            Ty::Int(_) => return fail("unsupported machine integer width"),
            Ty::Array(n, element) => {
                if *n > u32::MAX as u64 {
                    return fail("machine array length exceeds current profile");
                }
                self.ty(element, false, stack, depth + 1)?;
            }
            Ty::Struct(fields) => {
                if fields.len() > u32::MAX as usize {
                    return fail("too many struct fields");
                }
                for f in fields {
                    self.ty(f, false, stack, depth + 1)?;
                }
            }
            Ty::Named(name) => {
                if !identifier(name) {
                    return fail("invalid named machine type");
                }
                let fields = self
                    .named
                    .get(name.as_str())
                    .ok_or_else(|| format!("undefined type {name}"))?;
                if !stack.insert(name.clone()) {
                    return fail("recursive inline machine layout");
                }
                for f in fields.iter() {
                    self.ty(f, false, stack, depth + 1)?;
                }
                stack.remove(name);
            }
        }
        Ok(())
    }
    fn check_ty(&self, t: &Ty, void: bool) -> Result<()> {
        self.ty(t, void, &mut BTreeSet::new(), 0)
    }
    fn fields<'b>(&'b self, t: &'b Ty) -> Result<&'b [Ty]> {
        match t {
            Ty::Struct(fields) => Ok(fields),
            Ty::Named(name) => self
                .named
                .get(name.as_str())
                .map(|f| f.as_slice())
                .ok_or_else(|| format!("undefined aggregate {name}")),
            _ => fail("operation requires struct/named aggregate"),
        }
    }
    fn indexed(&self, t: &Ty, index: u32) -> Result<Ty> {
        match t {
            Ty::Array(n, element) if u64::from(index) < *n => Ok((**element).clone()),
            Ty::Array(..) => fail("aggregate index outside machine array"),
            _ => self
                .fields(t)?
                .get(index as usize)
                .cloned()
                .ok_or_else(|| "aggregate index outside struct".into()),
        }
    }
    fn gep(
        &self,
        t: &Ty,
        indices: &[Arg],
        defs: &BTreeMap<String, Definition>,
        constant: bool,
    ) -> Result<()> {
        self.check_ty(t, false)?;
        if indices.is_empty() {
            return fail("GEP needs a pointer index");
        }
        let mut current = t.clone();
        for (i, a) in indices.iter().enumerate() {
            if !matches!(a.ty, Ty::Int(_)) {
                return fail("GEP index is not integer");
            }
            self.arg(a, defs, constant)?;
            if i == 0 {
                continue;
            }
            current = match current {
                Ty::Array(_, element) => *element,
                Ty::Struct(_) | Ty::Named(_) => {
                    if a.ty != Ty::Int(32) {
                        return fail("struct GEP index must be i32");
                    }
                    let Val::Int(n) = a.value else {
                        return fail("struct GEP index must be literal");
                    };
                    if n < 0 || n > u32::MAX as i128 {
                        return fail("invalid struct GEP index");
                    }
                    self.indexed(&current, n as u32)?
                }
                _ => return fail("GEP traverses a non-aggregate"),
            };
        }
        Ok(())
    }
    fn arg(&self, a: &Arg, defs: &BTreeMap<String, Definition>, constant: bool) -> Result<()> {
        self.check_ty(&a.ty, false)?;
        attrs(&a.attrs, Some(&a.ty))?;
        self.value(&a.value, &a.ty, defs, constant, 0)
    }
    fn value(
        &self,
        v: &Val,
        t: &Ty,
        defs: &BTreeMap<String, Definition>,
        constant: bool,
        depth: usize,
    ) -> Result<()> {
        if depth > 128 {
            return fail("constant nesting exceeds supported limit");
        }
        match v {
            Val::Local(name) => {
                if constant {
                    return fail("local reference in global/constant initializer");
                }
                let d = defs
                    .get(name)
                    .ok_or_else(|| format!("undefined SSA value %{name}"))?;
                if d.ty != *t {
                    return fail(format!("SSA type mismatch for %{name}: {} != {t}", d.ty));
                }
            }
            Val::Global(name) => {
                if *t != Ty::Ptr {
                    return fail("global address requires ptr type");
                }
                if !self.symbols.contains_key(name.as_str()) {
                    return fail(format!("undefined symbol @{name}"));
                }
            }
            Val::Int(n) => {
                let Ty::Int(bits) = t else {
                    return fail("integer constant has non-integer type");
                };
                // LLVM integers are signless; accept the signed and unsigned ranges.
                let low = -(1i128 << (*bits - 1));
                let high = (1i128 << *bits) - 1;
                if *n < low || *n > high {
                    return fail("machine integer constant out of range");
                }
            }
            Val::Bool(_) if *t == Ty::Int(1) => {}
            Val::Bool(_) => return fail("boolean constant is not i1"),
            Val::Null if *t == Ty::Ptr => {}
            Val::Null => return fail("null has non-pointer type"),
            Val::Zero | Val::Undef | Val::Poison => {
                self.check_ty(t, false)?;
            }
            Val::Array(values) => {
                let Ty::Array(n, element) = t else {
                    return fail("array initializer has non-array type");
                };
                if *n != values.len() as u64 {
                    return fail("array initializer length mismatch");
                }
                for a in values {
                    if a.ty != **element {
                        return fail("array initializer element type mismatch");
                    }
                    if !a.attrs.is_empty() {
                        return fail("attribute on constant aggregate element");
                    }
                    self.value(&a.value, element, defs, true, depth + 1)?;
                }
            }
            Val::Bytes(bytes) => {
                if *t != Ty::array(bytes.len() as u64, Ty::Int(8)) {
                    return fail("byte initializer type mismatch");
                }
            }
            Val::Gep { ty, ptr, indices } => {
                if *t != Ty::Ptr {
                    return fail("constant GEP is not ptr");
                }
                self.value(ptr, &Ty::Ptr, defs, true, depth + 1)?;
                self.gep(ty, indices, defs, true)?;
            }
            Val::Invalid(atom) => return fail(format!("invalid lowering operand: {atom}")),
        }
        Ok(())
    }
    fn result(&self, op: &Op) -> Result<Option<Ty>> {
        Ok(match op {
            Op::Alloca { .. } | Op::Gep { .. } => Some(Ty::Ptr),
            Op::Load { ty, .. } | Op::Binary { ty, .. } | Op::Phi { ty, .. } => Some(ty.clone()),
            Op::Compare { .. } => Some(Ty::Int(1)),
            Op::Cast { to, .. } => Some(to.clone()),
            Op::Extract { arg, index } => Some(self.indexed(&arg.ty, *index)?),
            Op::Insert { arg, .. } | Op::Freeze(arg) => Some(arg.ty.clone()),
            Op::Call {
                result: Ty::Void, ..
            } => None,
            Op::Call { result, .. } => Some(result.clone()),
            _ => None,
        })
    }
    fn op(
        &self,
        op: &Op,
        defs: &BTreeMap<String, Definition>,
        signature: &Signature,
    ) -> Result<()> {
        let value = |v: &Val, t: &Ty| self.value(v, t, defs, false, 0);
        match op {
            Op::Alloca { ty, align } => {
                self.check_ty(ty, false)?;
                alignment(*align)?;
            }
            Op::Load { ty, ptr, align } => {
                self.check_ty(ty, false)?;
                value(ptr, &Ty::Ptr)?;
                alignment(*align)?;
            }
            Op::Store { arg, ptr, align } => {
                self.arg(arg, defs, false)?;
                value(ptr, &Ty::Ptr)?;
                alignment(*align)?;
            }
            Op::Binary {
                opcode,
                ty,
                left,
                right,
            } => {
                if !matches!(
                    opcode.as_str(),
                    "add"
                        | "sub"
                        | "mul"
                        | "sdiv"
                        | "udiv"
                        | "srem"
                        | "urem"
                        | "and"
                        | "or"
                        | "xor"
                        | "shl"
                        | "lshr"
                        | "ashr"
                ) {
                    return fail(format!("unsupported binary opcode {opcode}"));
                }
                if !matches!(ty, Ty::Int(_)) {
                    return fail("binary operands must be integers");
                }
                self.check_ty(ty, false)?;
                value(left, ty)?;
                value(right, ty)?;
            }
            Op::Compare {
                predicate,
                ty,
                left,
                right,
            } => {
                if !matches!(
                    predicate.as_str(),
                    "eq" | "ne" | "ugt" | "uge" | "ult" | "ule" | "sgt" | "sge" | "slt" | "sle"
                ) {
                    return fail("unsupported integer comparison");
                }
                if !matches!(ty, Ty::Int(_) | Ty::Ptr) {
                    return fail("comparison requires integer/pointer");
                }
                self.check_ty(ty, false)?;
                value(left, ty)?;
                value(right, ty)?;
            }
            Op::Cast { opcode, arg, to } => {
                self.arg(arg, defs, false)?;
                self.check_ty(to, false)?;
                let valid = match (opcode.as_str(), &arg.ty, to) {
                    ("zext" | "sext", Ty::Int(a), Ty::Int(b)) => a < b,
                    ("trunc", Ty::Int(a), Ty::Int(b)) => a > b,
                    ("ptrtoint", Ty::Ptr, Ty::Int(_)) => true,
                    _ => false,
                };
                if !valid {
                    return fail("invalid machine cast");
                }
            }
            Op::Gep { ty, ptr, indices } => {
                value(ptr, &Ty::Ptr)?;
                self.gep(ty, indices, defs, false)?;
            }
            Op::Extract { arg, index } => {
                self.arg(arg, defs, false)?;
                self.indexed(&arg.ty, *index)?;
            }
            Op::Insert { arg, field, index } => {
                self.arg(arg, defs, false)?;
                self.arg(field, defs, false)?;
                if self.indexed(&arg.ty, *index)? != field.ty {
                    return fail("insertvalue field type mismatch");
                }
            }
            Op::Freeze(arg) => self.arg(arg, defs, false)?,
            Op::Call {
                result,
                result_attrs,
                callee,
                args,
                vararg,
            } => {
                self.check_ty(result, true)?;
                attrs(result_attrs, Some(result))?;
                value(callee, &Ty::Ptr)?;
                if args.len() > u32::MAX as usize {
                    return fail("too many call arguments");
                }
                for a in args {
                    self.arg(a, defs, false)?;
                }
                if let Val::Global(name) = callee {
                    let Some(Item::Function(f)) = self.symbols.get(name.as_str()) else {
                        return fail("calling a global that is not a function");
                    };
                    let sig = &f.signature;
                    if sig.result != *result
                        || sig.result_attrs != *result_attrs
                        || args.len() < sig.params.len()
                        || (!sig.variadic && args.len() != sig.params.len())
                    {
                        return fail(format!("call signature mismatch for @{name}"));
                    }
                    if sig.variadic != vararg.is_some() {
                        return fail("variadic call signature missing/mismatched");
                    }
                    if let Some(ts) = vararg
                        && *ts != sig.params.iter().map(|p| p.ty.clone()).collect::<Vec<_>>()
                    {
                        return fail("variadic fixed parameter mismatch");
                    }
                    for (p, a) in sig.params.iter().zip(args) {
                        if p.ty != a.ty || p.attrs != a.attrs {
                            return fail(format!("call parameter ABI mismatch for @{name}"));
                        }
                    }
                } else if vararg.is_some() {
                    return fail("indirect variadic calls are outside current profile");
                }
            }
            Op::Phi { ty, incoming } => {
                self.check_ty(ty, false)?;
                for (v, _) in incoming {
                    value(v, ty)?;
                }
            }
            Op::Branch(_) => {}
            Op::CondBranch { condition, .. } => value(condition, &Ty::Int(1))?,
            Op::Return(None) => {
                if signature.result != Ty::Void {
                    return fail("missing return value");
                }
            }
            Op::Return(Some(a)) => {
                if a.ty != signature.result {
                    return fail("return type mismatch");
                }
                self.arg(a, defs, false)?;
            }
            Op::Unreachable => {}
        }
        Ok(())
    }
}
fn successors(op: &Op) -> Vec<&str> {
    match op {
        Op::Branch(b) => vec![b],
        Op::CondBranch { yes, no, .. } => vec![yes, no],
        _ => vec![],
    }
}
/// Immediate dominators in reverse postorder. No CFG×SSA/variable Cartesian table.
fn dominators(succ: &[Vec<usize>], pred: &[Vec<usize>]) -> (Vec<bool>, Vec<usize>) {
    let n = succ.len();
    let mut seen = vec![false; n];
    let mut post = vec![];
    let mut stack = vec![(0usize, false)];
    while let Some((b, exit)) = stack.pop() {
        if exit {
            post.push(b);
            continue;
        }
        if seen[b] {
            continue;
        }
        seen[b] = true;
        stack.push((b, true));
        for &next in succ[b].iter().rev() {
            if !seen[next] {
                stack.push((next, false));
            }
        }
    }
    post.reverse();
    let mut rank = vec![usize::MAX; n];
    for (i, &b) in post.iter().enumerate() {
        rank[b] = i;
    }
    let mut idom = vec![usize::MAX; n];
    idom[0] = 0;
    let intersect = |mut a: usize, mut b: usize, idom: &[usize]| {
        while a != b {
            while rank[a] > rank[b] {
                a = idom[a];
            }
            while rank[b] > rank[a] {
                b = idom[b];
            }
        }
        a
    };
    let mut changed = true;
    while changed {
        changed = false;
        for &b in post.iter().skip(1) {
            let mut p = pred[b].iter().copied().filter(|&p| idom[p] != usize::MAX);
            let Some(mut next) = p.next() else { continue };
            for p in p {
                next = intersect(next, p, &idom);
            }
            if next != idom[b] {
                idom[b] = next;
                changed = true;
            }
        }
    }
    (seen, idom)
}
fn function(world: &World<'_>, f: &Function) -> Result<Vec<(usize, usize)>> {
    let Some(blocks) = &f.blocks else {
        return Ok(vec![]);
    };
    if blocks.is_empty() {
        return fail("function has no entry block");
    }
    if f.signature.variadic {
        return fail("variadic definition not supported by machine lowering");
    }
    let n = blocks.len();
    let mut labels = BTreeMap::new();
    let mut defs = BTreeMap::new();
    for p in &f.signature.params {
        let name = p
            .name
            .as_deref()
            .ok_or("definition parameter has no name")?;
        if !identifier(name)
            || defs
                .insert(
                    name.to_owned(),
                    Definition {
                        ty: p.ty.clone(),
                        place: None,
                    },
                )
                .is_some()
        {
            return fail("invalid/duplicate parameter name");
        }
    }
    for (b, block) in blocks.iter().enumerate() {
        if !identifier(&block.name)
            || labels.insert(block.name.clone(), b).is_some()
            || defs.contains_key(&block.name)
        {
            return fail("invalid/duplicate block label");
        }
        if block.instructions.is_empty() || !block.instructions.last().unwrap().op.terminator() {
            return fail(format!("unterminated block {}", block.name));
        }
        let mut non_phi = false;
        for (j, i) in block.instructions.iter().enumerate() {
            if j + 1 != block.instructions.len() && i.op.terminator() {
                return fail("instruction after terminator");
            }
            if matches!(i.op, Op::Phi { .. }) {
                if non_phi {
                    return fail("phi not at block start");
                }
            } else {
                non_phi = true;
            }
            if let Some(name) = &i.result {
                let ty = world.result(&i.op)?.ok_or("named void instruction")?;
                world.check_ty(&ty, false)?;
                if !identifier(name)
                    || defs
                        .insert(
                            name.clone(),
                            Definition {
                                ty,
                                place: Some((b, j)),
                            },
                        )
                        .is_some()
                {
                    return fail(format!("invalid/duplicate SSA definition %{name}"));
                }
            }
        }
    }
    // Block labels and named SSA values share the same LLVM local namespace.
    if labels.keys().any(|label| defs.contains_key(label)) {
        return fail("block label collides with SSA name");
    }
    let mut succ = vec![vec![]; n];
    let mut pred = vec![vec![]; n];
    for (b, block) in blocks.iter().enumerate() {
        for next in successors(&block.instructions.last().unwrap().op) {
            let id = *labels
                .get(next)
                .ok_or_else(|| format!("unknown block %{next}"))?;
            succ[b].push(id);
            pred[id].push(b);
        }
    }
    if !pred[0].is_empty() {
        return fail("entry block has a predecessor");
    }
    let (reachable, idom) = dominators(&succ, &pred);
    let dominates = |a: usize, mut b: usize| {
        if !reachable[b] {
            return true;
        }
        if !reachable[a] {
            return false;
        }
        loop {
            if a == b {
                return true;
            }
            if b == 0 {
                return false;
            }
            b = idom[b];
        }
    };
    for (b, block) in blocks.iter().enumerate() {
        for (j, i) in block.instructions.iter().enumerate() {
            world
                .op(&i.op, &defs, &f.signature)
                .map_err(|s| format!("@{} / {} / {}: {s}", f.name, block.name, j))?;
            if let Op::Phi { incoming, .. } = &i.op {
                let mut incoming_ids = vec![];
                for (v, p) in incoming {
                    let p = *labels.get(p).ok_or("phi incoming block missing")?;
                    incoming_ids.push(p);
                    let mut uses = vec![];
                    v.local_uses(&mut uses);
                    for name in uses {
                        if let Some((d, _)) = defs[name].place
                            && !dominates(d, p)
                        {
                            return fail(format!(
                                "phi source %{name} does not dominate predecessor"
                            ));
                        }
                    }
                }
                incoming_ids.sort_unstable();
                let mut expected = pred[b].clone();
                expected.sort_unstable();
                if incoming_ids != expected {
                    return fail("phi incoming edges do not match CFG predecessor multiset");
                }
                for (idx, (v, p)) in incoming.iter().enumerate() {
                    if incoming[..idx]
                        .iter()
                        .any(|(old, block)| block == p && old != v)
                    {
                        return fail(
                            "duplicate phi edges from a predecessor carry different values",
                        );
                    }
                }
            } else {
                for value in i.op.values() {
                    let mut uses = vec![];
                    value.local_uses(&mut uses);
                    for name in uses {
                        if let Some((d, pos)) = defs[name].place
                            && ((d == b && pos >= j) || (d != b && !dominates(d, b)))
                        {
                            return fail(format!(
                                "SSA definition %{name} does not dominate its use"
                            ));
                        }
                    }
                }
            }
        }
    }
    // Phi nodes are created first. Sort all other instructions by operand and
    // per-block sequencing edges, so forward textual block order is irrelevant.
    let mut positions = vec![];
    let mut ids = BTreeMap::new();
    for (b, block) in blocks.iter().enumerate() {
        for (j, i) in block.instructions.iter().enumerate() {
            if !matches!(i.op, Op::Phi { .. }) {
                ids.insert((b, j), positions.len());
                positions.push((b, j));
            }
        }
    }
    let mut counts = vec![0; positions.len()];
    let mut users = vec![vec![]; positions.len()];
    for (id, &(b, j)) in positions.iter().enumerate() {
        let mut dependencies = BTreeSet::new();
        if let Some(prev) = j.checked_sub(1).and_then(|p| ids.get(&(b, p))) {
            dependencies.insert(*prev);
        }
        for v in blocks[b].instructions[j].op.values() {
            let mut uses = vec![];
            v.local_uses(&mut uses);
            for name in uses {
                if let Some(pos) = defs[name].place
                    && let Some(&source) = ids.get(&pos)
                {
                    dependencies.insert(source);
                }
            }
        }
        counts[id] = dependencies.len();
        for source in dependencies {
            users[source].push(id);
        }
    }
    let mut ready = VecDeque::new();
    for (id, &count) in counts.iter().enumerate() {
        if count == 0 {
            ready.push_back(id);
        }
    }
    let mut schedule = vec![];
    while let Some(id) = ready.pop_front() {
        schedule.push(positions[id]);
        for &user in &users[id] {
            counts[user] -= 1;
            if counts[user] == 0 {
                ready.push_back(user);
            }
        }
    }
    if schedule.len() != positions.len() {
        return fail("cycle in non-phi SSA or instruction sequence");
    }
    Ok(schedule)
}

pub(crate) fn validate(module: &Module) -> Result<Schedule> {
    if !matches!(
        module.triple.as_str(),
        "x86_64-unknown-linux-gnu" | "x86_64-pc-windows-msvc"
    ) {
        return fail("machine module target is not supported");
    }
    let mut world = World {
        named: BTreeMap::new(),
        symbols: BTreeMap::new(),
    };
    for item in &module.items {
        match item {
            Item::NamedType(n, fields) => {
                if !identifier(n) || world.named.insert(n, fields).is_some() {
                    return fail("duplicate/invalid type name");
                }
            }
            Item::Global(Global { name, .. }) | Item::Function(Function { name, .. }) => {
                if !identifier(name) || world.symbols.insert(name, item).is_some() {
                    return fail(format!("duplicate/invalid global symbol {name}"));
                }
            }
        }
    }
    let mut functions = BTreeMap::new();
    for item in &module.items {
        match item {
            Item::NamedType(n, _) => world.check_ty(&Ty::Named(n.clone()), false)?,
            Item::Global(g) => {
                world.check_ty(&g.ty, false)?;
                alignment(g.alignment)?;
                if let Some(v) = &g.initializer {
                    world.value(v, &g.ty, &BTreeMap::new(), true, 0)?;
                } else if g.linkage.is_local() {
                    return fail("local global declaration without definition");
                }
            }
            Item::Function(f) => {
                world.check_ty(&f.signature.result, true)?;
                attrs(&f.signature.result_attrs, Some(&f.signature.result))?;
                attrs(&f.attrs, None)?;
                if f.signature.params.len() > u32::MAX as usize {
                    return fail("too many machine parameters");
                }
                for p in &f.signature.params {
                    world.check_ty(&p.ty, false)?;
                    attrs(&p.attrs, Some(&p.ty))?;
                }
                if f.blocks.is_none() && f.linkage.is_local() {
                    return fail("local function without definition");
                }
                functions.insert(f.name.clone(), function(&world, f)?);
            }
        }
    }
    Ok(Schedule { functions })
}
