//! Backend-neutral, owned machine IR. No LLVM pointers and no assembly parser.
//! Text is a diagnostic/reference rendering; native codegen consumes these nodes.
pub use crate::shared_vec::SharedVec;
use std::collections::BTreeSet;
use std::fmt::{self, Write};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Ty {
    Void,
    Int(u32),
    F32,
    F64,
    Ptr,
    Array(u64, Box<Ty>),
    Struct(Vec<Ty>),
    Named(String),
}
impl Ty {
    pub fn array(length: u64, element: Ty) -> Self {
        Self::Array(length, Box::new(element))
    }
    pub fn structure(fields: Vec<Ty>) -> Self {
        Self::Struct(fields)
    }
}
impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Void => f.write_str("void"),
            Self::Int(bits) => write!(f, "i{bits}"),
            Self::F32 => f.write_str("float"),
            Self::F64 => f.write_str("double"),
            Self::Ptr => f.write_str("ptr"),
            Self::Array(n, ty) => write!(f, "[{n} x {ty}]"),
            Self::Struct(fields) => {
                if fields.is_empty() {
                    return f.write_str("{}");
                }
                f.write_str("{ ")?;
                for (i, field) in fields.iter().enumerate() {
                    if i != 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{field}")?;
                }
                f.write_str(" }")
            }
            Self::Named(name) => write!(f, "%{name}"),
        }
    }
}
// Only primitive spellings used at lowering call sites; never parse type syntax.
impl From<&str> for Ty {
    fn from(s: &str) -> Self {
        match s {
            "void" => Self::Void,
            "ptr" => Self::Ptr,
            "i1" => Self::Int(1),
            "i8" => Self::Int(8),
            "i16" => Self::Int(16),
            "i32" => Self::Int(32),
            "i64" => Self::Int(64),
            "float" => Self::F32,
            "double" => Self::F64,
            "{}" => Self::Struct(vec![]),
            _ => panic!("unsupported primitive type spelling in compiler: {s}"),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Linkage {
    External,
    /// Optimizer-only body; the canonical definition is emitted by another unit.
    AvailableExternally,
    Internal,
    Private,
}
impl Linkage {
    pub fn is_local(self) -> bool {
        matches!(self, Self::Internal | Self::Private)
    }
    fn prefix(self) -> &'static str {
        match self {
            Self::External => "",
            Self::AvailableExternally => "available_externally ",
            Self::Internal => "internal ",
            Self::Private => "private ",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Attr {
    ZeroExt,
    NonLazyBind,
    /// Function-level optimization boundary; never a language permission.
    NoInline,
    SignExt,
}
impl Attr {
    pub fn name(self) -> &'static str {
        match self {
            Self::ZeroExt => "zeroext",
            Self::SignExt => "signext",
            Self::NonLazyBind => "nonlazybind",
            Self::NoInline => "noinline",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Val {
    Local(String),
    Global(String),
    Int(i128),
    /// Exact IEEE bits, rendered/constructed through constant bitcasts.
    F32(u32),
    F64(u64),
    Bool(bool),
    Null,
    Zero,
    Undef,
    Poison,
    Array(Vec<Arg>),
    Bytes(Vec<u8>),
    /// Constant address arithmetic, e.g. sizeof(T), not an instruction.
    Gep {
        ty: Ty,
        ptr: Box<Val>,
        indices: Vec<Arg>,
    },
    Invalid(String),
}
impl Val {
    pub fn symbols(&self, output: &mut BTreeSet<String>) {
        match self {
            Self::Global(name) => {
                output.insert(name.clone());
            }
            Self::Array(args) => {
                for a in args {
                    a.value.symbols(output);
                }
            }
            Self::Gep { ptr, indices, .. } => {
                ptr.symbols(output);
                for a in indices {
                    a.value.symbols(output);
                }
            }
            _ => {}
        }
    }
    pub fn local_uses<'a>(&'a self, output: &mut Vec<&'a str>) {
        match self {
            Self::Local(name) => output.push(name),
            Self::Array(args) => {
                for a in args {
                    a.value.local_uses(output);
                }
            }
            Self::Gep { ptr, indices, .. } => {
                ptr.local_uses(output);
                for a in indices {
                    a.value.local_uses(output);
                }
            }
            _ => {}
        }
    }
}
impl fmt::Display for Val {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local(n) => write!(f, "%{n}"),
            Self::Global(n) => write!(f, "@{n}"),
            Self::Int(n) => write!(f, "{n}"),
            Self::F32(bits) => write!(f, "bitcast (i32 {bits} to float)"),
            Self::F64(bits) => write!(f, "bitcast (i64 {bits} to double)"),
            Self::Bool(b) => write!(f, "{b}"),
            Self::Null => f.write_str("null"),
            Self::Zero => f.write_str("zeroinitializer"),
            Self::Undef => f.write_str("undef"),
            Self::Poison => f.write_str("poison"),
            Self::Array(args) => {
                f.write_str("[")?;
                for (i, a) in args.iter().enumerate() {
                    if i != 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{a}")?;
                }
                f.write_str("]")
            }
            Self::Bytes(bytes) => {
                f.write_str("c\"")?;
                for b in bytes {
                    if (32..=126).contains(b) && *b != b'\\' && *b != b'"' {
                        write!(f, "{}", char::from(*b))?;
                    } else {
                        write!(f, "\\{b:02X}")?;
                    }
                }
                f.write_str("\"")
            }
            Self::Gep { ty, ptr, indices } => {
                write!(f, "getelementptr ({ty}, ptr {ptr}")?;
                for a in indices {
                    write!(f, ", {a}")?;
                }
                f.write_str(")")
            }
            Self::Invalid(s) => write!(f, "<invalid:{s}>"),
        }
    }
}
// Existing lowering uses textual *names* for temporary slots. Decode only a
// single symbolic atom, never an LLVM expression or instruction.
impl From<&str> for Val {
    fn from(s: &str) -> Self {
        if let Some(n) = s.strip_prefix('%') {
            Self::Local(n.into())
        } else if let Some(n) = s.strip_prefix('@') {
            Self::Global(n.into())
        } else {
            match s {
                "zeroinitializer" => Self::Zero,
                "undef" => Self::Undef,
                "poison" => Self::Poison,
                "null" => Self::Null,
                "true" => Self::Bool(true),
                "false" => Self::Bool(false),
                "0" => Self::Int(0),
                "1" => Self::Int(1),
                _ => Self::Invalid(s.into()),
            }
        }
    }
}
impl From<String> for Val {
    fn from(mut s: String) -> Self {
        match s.as_bytes().first() {
            Some(b'%') => {
                s.remove(0);
                Self::Local(s)
            }
            Some(b'@') => {
                s.remove(0);
                Self::Global(s)
            }
            _ => Self::from(s.as_str()),
        }
    }
}
impl From<&String> for Val {
    fn from(s: &String) -> Self {
        Self::from(s.as_str())
    }
}
impl From<&Val> for Val {
    fn from(v: &Val) -> Self {
        v.clone()
    }
}
impl From<bool> for Val {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}
impl From<&bool> for Val {
    fn from(v: &bool) -> Self {
        Self::Bool(*v)
    }
}
macro_rules! integers { ($($t:ty),*) => {$(
    impl From<$t> for Val { fn from(v:$t)->Self { Self::Int(v as i128) } }
    impl From<&$t> for Val { fn from(v:&$t)->Self { Self::Int(*v as i128) } }
)*}; }
integers!(u8, u16, u32, u64, usize, i8, i16, i32, i64, i128, isize);
pub fn val(v: impl Into<Val>) -> Val {
    v.into()
}
pub fn local(n: String) -> Val {
    Val::Local(n)
}
pub fn global(n: String) -> Val {
    Val::Global(n)
}
pub fn name(n: impl AsRef<str>) -> String {
    n.as_ref()
        .strip_prefix('%')
        .unwrap_or(n.as_ref())
        .to_owned()
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Arg {
    pub ty: Ty,
    pub value: Val,
    pub attrs: Vec<Attr>,
}
impl Arg {
    pub fn new(ty: Ty, value: impl Into<Val>) -> Self {
        Self {
            ty,
            value: value.into(),
            attrs: vec![],
        }
    }
    pub fn attributed(mut self, attrs: Vec<Attr>) -> Self {
        self.attrs = attrs;
        self
    }
}
impl fmt::Display for Arg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ", self.ty)?;
        for a in &self.attrs {
            write!(f, "{} ", a.name())?;
        }
        write!(f, "{}", self.value)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Param {
    pub ty: Ty,
    pub name: Option<String>,
    pub attrs: Vec<Attr>,
}
impl Param {
    pub fn new(ty: Ty, name: impl Into<String>) -> Self {
        Self {
            ty,
            name: Some(name.into()),
            attrs: vec![],
        }
    }
    pub fn unnamed(ty: Ty) -> Self {
        Self {
            ty,
            name: None,
            attrs: vec![],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Signature {
    pub result: Ty,
    pub result_attrs: Vec<Attr>,
    pub params: Vec<Param>,
    pub variadic: bool,
}
impl Signature {
    pub fn new(result: Ty, params: Vec<Param>) -> Self {
        Self {
            result,
            result_attrs: vec![],
            params,
            variadic: false,
        }
    }
    pub fn same_abi(&self, other: &Self) -> bool {
        self.result == other.result
            && self.result_attrs == other.result_attrs
            && self.variadic == other.variadic
            && self.params.len() == other.params.len()
            && self
                .params
                .iter()
                .zip(&other.params)
                .all(|(a, b)| a.ty == b.ty && a.attrs == b.attrs)
    }
}
/// Actual LLVM memory orders. Ordinary Load/Store remain non-atomic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AtomicOrdering {
    Relaxed,
    Acquire,
    Release,
    AcqRel,
    SeqCst,
}
impl AtomicOrdering {
    pub fn spelling(self) -> &'static str {
        match self {
            Self::Relaxed => "monotonic",
            Self::Acquire => "acquire",
            Self::Release => "release",
            Self::AcqRel => "acq_rel",
            Self::SeqCst => "seq_cst",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AtomicRmwOp {
    Exchange,
    Add,
    Subtract,
    And,
    Or,
    Xor,
}
impl AtomicRmwOp {
    pub fn spelling(self) -> &'static str {
        match self {
            Self::Exchange => "xchg",
            Self::Add => "add",
            Self::Subtract => "sub",
            Self::And => "and",
            Self::Or => "or",
            Self::Xor => "xor",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Op {
    Alloca {
        ty: Ty,
        align: u32,
    },
    Load {
        ty: Ty,
        ptr: Val,
        align: u32,
    },
    Store {
        arg: Arg,
        ptr: Val,
        align: u32,
    },
    AtomicLoad {
        ty: Ty,
        ptr: Val,
        align: u32,
        ordering: AtomicOrdering,
    },
    AtomicStore {
        arg: Arg,
        ptr: Val,
        align: u32,
        ordering: AtomicOrdering,
    },
    AtomicRmw {
        operation: AtomicRmwOp,
        arg: Arg,
        ptr: Val,
        align: u32,
        ordering: AtomicOrdering,
    },
    /// Strong process-scope compare/exchange; result must be exactly
    /// {expected integer type, i1}. Orders are independently preflighted.
    AtomicCompareExchange {
        result: Ty,
        expected: Arg,
        desired: Val,
        ptr: Val,
        align: u32,
        success: AtomicOrdering,
        failure: AtomicOrdering,
    },
    Binary {
        opcode: String,
        ty: Ty,
        left: Val,
        right: Val,
    },
    Compare {
        predicate: String,
        ty: Ty,
        left: Val,
        right: Val,
    },
    Cast {
        opcode: String,
        arg: Arg,
        to: Ty,
    },
    Gep {
        ty: Ty,
        ptr: Val,
        indices: Vec<Arg>,
    },
    Extract {
        arg: Arg,
        index: u32,
    },
    Insert {
        arg: Arg,
        field: Arg,
        index: u32,
    },
    Call {
        result: Ty,
        result_attrs: Vec<Attr>,
        callee: Val,
        args: Vec<Arg>,
        vararg: Option<Vec<Ty>>,
    },
    Freeze(Arg),
    Phi {
        ty: Ty,
        incoming: Vec<(Val, String)>,
    },
    Branch(String),
    CondBranch {
        condition: Val,
        yes: String,
        no: String,
    },
    Return(Option<Arg>),
    Unreachable,
    /// Scalar integer selection; an unselected poison arm is not observed.
    Select { ty: Ty, condition: Val, yes: Val, no: Val },
}
impl Op {
    pub fn terminator(&self) -> bool {
        matches!(
            self,
            Self::Branch(_) | Self::CondBranch { .. } | Self::Return(_) | Self::Unreachable
        )
    }
    /// Borrow operands in semantic order without allocating a list per instruction.
    pub fn values(&self) -> impl Iterator<Item = &Val> {
        type OperandParts<'a> = (
            Option<&'a Val>,
            Option<&'a Val>,
            &'a [Arg],
            &'a [(Val, String)],
        );
        let (first, second, args, incoming): OperandParts<'_> = match self {
            Self::Alloca { .. } | Self::Branch(_) | Self::Unreachable | Self::Return(None) => {
                (None, None, &[], &[])
            }
            Self::Load { ptr, .. } | Self::AtomicLoad { ptr, .. } => (Some(ptr), None, &[], &[]),
            Self::Store { arg, ptr, .. }
            | Self::AtomicStore { arg, ptr, .. }
            | Self::AtomicRmw { arg, ptr, .. } => (Some(&arg.value), Some(ptr), &[], &[]),
            Self::AtomicCompareExchange {
                expected,
                desired,
                ptr,
                ..
            } => (
                Some(ptr),
                Some(desired),
                std::slice::from_ref(expected),
                &[],
            ),
            Self::Binary { left, right, .. } | Self::Compare { left, right, .. } => {
                (Some(left), Some(right), &[], &[])
            }
            Self::Cast { arg, .. }
            | Self::Extract { arg, .. }
            | Self::Freeze(arg)
            | Self::Return(Some(arg)) => (Some(&arg.value), None, &[], &[]),
            Self::Gep { ptr, indices, .. } => (Some(ptr), None, indices, &[]),
            Self::Insert { arg, field, .. } => (Some(&arg.value), Some(&field.value), &[], &[]),
            Self::Call { callee, args, .. } => (Some(callee), None, args, &[]),
            Self::Phi { incoming, .. } => (None, None, &[], incoming),
            Self::CondBranch { condition, .. } => (Some(condition), None, &[], &[]),
            Self::Select { condition, yes, .. } => (Some(condition), Some(yes), &[], &[]),
        };
        first
            .into_iter()
            .chain(second)
            .chain(match self { Self::Select { no, .. } => Some(no), _ => None })
            .chain(args.iter().map(|a| &a.value))
            .chain(incoming.iter().map(|(value, _)| value))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Inst {
    pub result: Option<String>,
    pub op: Op,
}
impl Inst {
    pub fn new(result: Option<String>, op: Op) -> Self {
        Self { result, op }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Block {
    pub name: String,
    pub instructions: Vec<Inst>,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Function {
    pub name: String,
    pub linkage: Linkage,
    pub signature: Signature,
    pub attrs: Vec<Attr>,
    /// None is a declaration, Some blocks is a definition.
    pub blocks: Option<SharedVec<Block>>,
}
impl Function {
    pub fn declaration(&self) -> Self {
        Self {
            name: self.name.clone(),
            linkage: Linkage::External,
            signature: Signature {
                result: self.signature.result.clone(),
                result_attrs: self.signature.result_attrs.clone(),
                params: self
                    .signature
                    .params
                    .iter()
                    .map(|p| Param {
                        ty: p.ty.clone(),
                        name: None,
                        attrs: p.attrs.clone(),
                    })
                    .collect(),
                variadic: self.signature.variadic,
            },
            attrs: self.attrs.clone(),
            blocks: None,
        }
    }
    pub fn references(&self) -> BTreeSet<String> {
        let mut refs = BTreeSet::new();
        for b in self.blocks.iter().flatten() {
            for i in &b.instructions {
                for v in i.op.values() {
                    v.symbols(&mut refs);
                }
            }
        }
        refs
    }
    pub fn cost(&self) -> usize {
        self.blocks
            .as_ref()
            .map_or(1, |b| b.iter().map(|x| x.instructions.len() + 1).sum())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Global {
    pub name: String,
    pub linkage: Linkage,
    pub ty: Ty,
    pub initializer: Option<Val>,
    pub constant: bool,
    pub unnamed_addr: bool,
    pub alignment: u32,
}
impl Global {
    pub fn declaration(&self) -> Self {
        Self {
            name: self.name.clone(),
            linkage: Linkage::External,
            ty: self.ty.clone(),
            initializer: None,
            constant: self.constant,
            unnamed_addr: self.unnamed_addr,
            alignment: self.alignment,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Item {
    NamedType(String, Vec<Ty>),
    Global(Global),
    Function(Function),
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Module {
    pub triple: String,
    pub fingerprint: String,
    pub items: SharedVec<Item>,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Event {
    NamedType(String, Vec<Ty>),
    Global(Global),
    Declare(Function),
    Begin(Function),
    Label(String),
    Instruction(Inst),
    End,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Code(pub Vec<Event>);
impl Code {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn append(&mut self, other: impl Into<Code>) {
        self.0.extend(other.into().0);
    }
    pub fn push(&mut self, event: Event) {
        self.0.push(event);
    }
}
impl From<Event> for Code {
    fn from(e: Event) -> Self {
        Self(vec![e])
    }
}
impl From<Vec<Event>> for Code {
    fn from(e: Vec<Event>) -> Self {
        Self(e)
    }
}
impl From<&Code> for Code {
    fn from(c: &Code) -> Self {
        c.clone()
    }
}

/// Consume lowering events directly into the owned machine model. Structural
/// errors are retained until `into_items`, so a producer's own error or budget
/// rejection keeps its original precedence over event validation.
#[derive(Debug, Default)]
pub struct ItemsBuilder {
    items: Vec<Item>,
    active: Option<Function>,
    error: Option<&'static str>,
}
impl ItemsBuilder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn append(&mut self, code: impl Into<Code>) {
        for event in code.into().0 {
            self.push(event);
        }
    }
    pub fn push(&mut self, event: Event) {
        if self.error.is_some() {
            return;
        }
        match event {
            Event::Begin(mut f) if self.active.is_none() => {
                f.blocks = Some(Vec::new().into());
                self.active = Some(f);
            }
            Event::Label(name) => {
                if let Some(f) = self.active.as_mut() {
                    f.blocks.as_mut().unwrap().push(Block {
                        name,
                        instructions: vec![],
                    });
                } else {
                    self.error = Some("block outside function");
                }
            }
            Event::Instruction(i) => {
                if let Some(block) = self
                    .active
                    .as_mut()
                    .and_then(|f| f.blocks.as_mut())
                    .and_then(|b| b.last_mut())
                {
                    block.instructions.push(i);
                } else {
                    self.error = Some("instruction outside block");
                }
            }
            Event::End => {
                if let Some(f) = self.active.take() {
                    self.items.push(Item::Function(f));
                } else {
                    self.error = Some("unmatched function end");
                }
            }
            Event::NamedType(n, fields) if self.active.is_none() => {
                self.items.push(Item::NamedType(n, fields));
            }
            Event::Global(g) if self.active.is_none() => self.items.push(Item::Global(g)),
            Event::Declare(f) if self.active.is_none() => self.items.push(Item::Function(f)),
            _ => self.error = Some("malformed lowering event structure"),
        }
    }
    pub fn into_items(self) -> Result<Vec<Item>, String> {
        if let Some(error) = self.error {
            return Err(error.into());
        }
        if self.active.is_some() {
            return Err("unterminated function".into());
        }
        Ok(self.items)
    }
}
impl Code {
    /// The retained-event API and streaming producers share one structural
    /// state machine; neither grants native validation authority.
    pub fn into_items(self) -> Result<Vec<Item>, String> {
        let mut items = ItemsBuilder::new();
        items.append(self);
        items.into_items()
    }
}

impl Module {
    pub fn from_code(triple: String, fingerprint: String, code: Code) -> Result<Self, String> {
        Ok(Self {
            triple,
            fingerprint,
            items: code.into_items()?.into(),
        })
    }
    pub fn to_text(&self) -> String {
        render_module(self)
    }
}

// Text is an exact diagnostic/reference rendering of the same owned model.
// Keep public Display independent; the former renderer is a cfg(test) oracle.
fn render_module(module: &Module) -> String {
    let mut s = String::new();
    s.push_str("; ZIL verified LLVM module\n; verified-fingerprint: ");
    s.push_str(&module.fingerprint);
    s.push_str("\ntarget triple = \"");
    s.push_str(&module.triple);
    s.push_str("\"\n\n");
    for item in &module.items {
        render_item(&mut s, item);
    }
    s
}

/// Render exactly one machine item, without a module header. This is the same
/// body formatter used by `Module::to_text`; it grants no validation authority.
pub fn item_to_text(item: &Item) -> String {
    let mut text = String::new();
    render_item(&mut text, item);
    text
}
fn render_item(s: &mut String, item: &Item) {
    match item {
        Item::NamedType(name, fields) => {
            s.push('%');
            s.push_str(name);
            s.push_str(" = type ");
            if fields.is_empty() {
                s.push_str("{  }");
            } else {
                write_fields(s, fields);
            }
            s.push('\n');
        }
        Item::Global(global) => {
            s.push('@');
            s.push_str(&global.name);
            s.push_str(" = ");
            s.push_str(global.linkage.prefix());
            if global.initializer.is_none() {
                s.push_str("external ");
            }
            if global.unnamed_addr {
                s.push_str("unnamed_addr ");
            }
            s.push_str(if global.constant {
                "constant "
            } else {
                "global "
            });
            write_type(s, &global.ty);
            if let Some(value) = &global.initializer {
                s.push(' ');
                write_value(s, value);
            }
            write_alignment(s, global.alignment);
            s.push('\n');
        }
        Item::Function(function) => {
            s.push_str(if function.blocks.is_some() {
                "define "
            } else {
                "declare "
            });
            s.push_str(function.linkage.prefix());
            write_attrs(s, &function.signature.result_attrs);
            write_type(s, &function.signature.result);
            s.push_str(" @");
            s.push_str(&function.name);
            s.push('(');
            for (index, parameter) in function.signature.params.iter().enumerate() {
                if index != 0 {
                    s.push_str(", ");
                }
                write_type(s, &parameter.ty);
                for attr in &parameter.attrs {
                    s.push(' ');
                    s.push_str(attr.name());
                }
                if let Some(name) = &parameter.name {
                    s.push_str(" %");
                    s.push_str(name);
                }
            }
            if function.signature.variadic {
                if !function.signature.params.is_empty() {
                    s.push_str(", ");
                }
                s.push_str("...");
            }
            s.push(')');
            for attr in &function.attrs {
                s.push(' ');
                s.push_str(attr.name());
            }
            if let Some(blocks) = &function.blocks {
                s.push_str(" {\n");
                for block in blocks {
                    s.push_str(&block.name);
                    s.push_str(":\n");
                    for instruction in &block.instructions {
                        write_inst(s, instruction);
                    }
                }
                s.push_str("}\n");
            } else {
                s.push('\n');
            }
        }
    }
}

fn write_attrs(s: &mut String, attrs: &[Attr]) {
    for attr in attrs {
        s.push_str(attr.name());
        s.push(' ');
    }
}

fn write_fields(s: &mut String, fields: &[Ty]) {
    if fields.is_empty() {
        s.push_str("{}");
        return;
    }
    s.push_str("{ ");
    for (index, field) in fields.iter().enumerate() {
        if index != 0 {
            s.push_str(", ");
        }
        write_type(s, field);
    }
    s.push_str(" }");
}

fn write_type(s: &mut String, ty: &Ty) {
    match ty {
        Ty::Void => s.push_str("void"),
        Ty::F32 => s.push_str("float"),
        Ty::F64 => s.push_str("double"),
        Ty::Int(bits) => match bits {
            1 => s.push_str("i1"),
            8 => s.push_str("i8"),
            16 => s.push_str("i16"),
            32 => s.push_str("i32"),
            64 => s.push_str("i64"),
            _ => write!(s, "i{bits}").unwrap(),
        },
        Ty::Ptr => s.push_str("ptr"),
        Ty::Array(length, element) => {
            write!(s, "[{length} x ").unwrap();
            write_type(s, element);
            s.push(']');
        }
        Ty::Struct(fields) => write_fields(s, fields),
        Ty::Named(name) => {
            s.push('%');
            s.push_str(name);
        }
    }
}

fn write_value(s: &mut String, value: &Val) {
    match value {
        Val::Local(name) => {
            s.push('%');
            s.push_str(name);
        }
        Val::Global(name) => {
            s.push('@');
            s.push_str(name);
        }
        Val::Int(value) => write!(s, "{value}").unwrap(),
        Val::F32(bits) => write!(s, "bitcast (i32 {bits} to float)").unwrap(),
        Val::F64(bits) => write!(s, "bitcast (i64 {bits} to double)").unwrap(),
        Val::Bool(value) => s.push_str(if *value { "true" } else { "false" }),
        Val::Null => s.push_str("null"),
        Val::Zero => s.push_str("zeroinitializer"),
        Val::Undef => s.push_str("undef"),
        Val::Poison => s.push_str("poison"),
        Val::Array(args) => {
            s.push('[');
            write_arguments(s, args);
            s.push(']');
        }
        Val::Bytes(bytes) => {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            s.push_str("c\"");
            for &byte in bytes {
                if (32..=126).contains(&byte) && byte != b'\\' && byte != b'"' {
                    s.push(char::from(byte));
                } else {
                    s.push('\\');
                    s.push(char::from(HEX[usize::from(byte >> 4)]));
                    s.push(char::from(HEX[usize::from(byte & 15)]));
                }
            }
            s.push('"');
        }
        Val::Gep { ty, ptr, indices } => {
            s.push_str("getelementptr (");
            write_type(s, ty);
            s.push_str(", ptr ");
            write_value(s, ptr);
            for index in indices {
                s.push_str(", ");
                write_argument(s, index);
            }
            s.push(')');
        }
        Val::Invalid(value) => {
            s.push_str("<invalid:");
            s.push_str(value);
            s.push('>');
        }
    }
}

fn write_argument(s: &mut String, arg: &Arg) {
    write_type(s, &arg.ty);
    s.push(' ');
    write_attrs(s, &arg.attrs);
    write_value(s, &arg.value);
}

fn write_arguments(s: &mut String, args: &[Arg]) {
    for (index, arg) in args.iter().enumerate() {
        if index != 0 {
            s.push_str(", ");
        }
        write_argument(s, arg);
    }
}

fn write_alignment(s: &mut String, alignment: u32) {
    if alignment != 0 {
        write!(s, ", align {alignment}").unwrap();
    }
}

fn write_inst(s: &mut String, instruction: &Inst) {
    s.push_str("  ");
    if let Some(result) = &instruction.result {
        s.push('%');
        s.push_str(result);
        s.push_str(" = ");
    }
    match &instruction.op {
        Op::Alloca { ty, align } => {
            s.push_str("alloca ");
            write_type(s, ty);
            write_alignment(s, *align);
        }
        Op::Load { ty, ptr, align } => {
            s.push_str("load ");
            write_type(s, ty);
            s.push_str(", ptr ");
            write_value(s, ptr);
            write_alignment(s, *align);
        }
        Op::Store { arg, ptr, align } => {
            s.push_str("store ");
            write_argument(s, arg);
            s.push_str(", ptr ");
            write_value(s, ptr);
            write_alignment(s, *align);
        }
        Op::AtomicLoad {
            ty,
            ptr,
            align,
            ordering,
        } => {
            s.push_str("load atomic ");
            write_type(s, ty);
            s.push_str(", ptr ");
            write_value(s, ptr);
            write!(s, " {}", ordering.spelling()).unwrap();
            write_alignment(s, *align);
        }
        Op::AtomicStore {
            arg,
            ptr,
            align,
            ordering,
        } => {
            s.push_str("store atomic ");
            write_argument(s, arg);
            s.push_str(", ptr ");
            write_value(s, ptr);
            write!(s, " {}", ordering.spelling()).unwrap();
            write_alignment(s, *align);
        }
        Op::AtomicRmw {
            operation,
            arg,
            ptr,
            align,
            ordering,
        } => {
            write!(s, "atomicrmw {} ptr ", operation.spelling()).unwrap();
            write_value(s, ptr);
            s.push_str(", ");
            write_argument(s, arg);
            write!(s, " {}", ordering.spelling()).unwrap();
            write_alignment(s, *align);
        }
        Op::AtomicCompareExchange {
            expected,
            desired,
            ptr,
            align,
            success,
            failure,
            ..
        } => {
            s.push_str("cmpxchg ptr ");
            write_value(s, ptr);
            s.push_str(", ");
            write_argument(s, expected);
            s.push_str(", ");
            write_type(s, &expected.ty);
            s.push(' ');
            write_value(s, desired);
            write!(s, " {} {}", success.spelling(), failure.spelling()).unwrap();
            write_alignment(s, *align);
        }
        Op::Binary {
            opcode,
            ty,
            left,
            right,
        } => {
            s.push_str(opcode);
            s.push(' ');
            write_binary_operands(s, ty, left, right);
        }
        Op::Compare {
            predicate,
            ty,
            left,
            right,
        } => {
            s.push_str(if matches!(ty, Ty::F32 | Ty::F64) {
                "fcmp "
            } else {
                "icmp "
            });
            s.push_str(predicate);
            s.push(' ');
            write_binary_operands(s, ty, left, right);
        }
        Op::Cast { opcode, arg, to } => {
            s.push_str(opcode);
            s.push(' ');
            write_argument(s, arg);
            s.push_str(" to ");
            write_type(s, to);
        }
        Op::Gep { ty, ptr, indices } => {
            s.push_str("getelementptr ");
            write_type(s, ty);
            s.push_str(", ptr ");
            write_value(s, ptr);
            for index in indices {
                s.push_str(", ");
                write_argument(s, index);
            }
        }
        Op::Extract { arg, index } => {
            s.push_str("extractvalue ");
            write_argument(s, arg);
            write!(s, ", {index}").unwrap();
        }
        Op::Insert { arg, field, index } => {
            s.push_str("insertvalue ");
            write_argument(s, arg);
            s.push_str(", ");
            write_argument(s, field);
            write!(s, ", {index}").unwrap();
        }
        Op::Call {
            result,
            result_attrs,
            callee,
            args,
            vararg,
        } => {
            s.push_str("call ");
            write_attrs(s, result_attrs);
            write_type(s, result);
            s.push(' ');
            if let Some(types) = vararg {
                s.push('(');
                for (index, ty) in types.iter().enumerate() {
                    if index != 0 {
                        s.push_str(", ");
                    }
                    write_type(s, ty);
                }
                if !types.is_empty() {
                    s.push_str(", ");
                }
                s.push_str("...) ");
            }
            write_value(s, callee);
            s.push('(');
            write_arguments(s, args);
            s.push(')');
        }
        Op::Freeze(arg) => {
            s.push_str("freeze ");
            write_argument(s, arg);
        }
        Op::Select { ty, condition, yes, no } => {
            s.push_str("select i1 "); write_value(s, condition);
            s.push_str(", "); write_type(s, ty); s.push(' '); write_value(s, yes);
            s.push_str(", "); write_type(s, ty); s.push(' '); write_value(s, no);
        }
        Op::Phi { ty, incoming } => {
            s.push_str("phi ");
            write_type(s, ty);
            s.push(' ');
            for (index, (value, block)) in incoming.iter().enumerate() {
                if index != 0 {
                    s.push_str(", ");
                }
                s.push_str("[ ");
                write_value(s, value);
                s.push_str(", %");
                s.push_str(block);
                s.push_str(" ]");
            }
        }
        Op::Branch(block) => {
            s.push_str("br label %");
            s.push_str(block);
        }
        Op::CondBranch { condition, yes, no } => {
            s.push_str("br i1 ");
            write_value(s, condition);
            s.push_str(", label %");
            s.push_str(yes);
            s.push_str(", label %");
            s.push_str(no);
        }
        Op::Return(Some(arg)) => {
            s.push_str("ret ");
            write_argument(s, arg);
        }
        Op::Return(None) => s.push_str("ret void"),
        Op::Unreachable => s.push_str("unreachable"),
    }
    s.push('\n');
}

fn write_binary_operands(s: &mut String, ty: &Ty, left: &Val, right: &Val) {
    write_type(s, ty);
    s.push(' ');
    write_value(s, left);
    s.push_str(", ");
    write_value(s, right);
}

#[cfg(test)]
#[path = "text_reference_tests.rs"]
mod text_reference_tests;

pub fn type_name(ty: &Ty) -> String {
    if let Ty::Named(name) = ty {
        name.clone()
    } else {
        "invalid.named.type".into()
    }
}

#[cfg(test)]
mod sharing_tests {
    use super::*;
    #[test]
    fn machine_module_and_function_clones_keep_immutable_body_allocations() {
        let function = Function {
            name: "body".into(),
            linkage: Linkage::External,
            signature: Signature::new(Ty::Void, vec![]),
            attrs: vec![],
            blocks: Some(
                vec![Block {
                    name: "entry".into(),
                    instructions: vec![Inst::new(None, Op::Return(None))],
                }]
                .into(),
            ),
        };
        let original = Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: String::new(),
            items: vec![Item::Function(function.clone())].into(),
        };
        let mut clone = original.clone();
        assert!(original.items.ptr_eq(&clone.items));
        let Item::Function(changed) = &mut clone.items[0] else {
            unreachable!()
        };
        // COW of the module copies function headers while retaining CFG storage.
        assert!(
            function
                .blocks
                .as_ref()
                .unwrap()
                .ptr_eq(changed.blocks.as_ref().unwrap())
        );
        changed.name.push_str(".renamed");
        assert!(!original.items.ptr_eq(&clone.items));
        let Item::Function(original_function) = &original.items[0] else {
            unreachable!()
        };
        assert_eq!(original_function.name, "body");
        let Item::Function(changed) = &mut clone.items[0] else {
            unreachable!()
        };
        changed.blocks.as_mut().unwrap()[0]
            .name
            .push_str(".changed");
        assert!(
            !original_function
                .blocks
                .as_ref()
                .unwrap()
                .ptr_eq(changed.blocks.as_ref().unwrap())
        );
        assert_eq!(original_function.blocks.as_ref().unwrap()[0].name, "entry");
    }
}

#[cfg(test)]
mod item_text_tests {
    use super::*;
    #[test]
    fn body_only_items_have_exact_spelling_and_compose_the_full_module() {
        let items = vec![
            Item::NamedType("empty".into(), vec![]),
            Item::Global(Global {
                name: "value".into(),
                linkage: Linkage::Private,
                ty: Ty::Int(32),
                initializer: Some(Val::Int(7)),
                constant: true,
                unnamed_addr: false,
                alignment: 4,
            }),
            Item::Function(Function {
                name: "decl".into(),
                linkage: Linkage::External,
                signature: Signature::new(Ty::Void, vec![]),
                attrs: vec![],
                blocks: None,
            }),
            Item::Function(Function {
                name: "body".into(),
                linkage: Linkage::External,
                signature: Signature::new(Ty::Void, vec![]),
                attrs: vec![],
                blocks: Some(
                    vec![Block {
                        name: "entry".into(),
                        instructions: vec![Inst::new(None, Op::Return(None))],
                    }]
                    .into(),
                ),
            }),
        ];
        let expected = [
            "%empty = type {  }\n",
            "@value = private constant i32 7, align 4\n",
            "declare void @decl()\n",
            "define void @body() {\nentry:\n  ret void\n}\n",
        ];
        for (item, expected) in items.iter().zip(expected) {
            assert_eq!(item_to_text(item), expected);
        }
        let mut module = Module {
            triple: "x86_64-unknown-linux-gnu".into(),
            fingerprint: "current".into(),
            items: Vec::new().into(),
        };
        let header = module.to_text();
        module.items = items.into();
        assert_eq!(module.to_text(), header + &expected.concat());
    }
}

#[cfg(test)]
mod streaming_items_tests {
    use super::*;

    fn function(name: &str) -> Function {
        Function {
            name: name.into(),
            linkage: Linkage::External,
            signature: Signature::new(Ty::Int(32), vec![]),
            attrs: vec![],
            blocks: None,
        }
    }
    fn instruction() -> Inst {
        Inst::new(None, Op::Return(Some(Arg::new(Ty::Int(32), Val::Int(7)))))
    }

    #[test]
    fn streaming_and_retained_events_preserve_complete_items_across_append_boundaries() {
        let events = vec![
            Event::NamedType("pair".into(), vec![Ty::Int(32), Ty::Int(8)]),
            Event::Declare(function("declaration")),
            Event::Begin(function("body")),
            Event::Label("entry".into()),
            Event::Instruction(Inst::new(None, Op::Branch("exit".into()))),
            Event::Label("exit".into()),
            Event::Instruction(instruction()),
            Event::End,
            Event::Declare(function("after")),
        ];
        let mut body = function("body");
        body.blocks = Some(
            vec![
                Block {
                    name: "entry".into(),
                    instructions: vec![Inst::new(None, Op::Branch("exit".into()))],
                },
                Block {
                    name: "exit".into(),
                    instructions: vec![instruction()],
                },
            ]
            .into(),
        );
        let expected = vec![
            Item::NamedType("pair".into(), vec![Ty::Int(32), Ty::Int(8)]),
            Item::Function(function("declaration")),
            Item::Function(body),
            Item::Function(function("after")),
        ];
        assert_eq!(Code::from(events.clone()).into_items().unwrap(), expected);
        for first in 0..=events.len() {
            for second in first..=events.len() {
                let mut streaming = ItemsBuilder::new();
                streaming.append(Code::from(events[..first].to_vec()));
                for event in &events[first..second] {
                    streaming.push(event.clone());
                }
                streaming.append(Code::from(events[second..].to_vec()));
                let actual = streaming.into_items().unwrap();
                assert_eq!(actual, expected, "fragment boundaries {first}/{second}");
                let module = |items: Vec<Item>| Module {
                    triple: "x86_64-unknown-linux-gnu".into(),
                    fingerprint: "current".into(),
                    items: items.into(),
                };
                assert_eq!(module(actual).to_text(), module(expected.clone()).to_text());
            }
        }
    }

    #[test]
    fn streaming_defers_structural_errors_and_retains_the_first_error() {
        let cases = [
            (
                vec![Event::End, Event::Label("later".into())],
                "unmatched function end",
            ),
            (
                vec![Event::Label("outside".into()), Event::End],
                "block outside function",
            ),
            (
                vec![Event::Instruction(instruction()), Event::End],
                "instruction outside block",
            ),
            (
                vec![
                    Event::Begin(function("body")),
                    Event::Instruction(instruction()),
                ],
                "instruction outside block",
            ),
            (
                vec![
                    Event::Begin(function("body")),
                    Event::Begin(function("nested")),
                ],
                "malformed lowering event structure",
            ),
            (
                vec![
                    Event::Begin(function("body")),
                    Event::NamedType("inside".into(), vec![]),
                ],
                "malformed lowering event structure",
            ),
            (
                vec![
                    Event::Begin(function("body")),
                    Event::Declare(function("inside")),
                ],
                "malformed lowering event structure",
            ),
            (
                vec![Event::Begin(function("body"))],
                "unterminated function",
            ),
        ];
        for (events, expected) in cases {
            assert_eq!(
                Code::from(events.clone()).into_items().unwrap_err(),
                expected
            );
            for split in 0..=events.len() {
                let mut streaming = ItemsBuilder::new();
                streaming.append(Code::from(events[..split].to_vec()));
                // Pushing a malformed fragment never returns an early error;
                // producer work and any later producer error still take place.
                let mut continued = 0;
                for event in &events[split..] {
                    streaming.push(event.clone());
                    continued += 1;
                }
                assert_eq!(continued, events.len() - split);
                assert_eq!(streaming.into_items().unwrap_err(), expected);
            }
        }
        fn producer(fail: bool) -> Result<Vec<Item>, String> {
            let mut streaming = ItemsBuilder::new();
            streaming.push(Event::End);
            if fail {
                return Err("current producer budget failure".into());
            }
            streaming.into_items()
        }
        assert_eq!(
            producer(true).unwrap_err(),
            "current producer budget failure"
        );
        assert_eq!(producer(false).unwrap_err(), "unmatched function end");
    }

    #[test]
    fn streaming_consumes_instructions_before_the_function_finishes() {
        let mut streaming = ItemsBuilder::new();
        // Begin retains the old behavior of discarding any supplied blocks.
        let mut header = function("body");
        header.blocks = Some(
            vec![Block {
                name: "discard".into(),
                instructions: vec![],
            }]
            .into(),
        );
        streaming.push(Event::Begin(header));
        streaming.append(Code::from(vec![Event::Label("entry".into())]));
        for _ in 0..1024 {
            streaming.push(Event::Instruction(instruction()));
        }
        let blocks = streaming.active.as_ref().unwrap().blocks.as_ref().unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].name, "entry");
        assert_eq!(blocks[0].instructions.len(), 1024);
        streaming.push(Event::End);
        assert!(streaming.active.is_none());
        assert_eq!(streaming.items.len(), 1);
        assert_eq!(streaming.into_items().unwrap().len(), 1);
    }
}
