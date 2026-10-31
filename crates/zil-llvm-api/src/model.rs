//! Backend-neutral, owned machine IR. No LLVM pointers and no assembly parser.
//! Text is a diagnostic/reference rendering; native codegen consumes these nodes.
use std::collections::BTreeSet;
use std::fmt::{self, Write};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Ty {
    Void,
    Int(u32),
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
}
impl Attr {
    pub fn name(self) -> &'static str {
        match self {
            Self::ZeroExt => "zeroext",
            Self::NonLazyBind => "nonlazybind",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Val {
    Local(String),
    Global(String),
    Int(i128),
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
            Self::Load { ptr, .. } => (Some(ptr), None, &[], &[]),
            Self::Store { arg, ptr, .. } => (Some(&arg.value), Some(ptr), &[], &[]),
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
        };
        first
            .into_iter()
            .chain(second)
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
    pub blocks: Option<Vec<Block>>,
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
    pub items: Vec<Item>,
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

impl Code {
    /// Finish a bounded lowering fragment before accumulating the next one.
    /// Moving events into items per function avoids a whole-module event tape
    /// coexisting with the final machine IR. Event validation and order are unchanged.
    pub fn into_items(self) -> Result<Vec<Item>, String> {
        let mut items = vec![];
        let mut active: Option<Function> = None;
        for event in self.0 {
            match event {
                Event::Begin(mut f) if active.is_none() => {
                    f.blocks = Some(vec![]);
                    active = Some(f);
                }
                Event::Label(name) => {
                    let f = active.as_mut().ok_or("block outside function")?;
                    f.blocks.as_mut().unwrap().push(Block {
                        name,
                        instructions: vec![],
                    });
                }
                Event::Instruction(i) => {
                    active
                        .as_mut()
                        .and_then(|f| f.blocks.as_mut())
                        .and_then(|b| b.last_mut())
                        .ok_or("instruction outside block")?
                        .instructions
                        .push(i);
                }
                Event::End => items.push(Item::Function(
                    active.take().ok_or("unmatched function end")?,
                )),
                Event::NamedType(n, fields) if active.is_none() => {
                    items.push(Item::NamedType(n, fields))
                }
                Event::Global(g) if active.is_none() => items.push(Item::Global(g)),
                Event::Declare(f) if active.is_none() => items.push(Item::Function(f)),
                _ => return Err("malformed lowering event structure".into()),
            }
        }
        if active.is_some() {
            return Err("unterminated function".into());
        }
        Ok(items)
    }
}

impl Module {
    pub fn from_code(triple: String, fingerprint: String, code: Code) -> Result<Self, String> {
        Ok(Self {
            triple,
            fingerprint,
            items: code.into_items()?,
        })
    }
    pub fn to_text(&self) -> String {
        let mut s = format!(
            "; ZIL verified LLVM module\n; verified-fingerprint: {}\ntarget triple = \"{}\"\n\n",
            self.fingerprint, self.triple
        );
        for item in &self.items {
            match item {
                Item::NamedType(n, fields) => {
                    if fields.is_empty() {
                        writeln!(s, "%{n} = type {{  }}").unwrap();
                    } else {
                        writeln!(s, "%{n} = type {}", Ty::Struct(fields.clone())).unwrap();
                    }
                }
                Item::Global(g) => {
                    write!(s, "@{} = {}", g.name, g.linkage.prefix()).unwrap();
                    if g.initializer.is_none() {
                        s.push_str("external ");
                    }
                    if g.unnamed_addr {
                        s.push_str("unnamed_addr ");
                    }
                    write!(
                        s,
                        "{} {}",
                        if g.constant { "constant" } else { "global" },
                        g.ty
                    )
                    .unwrap();
                    if let Some(v) = &g.initializer {
                        write!(s, " {v}").unwrap();
                    }
                    if g.alignment != 0 {
                        write!(s, ", align {}", g.alignment).unwrap();
                    }
                    s.push('\n');
                }
                Item::Function(f) => {
                    write!(
                        s,
                        "{} {}",
                        if f.blocks.is_some() {
                            "define"
                        } else {
                            "declare"
                        },
                        f.linkage.prefix()
                    )
                    .unwrap();
                    for a in &f.signature.result_attrs {
                        write!(s, "{} ", a.name()).unwrap();
                    }
                    write!(s, "{} @{}(", f.signature.result, f.name).unwrap();
                    for (i, p) in f.signature.params.iter().enumerate() {
                        if i != 0 {
                            s.push_str(", ");
                        }
                        write!(s, "{}", p.ty).unwrap();
                        for a in &p.attrs {
                            write!(s, " {}", a.name()).unwrap();
                        }
                        if let Some(n) = &p.name {
                            write!(s, " %{n}").unwrap();
                        }
                    }
                    if f.signature.variadic {
                        if !f.signature.params.is_empty() {
                            s.push_str(", ");
                        }
                        s.push_str("...");
                    }
                    s.push(')');
                    for a in &f.attrs {
                        write!(s, " {}", a.name()).unwrap();
                    }
                    if let Some(blocks) = &f.blocks {
                        s.push_str(" {\n");
                        for b in blocks {
                            writeln!(s, "{}:", b.name).unwrap();
                            for i in &b.instructions {
                                write_inst(&mut s, i);
                            }
                        }
                        s.push_str("}\n");
                    } else {
                        s.push('\n');
                    }
                }
            }
        }
        s
    }
}
fn write_inst(s: &mut String, i: &Inst) {
    s.push_str("  ");
    if let Some(r) = &i.result {
        write!(s, "%{r} = ").unwrap();
    }
    match &i.op {
        Op::Alloca { ty, align } => {
            write!(s, "alloca {ty}").unwrap();
            if *align != 0 {
                write!(s, ", align {align}").unwrap();
            }
            Ok(())
        }
        Op::Load { ty, ptr, align } => {
            write!(s, "load {ty}, ptr {ptr}").unwrap();
            if *align != 0 {
                write!(s, ", align {align}").unwrap();
            }
            Ok(())
        }
        Op::Store { arg, ptr, align } => {
            write!(s, "store {arg}, ptr {ptr}").unwrap();
            if *align != 0 {
                write!(s, ", align {align}").unwrap();
            }
            Ok(())
        }
        Op::Binary {
            opcode,
            ty,
            left,
            right,
        } => write!(s, "{opcode} {ty} {left}, {right}"),
        Op::Compare {
            predicate,
            ty,
            left,
            right,
        } => write!(s, "icmp {predicate} {ty} {left}, {right}"),
        Op::Cast { opcode, arg, to } => write!(s, "{opcode} {arg} to {to}"),
        Op::Gep { ty, ptr, indices } => {
            write!(s, "getelementptr {ty}, ptr {ptr}").unwrap();
            for a in indices {
                write!(s, ", {a}").unwrap();
            }
            Ok(())
        }
        Op::Extract { arg, index } => write!(s, "extractvalue {arg}, {index}"),
        Op::Insert { arg, field, index } => write!(s, "insertvalue {arg}, {field}, {index}"),
        Op::Call {
            result,
            result_attrs,
            callee,
            args,
            vararg,
        } => {
            s.push_str("call ");
            for a in result_attrs {
                write!(s, "{} ", a.name()).unwrap();
            }
            write!(s, "{result} ").unwrap();
            if let Some(types) = vararg {
                s.push('(');
                for (i, t) in types.iter().enumerate() {
                    if i != 0 {
                        s.push_str(", ");
                    }
                    write!(s, "{t}").unwrap();
                }
                if !types.is_empty() {
                    s.push_str(", ");
                }
                s.push_str("...) ");
            }
            write!(s, "{callee}(").unwrap();
            for (i, a) in args.iter().enumerate() {
                if i != 0 {
                    s.push_str(", ");
                }
                write!(s, "{a}").unwrap();
            }
            s.push(')');
            Ok(())
        }
        Op::Freeze(arg) => write!(s, "freeze {arg}"),
        Op::Phi { ty, incoming } => {
            write!(s, "phi {ty} ").unwrap();
            for (i, (v, b)) in incoming.iter().enumerate() {
                if i != 0 {
                    s.push_str(", ");
                }
                write!(s, "[ {v}, %{b} ]").unwrap();
            }
            Ok(())
        }
        Op::Branch(b) => write!(s, "br label %{b}"),
        Op::CondBranch { condition, yes, no } => {
            write!(s, "br i1 {condition}, label %{yes}, label %{no}")
        }
        Op::Return(Some(a)) => write!(s, "ret {a}"),
        Op::Return(None) => write!(s, "ret void"),
        Op::Unreachable => write!(s, "unreachable"),
    }
    .unwrap();
    s.push('\n');
}

pub fn type_name(ty: &Ty) -> String {
    if let Ty::Named(name) = ty {
        name.clone()
    } else {
        "invalid.named.type".into()
    }
}
