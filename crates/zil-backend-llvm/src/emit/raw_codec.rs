//! Bounded raw scalar machine values. No model parser or validation receipts.
use super::{FunctionBatch, m};

pub(crate) const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_DEPTH: usize = 32;

pub(crate) struct Writer(pub(crate) Vec<u8>, usize);
impl Writer {
    pub(crate) fn new() -> Self {
        Self::bounded(MAX_BYTES)
    }
    pub(crate) fn bounded(maximum: usize) -> Self {
        Self(Vec::new(), maximum.min(MAX_BYTES))
    }
    pub(crate) fn put(&mut self, bytes: &[u8]) -> Option<()> {
        if self.0.len().checked_add(bytes.len())? > self.1 {
            return None;
        }
        self.0.extend_from_slice(bytes);
        Some(())
    }
    pub(crate) fn word(&mut self, n: usize) -> Option<()> {
        self.put(&u32::try_from(n).ok()?.to_le_bytes())
    }
    pub(crate) fn blob(&mut self, bytes: &[u8]) -> Option<()> {
        self.word(bytes.len())?;
        self.put(bytes)
    }
    pub(crate) fn text(&mut self, text: &str) -> Option<()> {
        self.blob(text.as_bytes())
    }
    fn tag(&mut self, tag: u8) -> Option<()> {
        self.put(&[tag])
    }
    fn optional_text(&mut self, text: &Option<String>) -> Option<()> {
        self.tag(u8::from(text.is_some()))?;
        if let Some(text) = text {
            self.text(text)?;
        }
        Some(())
    }
    fn attrs(&mut self, attrs: &[m::Attr]) -> Option<()> {
        self.word(attrs.len())?;
        for attr in attrs {
            self.tag(match attr {
                m::Attr::ZeroExt => 0,
                m::Attr::NonLazyBind => 1,
                m::Attr::NoInline => 2,
                m::Attr::SignExt => 3,
                m::Attr::NoUnwind => 4,
                m::Attr::WillReturn => 5,
                m::Attr::MemoryNone => 6,
            })?;
        }
        Some(())
    }
    fn ty(&mut self, ty: &m::Ty, depth: usize) -> Option<()> {
        if depth > MAX_DEPTH {
            return None;
        }
        match ty {
            m::Ty::Void => self.tag(0),
            m::Ty::Int(bits @ (1 | 8 | 16 | 32 | 64)) => {
                self.tag(1)?;
                self.word(*bits as usize)
            }
            m::Ty::Ptr => self.tag(2),
            m::Ty::Struct(fields) => {
                self.tag(3)?;
                self.word(fields.len())?;
                for field in fields {
                    self.ty(field, depth + 1)?;
                }
                Some(())
            }
            _ => None,
        }
    }
    fn val(&mut self, value: &m::Val) -> Option<()> {
        match value {
            m::Val::Local(name) => {
                self.tag(0)?;
                self.text(name)
            }
            m::Val::Global(name) => {
                self.tag(1)?;
                self.text(name)
            }
            m::Val::Int(value) => {
                self.tag(2)?;
                self.put(&value.to_le_bytes())
            }
            m::Val::Bool(value) => {
                self.tag(3)?;
                self.tag(u8::from(*value))
            }
            m::Val::Null => self.tag(4),
            m::Val::Zero => self.tag(5),
            m::Val::Undef => self.tag(6),
            m::Val::Poison => self.tag(7),
            _ => None,
        }
    }
    fn arg(&mut self, arg: &m::Arg) -> Option<()> {
        let m::Arg { ty, value, attrs } = arg;
        self.ty(ty, 0)?;
        self.val(value)?;
        self.attrs(attrs)
    }
    fn op(&mut self, op: &m::Op) -> Option<()> {
        use m::Op;
        match op {
            Op::Alloca { ty, align } => {
                self.tag(0)?;
                self.ty(ty, 0)?;
                self.word(*align as usize)
            }
            Op::Load { ty, ptr, align } => {
                self.tag(1)?;
                self.ty(ty, 0)?;
                self.val(ptr)?;
                self.word(*align as usize)
            }
            Op::Store { arg, ptr, align } => {
                self.tag(2)?;
                self.arg(arg)?;
                self.val(ptr)?;
                self.word(*align as usize)
            }
            Op::Binary {
                opcode,
                ty,
                left,
                right,
            } => {
                self.tag(3)?;
                self.text(opcode)?;
                self.ty(ty, 0)?;
                self.val(left)?;
                self.val(right)
            }
            Op::Compare {
                predicate,
                ty,
                left,
                right,
            } => {
                self.tag(4)?;
                self.text(predicate)?;
                self.ty(ty, 0)?;
                self.val(left)?;
                self.val(right)
            }
            Op::Cast { opcode, arg, to } => {
                self.tag(5)?;
                self.text(opcode)?;
                self.arg(arg)?;
                self.ty(to, 0)
            }
            Op::Extract { arg, index } => {
                self.tag(6)?;
                self.arg(arg)?;
                self.word(*index as usize)
            }
            Op::Insert { arg, field, index } => {
                self.tag(7)?;
                self.arg(arg)?;
                self.arg(field)?;
                self.word(*index as usize)
            }
            Op::Call {
                result,
                result_attrs,
                callee,
                args,
                vararg,
            } => {
                self.tag(8)?;
                self.ty(result, 0)?;
                self.attrs(result_attrs)?;
                self.val(callee)?;
                self.word(args.len())?;
                for arg in args {
                    self.arg(arg)?;
                }
                self.tag(u8::from(vararg.is_some()))?;
                if let Some(types) = vararg {
                    self.word(types.len())?;
                    for ty in types {
                        self.ty(ty, 0)?;
                    }
                }
                Some(())
            }
            Op::Freeze(arg) => {
                self.tag(9)?;
                self.arg(arg)
            }
            Op::Phi { ty, incoming } => {
                self.tag(10)?;
                self.ty(ty, 0)?;
                self.word(incoming.len())?;
                for (value, label) in incoming {
                    self.val(value)?;
                    self.text(label)?;
                }
                Some(())
            }
            Op::Branch(label) => {
                self.tag(11)?;
                self.text(label)
            }
            Op::CondBranch { condition, yes, no } => {
                self.tag(12)?;
                self.val(condition)?;
                self.text(yes)?;
                self.text(no)
            }
            Op::Return(arg) => {
                self.tag(13)?;
                self.tag(u8::from(arg.is_some()))?;
                if let Some(arg) = arg {
                    self.arg(arg)?;
                }
                Some(())
            }
            Op::Unreachable => self.tag(14),
            Op::Select {
                ty,
                condition,
                yes,
                no,
            } => {
                self.tag(15)?;
                self.ty(ty, 0)?;
                self.val(condition)?;
                self.val(yes)?;
                self.val(no)
            }
            _ => None,
        }
    }
}

pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    pub(crate) nodes: usize,
}
impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8], nodes: usize) -> Option<Self> {
        (bytes.len() <= MAX_BYTES).then_some(Self { bytes, nodes })
    }
    fn node(&mut self) -> Option<()> {
        self.nodes = self.nodes.checked_sub(1)?;
        Some(())
    }
    pub(crate) fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let (value, rest) = self.bytes.split_at_checked(count)?;
        self.bytes = rest;
        Some(value)
    }
    pub(crate) fn word(&mut self) -> Option<usize> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?) as usize)
    }
    pub(crate) fn count(&mut self) -> Option<usize> {
        let count = self.word()?;
        if count > self.nodes || count > self.bytes.len() {
            return None;
        }
        self.nodes -= count;
        Some(count)
    }
    pub(crate) fn blob(&mut self) -> Option<&'a [u8]> {
        let n = self.word()?;
        self.take(n)
    }
    pub(crate) fn text(&mut self) -> Option<String> {
        self.node()?;
        Some(std::str::from_utf8(self.blob()?).ok()?.to_owned())
    }
    fn tag(&mut self) -> Option<u8> {
        Some(*self.take(1)?.first()?)
    }
    fn boolean(&mut self) -> Option<bool> {
        match self.tag()? {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }
    fn optional_text(&mut self) -> Option<Option<String>> {
        Some(if self.boolean()? {
            Some(self.text()?)
        } else {
            None
        })
    }
    pub(crate) fn finish(&self) -> bool {
        self.bytes.is_empty()
    }
    // Option's fallible collector hides its exact lower size hint. Reserve the
    // validated count once for ordinary lists, with a small fixed cap so a
    // malformed large count cannot force a large allocation before decoding.
    // Every node/byte/depth check and the ordered element decode stays intact.
    fn list<T>(
        &mut self,
        count: usize,
        mut decode: impl FnMut(&mut Self) -> Option<T>,
    ) -> Option<Vec<T>> {
        let mut values = Vec::with_capacity(count.min(4096));
        for _ in 0..count {
            values.push(decode(self)?);
        }
        Some(values)
    }
    fn attrs(&mut self) -> Option<Vec<m::Attr>> {
        let count = self.count()?;
        self.list(count, |input| {
            Some(match input.tag()? {
                0 => m::Attr::ZeroExt,
                1 => m::Attr::NonLazyBind,
                2 => m::Attr::NoInline,
                3 => m::Attr::SignExt,
                4 => m::Attr::NoUnwind,
                5 => m::Attr::WillReturn,
                6 => m::Attr::MemoryNone,
                _ => return None,
            })
        })
    }
    fn ty(&mut self, depth: usize) -> Option<m::Ty> {
        if depth > MAX_DEPTH {
            return None;
        }
        self.node()?;
        Some(match self.tag()? {
            0 => m::Ty::Void,
            1 => {
                let bits = self.word()?;
                if !matches!(bits, 1 | 8 | 16 | 32 | 64) {
                    return None;
                }
                m::Ty::Int(bits as u32)
            }
            2 => m::Ty::Ptr,
            3 => {
                let count = self.count()?;
                m::Ty::Struct(self.list(count, |input| input.ty(depth + 1))?)
            }
            _ => return None,
        })
    }
    fn val(&mut self) -> Option<m::Val> {
        self.node()?;
        Some(match self.tag()? {
            0 => m::Val::Local(self.text()?),
            1 => m::Val::Global(self.text()?),
            2 => m::Val::Int(i128::from_le_bytes(self.take(16)?.try_into().ok()?)),
            3 => m::Val::Bool(self.boolean()?),
            4 => m::Val::Null,
            5 => m::Val::Zero,
            6 => m::Val::Undef,
            7 => m::Val::Poison,
            _ => return None,
        })
    }
    fn arg(&mut self) -> Option<m::Arg> {
        self.node()?;
        Some(m::Arg {
            ty: self.ty(0)?,
            value: self.val()?,
            attrs: self.attrs()?,
        })
    }
    fn op(&mut self) -> Option<m::Op> {
        use m::Op;
        self.node()?;
        Some(match self.tag()? {
            0 => Op::Alloca {
                ty: self.ty(0)?,
                align: self.word()? as u32,
            },
            1 => Op::Load {
                ty: self.ty(0)?,
                ptr: self.val()?,
                align: self.word()? as u32,
            },
            2 => Op::Store {
                arg: self.arg()?,
                ptr: self.val()?,
                align: self.word()? as u32,
            },
            3 => Op::Binary {
                opcode: self.text()?,
                ty: self.ty(0)?,
                left: self.val()?,
                right: self.val()?,
            },
            4 => Op::Compare {
                predicate: self.text()?,
                ty: self.ty(0)?,
                left: self.val()?,
                right: self.val()?,
            },
            5 => Op::Cast {
                opcode: self.text()?,
                arg: self.arg()?,
                to: self.ty(0)?,
            },
            6 => Op::Extract {
                arg: self.arg()?,
                index: self.word()? as u32,
            },
            7 => Op::Insert {
                arg: self.arg()?,
                field: self.arg()?,
                index: self.word()? as u32,
            },
            8 => {
                let result = self.ty(0)?;
                let result_attrs = self.attrs()?;
                let callee = self.val()?;
                let count = self.count()?;
                let args = self.list(count, |input| input.arg())?;
                let vararg = if self.boolean()? {
                    let count = self.count()?;
                    Some(self.list(count, |input| input.ty(0))?)
                } else {
                    None
                };
                Op::Call {
                    result,
                    result_attrs,
                    callee,
                    args,
                    vararg,
                }
            }
            9 => Op::Freeze(self.arg()?),
            10 => {
                let ty = self.ty(0)?;
                let count = self.count()?;
                let incoming = self.list(count, |input| Some((input.val()?, input.text()?)))?;
                Op::Phi { ty, incoming }
            }
            11 => Op::Branch(self.text()?),
            12 => Op::CondBranch {
                condition: self.val()?,
                yes: self.text()?,
                no: self.text()?,
            },
            13 => Op::Return(if self.boolean()? {
                Some(self.arg()?)
            } else {
                None
            }),
            14 => Op::Unreachable,
            15 => Op::Select {
                ty: self.ty(0)?,
                condition: self.val()?,
                yes: self.val()?,
                no: self.val()?,
            },
            _ => return None,
        })
    }
}

#[cfg(test)]
pub(super) fn encode(batch: &FunctionBatch) -> Option<Vec<u8>> {
    encode_bounded(batch, MAX_BYTES)
}

pub(super) fn encode_bounded(batch: &FunctionBatch, maximum: usize) -> Option<Vec<u8>> {
    if !batch.task_machines.is_empty() || !batch.foreign_adapters.is_empty() {
        return None;
    }
    let [m::Item::Function(function)] = batch.body.as_slice() else {
        return None;
    };
    let m::Function {
        name,
        linkage,
        signature,
        attrs,
        blocks,
    } = function;
    let m::Signature {
        result,
        result_attrs,
        params,
        variadic,
    } = signature;
    let mut out = Writer::bounded(maximum);
    out.put(b"ZILRAW01")?;
    out.text(name)?;
    out.tag(match linkage {
        m::Linkage::External => 0,
        _ => return None,
    })?;
    out.ty(result, 0)?;
    out.attrs(result_attrs)?;
    out.word(params.len())?;
    for m::Param { ty, name, attrs } in params {
        out.ty(ty, 0)?;
        out.optional_text(name)?;
        out.attrs(attrs)?;
    }
    out.tag(u8::from(*variadic))?;
    out.attrs(attrs)?;
    let blocks = blocks.as_ref()?;
    out.word(blocks.len())?;
    for m::Block { name, instructions } in blocks {
        out.text(name)?;
        out.word(instructions.len())?;
        for m::Inst { result, op } in instructions {
            out.optional_text(result)?;
            out.op(op)?;
        }
    }
    out.word(batch.intrinsics.len())?;
    for (name, bits) in &batch.intrinsics {
        out.text(name)?;
        out.word(*bits as usize)?;
    }
    Some(out.0)
}

pub(super) fn decode(
    bytes: &[u8],
    limits: &zil_base::Limits,
    remaining: &mut usize,
) -> Option<FunctionBatch> {
    let mut input = Reader::new(bytes, *remaining)?;
    if input.take(8)? != b"ZILRAW01" {
        return None;
    }
    let name = input.text()?;
    if input.tag()? != 0 {
        return None;
    }
    let result = input.ty(0)?;
    let result_attrs = input.attrs()?;
    let count = input.count()?;
    if count > limits.max_locals {
        return None;
    }
    let params = input.list(count, |input| {
        Some(m::Param {
            ty: input.ty(0)?,
            name: input.optional_text()?,
            attrs: input.attrs()?,
        })
    })?;
    let variadic = input.boolean()?;
    let attrs = input.attrs()?;
    let count = input.count()?;
    if count > limits.max_blocks {
        return None;
    }
    let mut blocks = Vec::with_capacity(count);
    let mut instructions_left = limits.max_instructions;
    for _ in 0..count {
        let name = input.text()?;
        let count = input.count()?;
        instructions_left = instructions_left.checked_sub(count)?;
        let instructions = input.list(count, |input| {
            Some(m::Inst {
                result: input.optional_text()?,
                op: input.op()?,
            })
        })?;
        blocks.push(m::Block { name, instructions });
    }
    let count = input.count()?;
    let mut intrinsics = std::collections::BTreeSet::new();
    for _ in 0..count {
        let name = input.text()?;
        let bits = input.word()? as u32;
        let name = match name.as_str() {
            "trap" => "trap",
            "sadd" => "sadd",
            "ssub" => "ssub",
            "smul" => "smul",
            "uadd" => "uadd",
            "usub" => "usub",
            "umul" => "umul",
            _ => return None,
        };
        if !matches!(bits, 1 | 8 | 16 | 32 | 64) || !intrinsics.insert((name, bits)) {
            return None;
        }
    }
    if !input.finish() {
        return None;
    }
    *remaining = input.nodes;
    Some(FunctionBatch {
        body: vec![m::Item::Function(m::Function {
            name,
            linkage: m::Linkage::External,
            signature: m::Signature {
                result,
                result_attrs,
                params,
                variadic,
            },
            attrs,
            blocks: Some(blocks.into()),
        })]
        .into(),
        intrinsics,
        task_machines: vec![],
        foreign_adapters: std::collections::BTreeSet::new(),
    })
}

/// Raw scalar model value; never a preflight receipt. Unlike emission batches,
/// this also represents declarations and the planner's internal definitions.
pub(crate) fn encode_function(function: &m::Function) -> Option<Vec<u8>> {
    let mut out = Writer::new();
    out.put(b"ZILFUN01")?;
    out.text(&function.name)?;
    out.word(match function.linkage {
        m::Linkage::External => 0,
        m::Linkage::Internal => 1,
        m::Linkage::Private => 2,
        m::Linkage::AvailableExternally => 3,
    })?;
    out.ty(&function.signature.result, 0)?;
    out.attrs(&function.signature.result_attrs)?;
    out.word(function.signature.params.len())?;
    for param in &function.signature.params {
        out.ty(&param.ty, 0)?;
        out.optional_text(&param.name)?;
        out.attrs(&param.attrs)?;
    }
    out.tag(u8::from(function.signature.variadic))?;
    out.attrs(&function.attrs)?;
    out.tag(u8::from(function.blocks.is_some()))?;
    if let Some(blocks) = &function.blocks {
        out.word(blocks.len())?;
        for block in blocks {
            out.text(&block.name)?;
            out.word(block.instructions.len())?;
            for inst in &block.instructions {
                out.optional_text(&inst.result)?;
                out.op(&inst.op)?;
            }
        }
    }
    Some(out.0)
}

pub(crate) fn decode_function(
    bytes: &[u8],
    limits: &zil_base::Limits,
    remaining: &mut usize,
) -> Option<m::Function> {
    let mut input = Reader::new(bytes, *remaining)?;
    if input.take(8)? != b"ZILFUN01" {
        return None;
    }
    let name = input.text()?;
    let linkage = match input.word()? {
        0 => m::Linkage::External,
        1 => m::Linkage::Internal,
        2 => m::Linkage::Private,
        3 => m::Linkage::AvailableExternally,
        _ => return None,
    };
    let result = input.ty(0)?;
    let result_attrs = input.attrs()?;
    let count = input.count()?;
    if count > limits.max_locals {
        return None;
    }
    let params = input.list(count, |input| {
        Some(m::Param {
            ty: input.ty(0)?,
            name: input.optional_text()?,
            attrs: input.attrs()?,
        })
    })?;
    let variadic = input.boolean()?;
    let attrs = input.attrs()?;
    let blocks = if input.boolean()? {
        let count = input.count()?;
        if count > limits.max_blocks {
            return None;
        }
        let mut blocks = Vec::with_capacity(count.min(4096));
        let mut instructions = limits.max_instructions;
        for _ in 0..count {
            let name = input.text()?;
            let count = input.count()?;
            instructions = instructions.checked_sub(count)?;
            blocks.push(m::Block {
                name,
                instructions: input.list(count, |input| {
                    Some(m::Inst {
                        result: input.optional_text()?,
                        op: input.op()?,
                    })
                })?,
            });
        }
        Some(blocks.into())
    } else {
        None
    };
    if !input.finish() {
        return None;
    }
    *remaining = input.nodes;
    Some(m::Function {
        name,
        linkage,
        signature: m::Signature {
            result,
            result_attrs,
            params,
            variadic,
        },
        attrs,
        blocks,
    })
}
