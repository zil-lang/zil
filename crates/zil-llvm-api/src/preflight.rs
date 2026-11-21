//! Preflight for LLVMBuilder inputs. This checks machine types, symbols, SSA,
//! CFG and supported operation shapes; it does NOT prove ZIL memory safety.
use super::model::*;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

type Result<T> = std::result::Result<T, String>;
/// Optional request-local work sink. Existing standalone/native validation uses
/// the same algorithm without a caller ledger; preparation supplies its ledger.
#[derive(Clone, Copy, Default)]
struct Meter<'a>(Option<&'a dyn Fn(usize) -> Result<()>>);
impl Meter<'_> {
    fn pay(self, work: usize) -> Result<()> {
        if let Some(charge) = self.0 {
            charge(work)?;
        }
        Ok(())
    }
    fn sort(self, count: usize) -> Result<()> {
        self.pay(count.saturating_mul(count.max(1).ilog2() as usize + 1))
    }
    fn same_name(self, a: &str, b: &str) -> Result<bool> {
        self.pay(1)?;
        if a.len() != b.len() {
            return Ok(false);
        }
        self.pay(a.len())?;
        Ok(a == b)
    }
    fn same_type(self, a: &Ty, b: &Ty) -> Result<bool> {
        if self.0.is_none() {
            return Ok(a == b);
        }
        self.pay(1)?;
        Ok(match (a, b) {
            (Ty::Void, Ty::Void) | (Ty::Ptr, Ty::Ptr) | (Ty::F32, Ty::F32) | (Ty::F64, Ty::F64) => {
                true
            }
            (Ty::Int(a), Ty::Int(b)) => a == b,
            (Ty::Named(a), Ty::Named(b)) => self.same_name(a, b)?,
            (Ty::Array(n, a), Ty::Array(m, b)) => n == m && self.same_type(a, b)?,
            (Ty::Struct(a), Ty::Struct(b)) => {
                if a.len() != b.len() {
                    return Ok(false);
                }
                for (a, b) in a.iter().zip(b) {
                    if !self.same_type(a, b)? {
                        return Ok(false);
                    }
                }
                true
            }
            _ => false,
        })
    }
    fn same_arg(self, a: &Arg, b: &Arg) -> Result<bool> {
        if !self.same_type(&a.ty, &b.ty)? || !self.same_value(&a.value, &b.value)? {
            return Ok(false);
        }
        self.pay(a.attrs.len().saturating_add(b.attrs.len()))?;
        Ok(a.attrs == b.attrs)
    }
    fn same_args(self, a: &[Arg], b: &[Arg]) -> Result<bool> {
        self.pay(1)?;
        if a.len() != b.len() {
            return Ok(false);
        }
        for (a, b) in a.iter().zip(b) {
            if !self.same_arg(a, b)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn same_value(self, a: &Val, b: &Val) -> Result<bool> {
        if self.0.is_none() {
            return Ok(a == b);
        }
        self.pay(1)?;
        Ok(match (a, b) {
            (Val::Local(a), Val::Local(b))
            | (Val::Global(a), Val::Global(b))
            | (Val::Invalid(a), Val::Invalid(b)) => self.same_name(a, b)?,
            (Val::Int(a), Val::Int(b)) => a == b,
            (Val::F32(a), Val::F32(b)) => a == b,
            (Val::F64(a), Val::F64(b)) => a == b,
            (Val::Bool(a), Val::Bool(b)) => a == b,
            (Val::Null, Val::Null)
            | (Val::Zero, Val::Zero)
            | (Val::Undef, Val::Undef)
            | (Val::Poison, Val::Poison) => true,
            (Val::Array(a), Val::Array(b)) => self.same_args(a, b)?,
            (Val::Bytes(a), Val::Bytes(b)) => {
                if a.len() != b.len() {
                    return Ok(false);
                }
                self.pay(a.len())?;
                a == b
            }
            (
                Val::Gep {
                    ty: a,
                    ptr: ap,
                    indices: ai,
                },
                Val::Gep {
                    ty: b,
                    ptr: bp,
                    indices: bi,
                },
            ) => self.same_type(a, b)? && self.same_value(ap, bp)? && self.same_args(ai, bi)?,
            _ => false,
        })
    }
    fn local_uses<'a>(self, value: &'a Val, output: &mut Vec<&'a str>) -> Result<()> {
        self.pay(1)?;
        match value {
            Val::Local(name) => {
                self.pay(name.len())?;
                output.push(name);
            }
            Val::Array(args) => {
                for arg in args {
                    self.local_uses(&arg.value, output)?;
                }
            }
            Val::Gep { ptr, indices, .. } => {
                self.local_uses(ptr, output)?;
                for arg in indices {
                    self.local_uses(&arg.value, output)?;
                }
            }
            Val::Global(_)
            | Val::Int(_)
            | Val::F32(_)
            | Val::F64(_)
            | Val::Bool(_)
            | Val::Null
            | Val::Zero
            | Val::Undef
            | Val::Poison
            | Val::Bytes(_)
            | Val::Invalid(_) => {}
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Schedule {
    /// Per-function positions in dependency order, preserving each block's order.
    #[cfg_attr(not(feature = "native"), allow(dead_code))]
    pub functions: BTreeMap<String, Vec<(usize, usize)>>,
}
#[derive(Clone, Debug)]
struct Definition<'a> {
    // The complete current model stays immutable for the whole preflight.
    // Sharing its type bytes does not reuse a validation result.
    ty: &'a Ty,
    place: Option<(usize, usize)>,
}
#[derive(Default)]
struct DominatorScratch {
    seen: Vec<bool>,
    post: Vec<usize>,
    stack: Vec<(usize, bool)>,
    rank: Vec<usize>,
    idom: Vec<usize>,
}
impl DominatorScratch {
    fn clear(&mut self) {
        self.seen.clear();
        self.post.clear();
        self.stack.clear();
        self.rank.clear();
        self.idom.clear();
    }
}
#[derive(Default)]
struct FunctionScratch<'a> {
    labels: HashMap<&'a str, usize>,
    defs: HashMap<&'a str, Definition<'a>>,
    succ: Vec<Vec<usize>>,
    pred: Vec<Vec<usize>>,
    uses: Vec<&'a str>,
    dominators: DominatorScratch,
}
impl FunctionScratch<'_> {
    fn clear(&mut self) {
        self.labels.clear();
        self.defs.clear();
        self.uses.clear();
        for edges in self.succ.iter_mut().chain(&mut self.pred) {
            edges.clear();
        }
        self.dominators.clear();
    }
}
fn reusable_function_scratch(f: &Function) -> bool {
    f.blocks.as_ref().is_some_and(|blocks| {
        blocks.len() <= 32
            && blocks
                .iter()
                .try_fold(f.signature.params.len(), |count, block| {
                    count
                        .checked_add(block.instructions.len())
                        .filter(|&count| count <= 256)
                })
                .is_some_and(|count| count <= 256)
    })
}
static POINTER_RESULT: Ty = Ty::Ptr;
static COMPARISON_RESULT: Ty = Ty::Int(1);
fn fail<T>(s: impl Into<String>) -> Result<T> {
    Err(s.into())
}
fn atomic_type_alignment(ty: &Ty, align: u32) -> Result<()> {
    let Ty::Int(bits @ (8 | 16 | 32 | 64)) = ty else {
        return fail("atomic operand requires an i8/i16/i32/i64 integer");
    };
    alignment(align)?;
    if align < bits / 8 {
        return fail("atomic operand requires explicit natural alignment");
    }
    Ok(())
}
fn identifier(s: &str) -> bool {
    let mut b = s.bytes();
    let Some(first) = b.next() else { return false };
    (first.is_ascii_alphabetic() || b"._$-".contains(&first))
        && b.all(|x| x.is_ascii_alphanumeric() || b"._$-".contains(&x))
}
fn attrs(a: &[Attr], ty: Option<&Ty>) -> Result<()> {
    let mut seen = BTreeSet::new();
    if a.contains(&Attr::ZeroExt) && a.contains(&Attr::SignExt) {
        return fail("conflicting integer ABI extension attributes");
    }
    for x in a {
        if !seen.insert(x.name()) {
            return fail("duplicate machine ABI attribute");
        }
        match (x, ty) {
            (Attr::ZeroExt | Attr::SignExt, Some(Ty::Int(1 | 8 | 16 | 32 | 64))) => {}
            (Attr::NonLazyBind | Attr::NoInline | Attr::NoBuiltin | Attr::NoReturn, None) => {}
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
    meter: Meter<'a>,
    named: BTreeMap<&'a str, &'a Vec<Ty>>,
    symbols: BTreeMap<&'a str, &'a Item>,
    // Request-local successful DAG summaries, including maximum nesting height.
    // A shallow success alone cannot authorize a deeper use of the same type.
    valid_named: RefCell<BTreeMap<String, usize>>,
}
impl<'a> World<'a> {
    fn ty(
        &self,
        t: &Ty,
        allow_void: bool,
        stack: &mut BTreeSet<String>,
        depth: usize,
    ) -> Result<usize> {
        self.meter.pay(1)?;
        if depth > 128 {
            return fail("machine type nesting exceeds supported limit");
        }
        let height = match t {
            Ty::Void if allow_void => 0,
            Ty::Void => return fail("void is not a stored/parameter machine type"),
            Ty::Int(1 | 8 | 16 | 32 | 64) | Ty::F32 | Ty::F64 | Ty::Ptr => 0,
            Ty::Int(_) => return fail("unsupported machine integer width"),
            Ty::Array(n, element) => {
                if *n > u32::MAX as u64 {
                    return fail("machine array length exceeds current profile");
                }
                1 + self.ty(element, false, stack, depth + 1)?
            }
            Ty::Struct(fields) => {
                if fields.len() > u32::MAX as usize {
                    return fail("too many struct fields");
                }
                let mut height = 0;
                for f in fields {
                    height = height.max(1 + self.ty(f, false, stack, depth + 1)?);
                }
                height
            }
            Ty::Named(name) => {
                self.meter.pay(name.len())?;
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
                if let Some(&height) = self.valid_named.borrow().get(name)
                    && height <= 128 - depth
                {
                    stack.remove(name);
                    return Ok(height);
                }
                // Insufficient height allowance follows the original ordered
                // DFS, preserving its first error. Failed paths never publish.
                let mut height = 0;
                for f in fields.iter() {
                    height = height.max(1 + self.ty(f, false, stack, depth + 1)?);
                }
                self.valid_named.borrow_mut().insert(name.clone(), height);
                stack.remove(name);
                height
            }
        };
        Ok(height)
    }
    fn check_ty(&self, t: &Ty, void: bool) -> Result<()> {
        self.ty(t, void, &mut BTreeSet::new(), 0).map(|_| ())
    }
    fn fields<'b>(&'b self, t: &'b Ty) -> Result<&'b [Ty]> {
        self.meter.pay(1)?;
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
    fn indexed<'b>(&'b self, t: &'b Ty, index: u32) -> Result<&'b Ty> {
        match t {
            Ty::Array(n, element) if u64::from(index) < *n => Ok(element.as_ref()),
            Ty::Array(..) => fail("aggregate index outside machine array"),
            _ => self
                .fields(t)?
                .get(index as usize)
                .ok_or_else(|| "aggregate index outside struct".into()),
        }
    }
    fn gep(
        &self,
        t: &Ty,
        indices: &[Arg],
        defs: &HashMap<&str, Definition<'_>>,
        constant: bool,
    ) -> Result<()> {
        self.check_ty(t, false)?;
        if indices.is_empty() {
            return fail("GEP needs a pointer index");
        }
        let mut current = t;
        for (i, a) in indices.iter().enumerate() {
            self.meter.pay(1)?;
            if !matches!(a.ty, Ty::Int(_)) {
                return fail("GEP index is not integer");
            }
            self.arg(a, defs, constant)?;
            if i == 0 {
                continue;
            }
            current = match current {
                Ty::Array(_, element) => element.as_ref(),
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
                    self.indexed(current, n as u32)?
                }
                _ => return fail("GEP traverses a non-aggregate"),
            };
        }
        Ok(())
    }
    fn arg(&self, a: &Arg, defs: &HashMap<&str, Definition<'_>>, constant: bool) -> Result<()> {
        self.check_ty(&a.ty, false)?;
        self.meter.pay(a.attrs.len())?;
        attrs(&a.attrs, Some(&a.ty))?;
        self.value(&a.value, &a.ty, defs, constant, 0)
    }
    fn value(
        &self,
        v: &Val,
        t: &Ty,
        defs: &HashMap<&str, Definition<'_>>,
        constant: bool,
        depth: usize,
    ) -> Result<()> {
        self.meter.pay(1)?;
        if depth > 128 {
            return fail("constant nesting exceeds supported limit");
        }
        match v {
            Val::Local(name) => {
                self.meter.pay(name.len())?;
                if constant {
                    return fail("local reference in global/constant initializer");
                }
                let d = defs
                    .get(name.as_str())
                    .ok_or_else(|| format!("undefined SSA value %{name}"))?;
                if !self.meter.same_type(d.ty, t)? {
                    return fail(format!("SSA type mismatch for %{name}: {} != {t}", d.ty));
                }
            }
            Val::Global(name) => {
                self.meter.pay(name.len())?;
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
            Val::F32(_) if *t == Ty::F32 => {}
            Val::F64(_) if *t == Ty::F64 => {}
            Val::F32(_) | Val::F64(_) => return fail("floating constant type mismatch"),
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
                    if !self.meter.same_type(&a.ty, element)? {
                        return fail("array initializer element type mismatch");
                    }
                    if !a.attrs.is_empty() {
                        return fail("attribute on constant aggregate element");
                    }
                    self.value(&a.value, element, defs, true, depth + 1)?;
                }
            }
            Val::Bytes(bytes) => {
                self.meter.pay(bytes.len())?;
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
    fn result<'b>(&'b self, op: &'b Op) -> Result<Option<&'b Ty>> {
        self.meter.pay(1)?;
        Ok(match op {
            Op::Alloca { .. } | Op::Gep { .. } => Some(&POINTER_RESULT),
            Op::Load { ty, .. }
            | Op::AtomicLoad { ty, .. }
            | Op::Binary { ty, .. }
            | Op::Phi { ty, .. } => Some(ty),
            Op::AtomicRmw { arg, .. } => Some(&arg.ty),
            Op::AtomicCompareExchange { result, .. } => Some(result),
            Op::Compare { .. } => Some(&COMPARISON_RESULT),
            Op::Cast { to, .. } => Some(to),
            Op::Extract { arg, index } => Some(self.indexed(&arg.ty, *index)?),
            Op::Insert { arg, .. } | Op::Freeze(arg) | Op::Select { yes: arg, .. } => Some(&arg.ty),
            Op::Call {
                result: Ty::Void, ..
            } => None,
            Op::Call { result, .. } => Some(result),
            _ => None,
        })
    }
    fn op(
        &self,
        op: &Op,
        defs: &HashMap<&str, Definition<'_>>,
        signature: &Signature,
    ) -> Result<()> {
        self.meter.pay(1)?;
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
            Op::AtomicLoad {
                ty,
                ptr,
                align,
                ordering,
            } => {
                atomic_type_alignment(ty, *align)?;
                if matches!(ordering, AtomicOrdering::Release | AtomicOrdering::AcqRel) {
                    return fail("atomic load cannot have release ordering");
                }
                value(ptr, &Ty::Ptr)?;
            }
            Op::AtomicStore {
                arg,
                ptr,
                align,
                ordering,
            } => {
                atomic_type_alignment(&arg.ty, *align)?;
                if matches!(ordering, AtomicOrdering::Acquire | AtomicOrdering::AcqRel) {
                    return fail("atomic store cannot have acquire ordering");
                }
                self.arg(arg, defs, false)?;
                value(ptr, &Ty::Ptr)?;
            }
            Op::AtomicRmw {
                arg, ptr, align, ..
            } => {
                atomic_type_alignment(&arg.ty, *align)?;
                self.arg(arg, defs, false)?;
                value(ptr, &Ty::Ptr)?;
            }
            Op::AtomicCompareExchange {
                result,
                expected,
                desired,
                ptr,
                align,
                success,
                failure,
            } => {
                atomic_type_alignment(&expected.ty, *align)?;
                if !expected.attrs.is_empty() {
                    return fail("compare exchange operands cannot carry call attributes");
                }
                self.arg(expected, defs, false)?;
                value(desired, &expected.ty)?;
                value(ptr, &Ty::Ptr)?;
                if !matches!(result,Ty::Struct(fields) if fields.as_slice()==[expected.ty.clone(),Ty::Int(1)])
                {
                    return fail("compare exchange result must be exact {integer,i1}");
                }
                let valid = match success {
                    AtomicOrdering::Relaxed | AtomicOrdering::Release => {
                        *failure == AtomicOrdering::Relaxed
                    }
                    AtomicOrdering::Acquire | AtomicOrdering::AcqRel => {
                        matches!(failure, AtomicOrdering::Relaxed | AtomicOrdering::Acquire)
                    }
                    AtomicOrdering::SeqCst => matches!(
                        failure,
                        AtomicOrdering::Relaxed | AtomicOrdering::Acquire | AtomicOrdering::SeqCst
                    ),
                };
                if !valid {
                    return fail(
                        "compare exchange success/failure ordering pair is outside the C11 subset",
                    );
                }
            }
            Op::Binary {
                opcode,
                ty,
                left,
                right,
            } => {
                let valid = match ty {
                    Ty::F32 | Ty::F64 => {
                        matches!(opcode.as_str(), "fadd" | "fsub" | "fmul" | "fdiv" | "frem")
                    }
                    Ty::Int(_) => matches!(
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
                    ),
                    _ => false,
                };
                if !valid {
                    return fail(format!("unsupported binary opcode/type {opcode}/{ty}"));
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
                let valid = match ty {
                    Ty::F32 | Ty::F64 => matches!(
                        predicate.as_str(),
                        "oeq" | "one" | "une" | "ogt" | "oge" | "olt" | "ole" | "ord" | "uno"
                    ),
                    Ty::Int(_) | Ty::Ptr => matches!(
                        predicate.as_str(),
                        "eq" | "ne" | "ugt" | "uge" | "ult" | "ule" | "sgt" | "sge" | "slt" | "sle"
                    ),
                    _ => false,
                };
                if !valid {
                    return fail("unsupported comparison predicate/type");
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
                    ("inttoptr", Ty::Int(_), Ty::Ptr) => true,
                    ("sitofp" | "uitofp", Ty::Int(8 | 16 | 32 | 64), Ty::F32 | Ty::F64) => true,
                    ("fptosi" | "fptoui", Ty::F32 | Ty::F64, Ty::Int(8 | 16 | 32 | 64)) => true,
                    ("fpext", Ty::F32, Ty::F64) | ("fptrunc", Ty::F64, Ty::F32) => true,
                    ("bitcast", a, b) => {
                        matches!(
                            (a, b),
                            (Ty::Int(32), Ty::F32)
                                | (Ty::F32, Ty::Int(32))
                                | (Ty::Int(64), Ty::F64)
                                | (Ty::F64, Ty::Int(64))
                                | (Ty::Ptr, Ty::Ptr)
                        ) || a == b
                    }
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
                if !self
                    .meter
                    .same_type(self.indexed(&arg.ty, *index)?, &field.ty)?
                {
                    return fail("insertvalue field type mismatch");
                }
            }
            Op::Freeze(arg) => self.arg(arg, defs, false)?,
            Op::Select { condition, yes, no } => {
                self.value(condition, &Ty::Int(1), defs, false, 0)?;
                self.arg(yes, defs, false)?;
                self.arg(no, defs, false)?;
                if yes.ty != no.ty {
                    return fail("select operand type mismatch");
                }
            }
            Op::Call {
                result,
                result_attrs,
                callee,
                args,
                vararg,
            } => {
                self.check_ty(result, true)?;
                self.meter.pay(result_attrs.len())?;
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
                    if !self.meter.same_type(&sig.result, result)?
                        || sig.result_attrs != *result_attrs
                        || args.len() < sig.params.len()
                        || (!sig.variadic && args.len() != sig.params.len())
                    {
                        return fail(format!("call signature mismatch for @{name}"));
                    }
                    if sig.variadic != vararg.is_some() {
                        return fail("variadic call signature missing/mismatched");
                    }
                    if let Some(ts) = vararg {
                        if ts.len() != sig.params.len() {
                            return fail("variadic fixed parameter mismatch");
                        }
                        for (ty, parameter) in ts.iter().zip(&sig.params) {
                            if !self.meter.same_type(ty, &parameter.ty)? {
                                return fail("variadic fixed parameter mismatch");
                            }
                        }
                    }
                    for (p, a) in sig.params.iter().zip(args) {
                        self.meter.pay(1 + p.attrs.len() + a.attrs.len())?;
                        if !self.meter.same_type(&p.ty, &a.ty)? || p.attrs != a.attrs {
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
                if !self.meter.same_type(&a.ty, &signature.result)? {
                    return fail("return type mismatch");
                }
                self.arg(a, defs, false)?;
            }
            Op::Unreachable => {}
        }
        Ok(())
    }
}
fn successors(op: &Op) -> impl Iterator<Item = &str> {
    let edges = match op {
        Op::Branch(b) => [Some(b.as_str()), None],
        Op::CondBranch { yes, no, .. } => [Some(yes.as_str()), Some(no.as_str())],
        _ => [None, None],
    };
    edges.into_iter().flatten()
}
/// Immediate dominators in reverse postorder. No CFG×SSA/variable Cartesian table.
fn dominators_in<'a>(
    succ: &[Vec<usize>],
    pred: &[Vec<usize>],
    scratch: &'a mut DominatorScratch,
    meter: Meter<'_>,
) -> Result<(&'a [bool], &'a [usize])> {
    let n = succ.len();
    meter.pay(n.saturating_mul(5).saturating_add(scratch.stack.len()))?;
    scratch.clear();
    let DominatorScratch {
        seen,
        post,
        stack,
        rank,
        idom,
    } = scratch;
    seen.resize(n, false);
    stack.push((0usize, false));
    while let Some((b, exit)) = stack.pop() {
        meter.pay(1)?;
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
            meter.pay(1)?;
            if !seen[next] {
                stack.push((next, false));
            }
        }
    }
    post.reverse();
    rank.resize(n, usize::MAX);
    for (i, &b) in post.iter().enumerate() {
        rank[b] = i;
    }
    idom.resize(n, usize::MAX);
    idom[0] = 0;
    let intersect = |mut a: usize, mut b: usize, idom: &[usize]| -> Result<usize> {
        while a != b {
            meter.pay(1)?;
            while rank[a] > rank[b] {
                meter.pay(1)?;
                a = idom[a];
            }
            while rank[b] > rank[a] {
                meter.pay(1)?;
                b = idom[b];
            }
        }
        Ok(a)
    };
    let mut changed = true;
    while changed {
        meter.pay(1)?;
        changed = false;
        for &b in post.iter().skip(1) {
            meter.pay(1 + pred[b].len())?;
            let mut p = pred[b].iter().copied().filter(|&p| idom[p] != usize::MAX);
            let Some(mut next) = p.next() else { continue };
            // Pay O(n) setup only when the current block has enough edges to
            // cover it. Many small joins in a large CFG keep the old walk.
            if pred[b].len() >= 32 && pred[b].len() >= n.div_ceil(4) {
                next = intersect_many(next, p, rank, idom, meter)?;
            } else {
                for p in p {
                    next = intersect(next, p, idom)?;
                }
            }
            if next != idom[b] {
                idom[b] = next;
                changed = true;
            }
        }
    }
    Ok((seen, idom))
}
#[cfg(test)]
fn dominators(succ: &[Vec<usize>], pred: &[Vec<usize>]) -> (Vec<bool>, Vec<usize>) {
    let mut scratch = DominatorScratch::default();
    dominators_in(succ, pred, &mut scratch, Meter::default()).unwrap();
    (scratch.seen, scratch.idom)
}
// The current idom tree does not change during one predecessor merge. Project
// each visited node onto the first predecessor's ancestor chain, then choose
// the highest intersection on that chain. Each parent edge is followed at most
// once in this merge; shared trap exits no longer rewalk a growing chain for
// every checked operation. Nothing is retained across blocks or iterations.
fn intersect_many(
    first: usize,
    predecessors: impl Iterator<Item = usize>,
    rank: &[usize],
    idom: &[usize],
    meter: Meter<'_>,
) -> Result<usize> {
    if first == 0 {
        return Ok(0);
    }
    meter.pay(idom.len())?;
    let mut projected = vec![usize::MAX; idom.len()];
    let mut ancestor = first;
    loop {
        meter.pay(1)?;
        projected[ancestor] = ancestor;
        if ancestor == 0 {
            break;
        }
        ancestor = idom[ancestor];
    }
    let mut common = first;
    let mut trail = Vec::new();
    for mut p in predecessors {
        meter.pay(1)?;
        trail.clear();
        while projected[p] == usize::MAX {
            meter.pay(1)?;
            trail.push(p);
            p = idom[p];
        }
        let meet = projected[p];
        for &node in &trail {
            meter.pay(1)?;
            projected[node] = meet;
        }
        if rank[meet] < rank[common] {
            common = meet;
        }
        if common == 0 {
            break;
        }
    }
    Ok(common)
}
// DFS intervals on the current dominator tree turn repeated ancestor walks
// into constant-time tests. Iterative traversal also handles deep CFGs without
// growing the Rust call stack. Unreachable blocks retain sentinel intervals.
fn dominator_intervals_charged(
    reachable: &[bool],
    idom: &[usize],
    meter: Meter<'_>,
) -> Result<Vec<(usize, usize)>> {
    let n = idom.len();
    meter.pay(n.saturating_mul(4))?;
    let mut child = vec![usize::MAX; n];
    let mut sibling = vec![usize::MAX; n];
    for b in 1..n {
        if reachable[b] {
            let parent = idom[b];
            sibling[b] = child[parent];
            child[parent] = b;
        }
    }
    let mut intervals = vec![(usize::MAX, usize::MAX); n];
    let mut stack = vec![(0usize, false)];
    let mut clock = 0;
    while let Some((b, exit)) = stack.pop() {
        meter.pay(1)?;
        if exit {
            intervals[b].1 = clock;
        } else {
            intervals[b].0 = clock;
            clock += 1;
            stack.push((b, true));
            let mut next = child[b];
            while next != usize::MAX {
                meter.pay(1)?;
                stack.push((next, false));
                next = sibling[next];
            }
        }
    }
    Ok(intervals)
}
fn dominates_charged(
    reachable: &[bool],
    idom: &[usize],
    intervals: Option<&[(usize, usize)]>,
    a: usize,
    mut b: usize,
    meter: Meter<'_>,
) -> Result<bool> {
    meter.pay(1)?;
    if !reachable[b] {
        return Ok(true);
    }
    if !reachable[a] {
        return Ok(false);
    }
    if a == 0 || a == b {
        return Ok(true);
    }
    if let Some(spans) = intervals {
        return Ok(spans[a].0 <= spans[b].0 && spans[b].0 < spans[a].1);
    }
    loop {
        meter.pay(1)?;
        if a == b {
            return Ok(true);
        }
        if b == 0 {
            return Ok(false);
        }
        b = idom[b];
    }
}
#[cfg(test)]
fn dominator_intervals(reachable: &[bool], idom: &[usize]) -> Vec<(usize, usize)> {
    dominator_intervals_charged(reachable, idom, Meter::default()).unwrap()
}
#[cfg(test)]
fn dominates(
    reachable: &[bool],
    idom: &[usize],
    intervals: Option<&[(usize, usize)]>,
    a: usize,
    b: usize,
) -> bool {
    dominates_charged(reachable, idom, intervals, a, b, Meter::default()).unwrap()
}
fn function<'a, const COLLECT_SCHEDULE: bool>(
    world: &'a World<'_>,
    f: &'a Function,
    reusable: &mut FunctionScratch<'a>,
) -> Result<Vec<(usize, usize)>> {
    world.meter.pay(1 + f.signature.params.len())?;
    if f.blocks.is_none() {
        return Ok(Vec::new());
    }
    if reusable_function_scratch(f) {
        let result = function_in::<COLLECT_SCHEDULE>(world, f, reusable);
        // A single operand may contain nested values. Bound retained scratch,
        // without rescanning operands or changing validation's peak/work rules.
        if reusable.uses.capacity() > 256 {
            reusable.uses = Vec::new();
        }
        result
    } else {
        // Large functions still receive all the same checks, without retaining
        // their allocations in the small-function pool for the rest of the unit.
        function_in::<COLLECT_SCHEDULE>(world, f, &mut FunctionScratch::default())
    }
}
fn function_in<'a, const COLLECT_SCHEDULE: bool>(
    world: &'a World<'_>,
    f: &'a Function,
    scratch: &mut FunctionScratch<'a>,
) -> Result<Vec<(usize, usize)>> {
    world.meter.pay(
        scratch
            .succ
            .len()
            .saturating_add(scratch.pred.len())
            .saturating_add(scratch.defs.len())
            .saturating_add(scratch.labels.len()),
    )?;
    scratch.clear();
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
    // Input order controls diagnostics and scheduling; these maps only perform
    // identity lookup. The label collision scan returns one fixed diagnostic.
    let FunctionScratch {
        labels,
        defs,
        succ,
        pred,
        uses,
        dominators,
    } = scratch;
    world.meter.pay(n.saturating_mul(4))?;
    labels.reserve(n);
    for p in &f.signature.params {
        world
            .meter
            .pay(1 + p.name.as_ref().map_or(0, String::len))?;
        let name = p
            .name
            .as_deref()
            .ok_or("definition parameter has no name")?;
        if !identifier(name)
            || defs
                .insert(
                    name,
                    Definition {
                        ty: &p.ty,
                        place: None,
                    },
                )
                .is_some()
        {
            return fail("invalid/duplicate parameter name");
        }
    }
    for (b, block) in blocks.iter().enumerate() {
        world.meter.pay(1 + block.name.len())?;
        if !identifier(&block.name)
            || labels.insert(block.name.as_str(), b).is_some()
            || defs.contains_key(block.name.as_str())
        {
            return fail("invalid/duplicate block label");
        }
        if block.instructions.is_empty() || !block.instructions.last().unwrap().op.terminator() {
            return fail(format!("unterminated block {}", block.name));
        }
        let mut non_phi = false;
        for (j, i) in block.instructions.iter().enumerate() {
            world
                .meter
                .pay(1 + i.result.as_ref().map_or(0, String::len))?;
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
                world.check_ty(ty, false)?;
                if !identifier(name)
                    || defs
                        .insert(
                            name.as_str(),
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
    world.meter.pay(labels.len())?;
    if labels.keys().any(|label| defs.contains_key(*label)) {
        return fail("block label collides with SSA name");
    }
    if succ.len() < n {
        succ.resize_with(n, Vec::new);
    }
    if pred.len() < n {
        pred.resize_with(n, Vec::new);
    }
    let succ = &mut succ[..n];
    let pred = &mut pred[..n];
    for (b, block) in blocks.iter().enumerate() {
        world.meter.pay(1 + block.name.len())?;
        for next in successors(&block.instructions.last().unwrap().op) {
            world.meter.pay(1 + next.len())?;
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
    let (reachable, idom) = dominators_in(succ, pred, dominators, world.meter)?;
    let intervals = if n >= 32 {
        Some(dominator_intervals_charged(reachable, idom, world.meter)?)
    } else {
        None
    };
    let dominates =
        |a, b| dominates_charged(reachable, idom, intervals.as_deref(), a, b, world.meter);
    for (b, block) in blocks.iter().enumerate() {
        world.meter.pay(1 + block.name.len())?;
        for (j, i) in block.instructions.iter().enumerate() {
            world
                .meter
                .pay(1 + i.result.as_ref().map_or(0, String::len))?;
            world
                .op(&i.op, defs, &f.signature)
                .map_err(|s| format!("@{} / {} / {}: {s}", f.name, block.name, j))?;
            if let Op::Phi { incoming, .. } = &i.op {
                let mut incoming_ids = vec![];
                for (v, p) in incoming {
                    world.meter.pay(1 + p.len())?;
                    let p = *labels.get(p.as_str()).ok_or("phi incoming block missing")?;
                    incoming_ids.push(p);
                    uses.clear();
                    world.meter.local_uses(v, uses)?;
                    for &name in uses.iter() {
                        if let Some((d, _)) = defs[name].place
                            && !dominates(d, p)?
                        {
                            return fail(format!(
                                "phi source %{name} does not dominate predecessor"
                            ));
                        }
                    }
                }
                world.meter.sort(incoming_ids.len())?;
                incoming_ids.sort_unstable();
                world.meter.pay(pred[b].len())?;
                world.meter.sort(pred[b].len())?;
                let mut expected = pred[b].clone();
                expected.sort_unstable();
                if incoming_ids != expected {
                    return fail("phi incoming edges do not match CFG predecessor multiset");
                }
                for (idx, (v, p)) in incoming.iter().enumerate() {
                    world.meter.pay(idx.saturating_add(1))?;
                    for (old, block) in &incoming[..idx] {
                        if world.meter.same_name(block, p)? && !world.meter.same_value(old, v)? {
                            return fail(
                                "duplicate phi edges from a predecessor carry different values",
                            );
                        }
                    }
                }
            } else {
                for value in i.op.values() {
                    uses.clear();
                    world.meter.local_uses(value, uses)?;
                    for &name in uses.iter() {
                        if let Some((d, pos)) = defs[name].place
                            && ((d == b && pos >= j) || (d != b && !dominates(d, b)?))
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
    // Validation-only callers do not consume a Builder instruction order.
    // With every block reachable, each non-phi data edge already passed strict
    // cross-block dominance or same-block earlier-position checks above. A
    // sequence edge also moves forward within its block. Lexicographic
    // (dominator-tree depth, instruction position) therefore increases along
    // every scheduling edge. Phi definitions are precreated and excluded from
    // this graph. Unreachable blocks have weaker dominance rules, so they must
    // retain the complete original scheduling/cycle check below.
    world.meter.pay(reachable.len())?;
    if !COLLECT_SCHEDULE && reachable.iter().all(|&block| block) {
        return Ok(Vec::new());
    }

    // Phi nodes are created first. Sort all other instructions by operand and
    // per-block sequencing edges, so forward textual block order is irrelevant.
    let mut positions = vec![];
    // Positions are dense within each block; do not allocate a tree node per instruction.
    let mut ids = blocks
        .iter()
        .map(|b| {
            world.meter.pay(b.instructions.len())?;
            Ok(vec![usize::MAX; b.instructions.len()])
        })
        .collect::<Result<Vec<_>>>()?;
    for (b, block) in blocks.iter().enumerate() {
        world.meter.pay(1 + block.name.len())?;
        for (j, i) in block.instructions.iter().enumerate() {
            world
                .meter
                .pay(1 + i.result.as_ref().map_or(0, String::len))?;
            if !matches!(i.op, Op::Phi { .. }) {
                ids[b][j] = positions.len();
                positions.push((b, j));
            }
        }
    }
    world.meter.pay(positions.len().saturating_mul(2))?;
    let mut counts = vec![0; positions.len()];
    let mut users = vec![vec![]; positions.len()];
    let mut dependencies = Vec::new();
    for (id, &(b, j)) in positions.iter().enumerate() {
        world.meter.pay(1)?;
        dependencies.clear();
        if j > 0 && ids[b][j - 1] != usize::MAX {
            dependencies.push(ids[b][j - 1]);
        }
        for v in blocks[b].instructions[j].op.values() {
            uses.clear();
            world.meter.local_uses(v, uses)?;
            for &name in uses.iter() {
                if let Some((block, instruction)) = defs[name].place {
                    let source = ids[block][instruction];
                    if source != usize::MAX {
                        dependencies.push(source);
                    }
                }
            }
        }
        // Match the previous ordered set, including repeated operands/sequence edges.
        world.meter.sort(dependencies.len())?;
        world.meter.pay(dependencies.len().saturating_mul(2))?;
        dependencies.sort_unstable();
        dependencies.dedup();
        counts[id] = dependencies.len();
        for &source in &dependencies {
            users[source].push(id);
        }
    }
    let mut ready = VecDeque::new();
    for (id, &count) in counts.iter().enumerate() {
        world.meter.pay(1)?;
        if count == 0 {
            ready.push_back(id);
        }
    }
    let mut schedule = vec![];
    while let Some(id) = ready.pop_front() {
        world.meter.pay(1 + users[id].len())?;
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

#[cfg_attr(not(feature = "native"), allow(dead_code))]
pub(crate) fn validate(module: &Module) -> Result<Schedule> {
    validate_impl::<true>(module)
}

pub(crate) fn validate_only(module: &Module) -> Result<()> {
    validate_impl::<false>(module).map(|_| ())
}

fn validate_impl<const COLLECT_SCHEDULE: bool>(module: &Module) -> Result<Schedule> {
    validate_metered::<COLLECT_SCHEDULE>(module, Meter::default())
}

pub(crate) fn validate_charged(
    module: &Module,
    mut charge: impl FnMut(usize) -> Result<()>,
) -> Result<()> {
    let charge = RefCell::new(&mut charge);
    let pay = |work| (charge.borrow_mut())(work);
    validate_metered::<false>(module, Meter(Some(&pay))).map(|_| ())
}

fn validate_metered<const COLLECT_SCHEDULE: bool>(
    module: &Module,
    meter: Meter<'_>,
) -> Result<Schedule> {
    meter.pay(1 + module.triple.len() + module.fingerprint.len())?;
    if !matches!(
        module.triple.as_str(),
        "x86_64-unknown-linux-gnu" | "x86_64-pc-windows-msvc"
    ) {
        return fail("machine module target is not supported");
    }
    let mut world = World {
        meter,
        named: BTreeMap::new(),
        symbols: BTreeMap::new(),
        valid_named: RefCell::new(BTreeMap::new()),
    };
    for item in &module.items {
        meter.pay(1)?;
        match item {
            Item::NamedType(n, fields) => {
                meter.pay(n.len())?;
                if !identifier(n) || world.named.insert(n, fields).is_some() {
                    return fail("duplicate/invalid type name");
                }
            }
            Item::Global(Global { name, .. }) | Item::Function(Function { name, .. }) => {
                meter.pay(name.len())?;
                if !identifier(name) || world.symbols.insert(name, item).is_some() {
                    return fail(format!("duplicate/invalid global symbol {name}"));
                }
            }
        }
    }
    let mut functions = BTreeMap::new();
    let mut scratch = FunctionScratch::default();
    for item in &module.items {
        meter.pay(1)?;
        match item {
            Item::NamedType(n, _) => world.check_ty(&Ty::Named(n.clone()), false)?,
            Item::Global(g) => {
                if g.linkage == Linkage::AvailableExternally {
                    return fail("available-externally globals are not supported");
                }
                world.check_ty(&g.ty, false)?;
                alignment(g.alignment)?;
                if let Some(v) = &g.initializer {
                    world.value(v, &g.ty, &HashMap::new(), true, 0)?;
                } else if g.linkage.is_local() {
                    return fail("local global declaration without definition");
                }
            }
            Item::Function(f) => {
                world.check_ty(&f.signature.result, true)?;
                meter.pay(f.signature.result_attrs.len() + f.attrs.len())?;
                attrs(&f.signature.result_attrs, Some(&f.signature.result))?;
                attrs(&f.attrs, None)?;
                if f.signature.params.len() > u32::MAX as usize {
                    return fail("too many machine parameters");
                }
                for p in &f.signature.params {
                    world.check_ty(&p.ty, false)?;
                    meter.pay(1 + p.attrs.len())?;
                    attrs(&p.attrs, Some(&p.ty))?;
                }
                if f.blocks.is_none()
                    && (f.linkage.is_local() || f.linkage == Linkage::AvailableExternally)
                {
                    return fail("local function without definition");
                }
                if COLLECT_SCHEDULE {
                    functions.insert(f.name.clone(), function::<true>(&world, f, &mut scratch)?);
                } else {
                    function::<false>(&world, f, &mut scratch)?;
                }
            }
        }
    }
    Ok(Schedule { functions })
}

#[cfg(test)]
#[path = "preflight_validate_only_tests.rs"]
mod validate_only_tests;

#[cfg(test)]
#[path = "complete_preparation_tests.rs"]
mod complete_preparation_tests;

#[cfg(test)]
mod named_layout_tests {
    use super::*;
    impl World<'_> {
        fn legacy_ty(
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
                Ty::Int(1 | 8 | 16 | 32 | 64) | Ty::F32 | Ty::F64 | Ty::Ptr => {}
                Ty::Int(_) => return fail("unsupported machine integer width"),
                Ty::Array(n, element) => {
                    if *n > u32::MAX as u64 {
                        return fail("machine array length exceeds current profile");
                    }
                    self.legacy_ty(element, false, stack, depth + 1)?;
                }
                Ty::Struct(fields) => {
                    if fields.len() > u32::MAX as usize {
                        return fail("too many struct fields");
                    }
                    for f in fields {
                        self.legacy_ty(f, false, stack, depth + 1)?;
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
                        self.legacy_ty(f, false, stack, depth + 1)?;
                    }
                    stack.remove(name);
                }
            }
            Ok(())
        }
    }
    fn world(definitions: &[(String, Vec<Ty>)]) -> World<'_> {
        World {
            meter: Meter::default(),
            named: definitions
                .iter()
                .map(|(name, fields)| (name.as_str(), fields))
                .collect(),
            symbols: BTreeMap::new(),
            valid_named: RefCell::new(BTreeMap::new()),
        }
    }
    fn diamond(count: usize) -> Vec<(String, Vec<Ty>)> {
        (0..count)
            .map(|i| {
                (
                    format!("T{i}"),
                    if i == 0 {
                        vec![Ty::Int(32)]
                    } else {
                        vec![
                            Ty::Named(format!("T{}", i - 1)),
                            Ty::Named(format!("T{}", i - 1)),
                        ]
                    },
                )
            })
            .collect()
    }
    #[test]
    fn shared_layout_dag_is_bounded_by_nodes_and_depth_not_expanded_paths() {
        let definitions = diamond(120);
        let world = world(&definitions);
        // The old DFS would visit more than 2^119 paths for this bounded input.
        world.check_ty(&Ty::Named("T119".into()), false).unwrap();
        assert_eq!(world.valid_named.borrow().len(), 120);
        for i in 0..120 {
            world.check_ty(&Ty::Named(format!("T{i}")), false).unwrap();
        }
        assert_eq!(world.valid_named.borrow().len(), 120);
    }
    #[test]
    fn cached_shallow_success_never_hides_deeper_failure_or_first_error() {
        let definitions = diamond(5);
        let world = world(&definitions);
        let named = Ty::Named("T4".into());
        world.check_ty(&named, false).unwrap();
        for depth in [0, 1, 123, 124, 125, 128, 129] {
            let old = world.legacy_ty(&named, false, &mut BTreeSet::new(), depth);
            let current = world
                .ty(&named, false, &mut BTreeSet::new(), depth)
                .map(|_| ());
            assert_eq!(current, old, "depth {depth}");
        }
        let mut active = BTreeSet::from(["T4".into()]);
        assert_eq!(
            world.ty(&named, false, &mut active, 0).unwrap_err(),
            "recursive inline machine layout"
        );
    }
    #[test]
    fn cyclic_missing_void_and_width_neighbors_match_original_dfs() {
        for bad in [
            Ty::Named("Missing".into()),
            Ty::Named("T2".into()),
            Ty::Void,
            Ty::Int(7),
            Ty::Named("bad name".into()),
            Ty::Array(u64::MAX, Box::new(Ty::Int(8))),
        ] {
            for position in [0, 1] {
                let mut definitions = diamond(5);
                definitions[2].1[position] = bad.clone();
                let world = world(&definitions);
                for i in [0, 1, 4, 2, 3, 4] {
                    let ty = Ty::Named(format!("T{i}"));
                    assert_eq!(
                        world.check_ty(&ty, false),
                        world.legacy_ty(&ty, false, &mut BTreeSet::new(), 0)
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod dominance_interval_tests {
    use super::*;
    fn old_walk(reachable: &[bool], idom: &[usize], a: usize, mut b: usize) -> bool {
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
    }
    #[test]
    fn intervals_match_original_walk_for_cycles_unreachable_and_irreducible_cfgs() {
        let mut seed = 0x9e3779b97f4a7c15u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for n in [1, 2, 7, 31, 32, 33, 64, 129] {
            for _ in 0..40 {
                let mut succ = vec![vec![]; n];
                let mut pred = vec![vec![]; n];
                for (b, edges) in succ.iter_mut().enumerate() {
                    for (p, predecessors) in pred.iter_mut().enumerate().skip(1) {
                        if next() % 11 == 0 {
                            edges.push(p);
                            predecessors.push(b);
                        }
                    }
                }
                let (reachable, idom) = dominators(&succ, &pred);
                let spans = dominator_intervals(&reachable, &idom);
                for a in 0..n {
                    for b in 0..n {
                        let expected = old_walk(&reachable, &idom, a, b);
                        assert_eq!(
                            dominates(&reachable, &idom, Some(&spans), a, b),
                            expected,
                            "n={n} {a}->{b}"
                        );
                        assert_eq!(dominates(&reachable, &idom, None, a, b), expected);
                    }
                }
            }
        }
    }
    #[test]
    fn deep_dominator_chain_uses_iterative_intervals() {
        let n: usize = 65_536;
        let idom = (0..n).map(|b| b.saturating_sub(1)).collect::<Vec<usize>>();
        let reachable = vec![true; n];
        let spans = dominator_intervals(&reachable, &idom);
        for b in 1..n {
            assert!(dominates(&reachable, &idom, Some(&spans), b - 1, b));
            assert!(!dominates(&reachable, &idom, Some(&spans), b, b - 1));
            assert!(dominates(&reachable, &idom, Some(&spans), 1, b));
        }
    }

    #[test]
    fn checked_chain_trap_idoms_survive_fanin_order_duplicates_and_backedges() {
        for length in [1, 2, 30, 31, 32, 33, 128, 4096] {
            for reverse in [false, true] {
                for backedge in [false, true] {
                    let trap = length + 1;
                    let unreachable = length + 2;
                    let mut succ = vec![vec![]; length + 3];
                    succ[0].push(1);
                    for (b, edges) in succ.iter_mut().enumerate().take(length + 1).skip(1) {
                        if b < length {
                            edges.push(b + 1);
                        }
                        edges.push(trap);
                    }
                    succ[length].push(trap); // Duplicate CFG edge.
                    if backedge {
                        succ[length].push((length / 2).max(1));
                    }
                    succ[unreachable].extend([unreachable, trap]);
                    let mut pred = vec![vec![]; succ.len()];
                    for (b, edges) in succ.iter().enumerate() {
                        for &next in edges {
                            pred[next].push(b);
                        }
                    }
                    if reverse {
                        for edges in &mut pred {
                            edges.reverse();
                        }
                    }
                    let (reachable, idom) = dominators(&succ, &pred);
                    for b in 1..=length {
                        assert!(reachable[b]);
                        assert_eq!(idom[b], b - 1);
                    }
                    assert_eq!(idom[trap], 1);
                    assert!(reachable[trap]);
                    assert!(!reachable[unreachable]);
                    assert_eq!(idom[unreachable], usize::MAX);
                }
            }
        }
    }
}

#[cfg(test)]
mod signed_abi_attribute_tests {
    use super::*;
    #[test]
    fn integer_extension_requires_an_integer_and_one_sign_policy() {
        assert!(attrs(&[Attr::SignExt], Some(&Ty::Int(8))).is_ok());
        assert!(attrs(&[Attr::SignExt], Some(&Ty::Int(16))).is_ok());
        assert!(attrs(&[Attr::SignExt], Some(&Ty::F32)).is_err());
        assert!(attrs(&[Attr::SignExt], None).is_err());
        assert!(attrs(&[Attr::ZeroExt, Attr::SignExt], Some(&Ty::Int(8))).is_err());
    }
}
