//! Bounded syntax memo codec for a trusted, compiler-identified local producer.
//! The transport owner must bind source bytes, parser rules, attribute context
//! and depth limits. Checksums are not authentication. Decoded AST is unverified.
use crate::{
    ParseResult, ParseUsage,
    ast::*,
    attributes::{DeferredAttribute, DeferredReason},
    token::{Symbol, Token, TokenKind},
};
use std::mem::size_of;
use zil_base::{Limits, ModuleId, SourceUnit, Span};

pub const MAX_PAYLOAD: usize = 32 * 1024 * 1024;
const MAX_ALLOCATION: usize = 64 * 1024 * 1024;
const MAX_DEPTH: usize = 128;
const MAGIC: &[u8; 8] = b"ZILAST01";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheError {
    Invalid,
    Limit,
    Budget,
}
type Result<T> = std::result::Result<T, CacheError>;

pub fn encode(source: &SourceUnit, parsed: &ParseResult) -> Result<Vec<u8>> {
    if !parsed.diagnostics.is_empty() || !valid_root(source, parsed) {
        return Err(CacheError::Invalid);
    }
    let mut w = Writer {
        bytes: Vec::new(),
        depth: 0,
        source,
    };
    w.put(MAGIC)?;
    parsed.usage.nodes_required.encode(&mut w)?;
    parsed.tokens.len().encode(&mut w)?;
    parsed.module.encode(&mut w)?;
    parsed.tokens.encode(&mut w)?;
    parsed.deferred_attributes.encode(&mut w)?;
    parsed.cfg_removed.encode(&mut w)?;
    Ok(w.bytes)
}

pub fn decode(source: &SourceUnit, bytes: &[u8], limits: &Limits) -> Result<ParseResult> {
    if source.text.len() > limits.max_source_bytes {
        return Err(CacheError::Budget);
    }
    if bytes.len() > MAX_PAYLOAD {
        return Err(CacheError::Limit);
    }
    let mut r = Reader {
        bytes,
        position: 0,
        depth: 0,
        allocation: MAX_ALLOCATION,
        source,
    };
    if r.take(8)? != MAGIC {
        return Err(CacheError::Invalid);
    }
    let nodes_required = usize::decode(&mut r)?;
    let tokens = usize::decode(&mut r)?;
    if nodes_required > limits.max_nodes || tokens > limits.max_tokens {
        return Err(CacheError::Budget);
    }
    let result = ParseResult {
        module: Module::decode(&mut r)?,
        tokens: Vec::decode(&mut r)?,
        diagnostics: Vec::new(),
        deferred_attributes: Vec::decode(&mut r)?,
        cfg_removed: Vec::decode(&mut r)?,
        usage: ParseUsage { nodes_required },
    };
    if r.position != bytes.len() || result.tokens.len() != tokens || !valid_root(source, &result) {
        return Err(CacheError::Invalid);
    }
    Ok(result)
}

fn valid_root(source: &SourceUnit, parsed: &ParseResult) -> bool {
    if parsed.module.id != source.module_id
        || parsed.module.span != source.span()
        || parsed.tokens.is_empty()
    {
        return false;
    }
    let mut end = 0;
    for (index, token) in parsed.tokens.iter().enumerate() {
        if !token.span.is_valid_for(source) || token.span.start != end {
            return false;
        }
        if index + 1 == parsed.tokens.len() {
            if token.kind != TokenKind::Eof
                || token.span.start != source.text.len()
                || token.span.end != source.text.len()
            {
                return false;
            }
        } else if matches!(token.kind, TokenKind::Eof | TokenKind::Invalid)
            || token.span.start == token.span.end
        {
            return false;
        }
        end = token.span.end;
    }
    end == source.text.len()
}

struct Writer<'a> {
    bytes: Vec<u8>,
    depth: usize,
    source: &'a SourceUnit,
}
impl Writer<'_> {
    fn put(&mut self, bytes: &[u8]) -> Result<()> {
        if bytes.len() > MAX_PAYLOAD.saturating_sub(self.bytes.len()) {
            return Err(CacheError::Limit);
        }
        self.bytes
            .try_reserve(bytes.len())
            .map_err(|_| CacheError::Limit)?;
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        if self.depth >= MAX_DEPTH {
            return Err(CacheError::Limit);
        }
        self.depth += 1;
        let result = f(self);
        self.depth -= 1;
        result
    }
}
struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
    depth: usize,
    allocation: usize,
    source: &'a SourceUnit,
}
impl Reader<'_> {
    fn take(&mut self, count: usize) -> Result<&[u8]> {
        let end = self
            .position
            .checked_add(count)
            .filter(|end| *end <= self.bytes.len())
            .ok_or(CacheError::Invalid)?;
        let bytes = &self.bytes[self.position..end];
        self.position = end;
        Ok(bytes)
    }
    fn allocate(&mut self, bytes: usize) -> Result<()> {
        self.allocation = self
            .allocation
            .checked_sub(bytes)
            .ok_or(CacheError::Limit)?;
        Ok(())
    }
    fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        if self.depth >= MAX_DEPTH {
            return Err(CacheError::Limit);
        }
        self.depth += 1;
        let result = f(self);
        self.depth -= 1;
        result
    }
}
trait Codec: Sized {
    fn encode(&self, w: &mut Writer<'_>) -> Result<()>;
    fn decode(r: &mut Reader<'_>) -> Result<Self>;
}
impl Codec for u8 {
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.put(&[*self])
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(r.take(1)?[0])
    }
}
impl Codec for u32 {
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.put(&self.to_le_bytes())
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self::from_le_bytes(
            r.take(4)?.try_into().map_err(|_| CacheError::Invalid)?,
        ))
    }
}
impl Codec for usize {
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.put(&(*self as u64).to_le_bytes())
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Self::try_from(u64::from_le_bytes(
            r.take(8)?.try_into().map_err(|_| CacheError::Invalid)?,
        ))
        .map_err(|_| CacheError::Limit)
    }
}
impl Codec for bool {
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        (*self as u8).encode(w)
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        match u8::decode(r)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(CacheError::Invalid),
        }
    }
}
impl Codec for String {
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        u32::try_from(self.len())
            .map_err(|_| CacheError::Limit)?
            .encode(w)?;
        w.put(self.as_bytes())
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let n = u32::decode(r)? as usize;
        r.allocate(n)?;
        let bytes = r.take(n)?;
        let text = std::str::from_utf8(bytes).map_err(|_| CacheError::Invalid)?;
        let mut out = String::new();
        out.try_reserve_exact(n).map_err(|_| CacheError::Limit)?;
        out.push_str(text);
        Ok(out)
    }
}
impl<T: Codec> Codec for Vec<T> {
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        u32::try_from(self.len())
            .map_err(|_| CacheError::Limit)?
            .encode(w)?;
        for item in self {
            item.encode(w)?;
        }
        Ok(())
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let n = u32::decode(r)? as usize;
        if n > r.bytes.len() - r.position {
            return Err(CacheError::Invalid);
        }
        r.allocate(n.checked_mul(size_of::<T>()).ok_or(CacheError::Limit)?)?;
        let mut out = Vec::new();
        out.try_reserve_exact(n).map_err(|_| CacheError::Limit)?;
        for _ in 0..n {
            out.push(T::decode(r)?);
        }
        Ok(out)
    }
}
impl<T: Codec> Codec for Option<T> {
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        self.is_some().encode(w)?;
        if let Some(value) = self {
            value.encode(w)?;
        }
        Ok(())
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        if bool::decode(r)? {
            Ok(Some(T::decode(r)?))
        } else {
            Ok(None)
        }
    }
}
impl<T: Codec> Codec for Box<T> {
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        (**self).encode(w)
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        r.allocate(size_of::<T>())?;
        Ok(Box::new(T::decode(r)?))
    }
}
impl Codec for Span {
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        if !self.is_valid_for(w.source) {
            return Err(CacheError::Invalid);
        }
        u32::try_from(self.start)
            .map_err(|_| CacheError::Limit)?
            .encode(w)?;
        u32::try_from(self.end)
            .map_err(|_| CacheError::Limit)?
            .encode(w)
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let span = Self::new(
            r.source.file_id,
            u32::decode(r)? as usize,
            u32::decode(r)? as usize,
        );
        if !span.is_valid_for(r.source) {
            return Err(CacheError::Invalid);
        }
        Ok(span)
    }
}
impl Codec for ModuleId {
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        self.0.encode(w)
    }
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self(u32::decode(r)?))
    }
}
macro_rules! structure {
    ($name:ident {$($field:ident),* $(,)?}) => {
        impl Codec for $name {
            fn encode(&self,w:&mut Writer<'_>)->Result<()>{w.nested(|w|{$(self.$field.encode(w)?;)*Ok(())})}
            fn decode(r:&mut Reader<'_>)->Result<Self>{r.nested(|r|Ok(Self{$($field:Codec::decode(r)?),*}))}
        }
    };
}
macro_rules! read_field {
    ($r:ident,$field:ident) => {
        Codec::decode($r)?
    };
}
macro_rules! enumeration {
    ($name:ident {$($tag:literal => $variant:ident $(($($value:ident),* ))? $({$($field:ident),*})?),* $(,)?}) => {
        impl Codec for $name {
            fn encode(&self,w:&mut Writer<'_>)->Result<()>{w.nested(|w|match self{$(Self::$variant $(($($value),*))? $({$($field),*})? => {($tag as u8).encode(w)?;$($($value.encode(w)?;)*)?$($($field.encode(w)?;)*)?Ok(())}),*})}
            fn decode(r:&mut Reader<'_>)->Result<Self>{r.nested(|r|Ok(match u8::decode(r)?{$($tag=>Self::$variant $(( $(read_field!(r,$value)),* ))? $({$($field:Codec::decode(r)?),*})?),*,_=>return Err(CacheError::Invalid)}))}
        }
    };
}

structure!(Module { id, items, span });

structure!(Item {
    kind,
    attributes,
    public,
    span
});

enumeration!(ItemKind {
    0 => Import(v0),
    1 => Function(v0),
    2 => Aggregate(v0),
    3 => Enum(v0),
    4 => Trait(v0),
    5 => Impl(v0),
    6 => TypeAlias(v0),
    7 => Const(v0),
    8 => Extern {abi, function},
    9 => Meta(v0)
});

structure!(Import {
    path,
    alias,
    syntax
});

structure!(Function {
    name,
    generics,
    params,
    return_type,
    where_clause,
    contract,
    body,
    span
});

enumeration!(FunctionBody {
    0 => Block(v0),
    1 => Expression(v0),
    2 => Declaration
});

structure!(Param {
    name,
    ty,
    give,
    attributes,
    span
});

structure!(GenericParam { name, kind, span });

enumeration!(GenericParamKind {
    0 => Type(v0),
    1 => Lifetime,
    2 => Const(v0)
});

structure!(BoundClause {
    subject,
    bounds,
    lifetime_bound,
    span
});

enumeration!(BoundSubject {
    0 => Type(v0),
    1 => Lifetime(v0)
});

structure!(TypeRef { kind, span });

enumeration!(TypeKind {
    0 => Named {path, args},
    1 => Reference {lifetime, mutable, inner},
    2 => Tuple(v0),
    3 => Group(v0),
    4 => Array {element, length},
    5 => Function {params, result},
    6 => ForeignFunction {abi, params, result},
    7 => Optional(v0),
    8 => Error {error, result},
    9 => Union(v0),
    10 => Dyn {path, args},
    11 => ForLifetime {lifetime, inner},
    12 => Impl(v0),
    13 => MetaEval(v0),
    14 => MetaBlock(v0)
});

enumeration!(GenericArg {
    0 => Type(v0),
    1 => Const(v0)
});

enumeration!(AggregateKind {
    0 => Struct,
    1 => Resource,
    2 => Package
});

structure!(Aggregate {
    kind,
    name,
    generics,
    fields,
    methods
});

structure!(Field {
    name,
    ty,
    default,
    public,
    manages,
    attributes,
    span
});

structure!(EnumDecl {
    name,
    generics,
    variants
});

structure!(Variant {
    name,
    fields,
    attributes,
    span
});

enumeration!(VariantFields {
    0 => Unit,
    1 => Tuple(v0),
    2 => Record(v0)
});

structure!(TraitDecl {
    name,
    generics,
    bounds,
    members
});

structure!(ImplBlock {
    generics,
    trait_type,
    target,
    receiver_default,
    where_clause,
    members
});

structure!(TypeAlias {
    name,
    generics,
    bounds,
    value
});

structure!(ConstDecl { name, ty, value });

structure!(ContractClause { name, terms, span });

structure!(ContractTerm {
    path,
    type_args,
    arguments,
    index,
    span
});

structure!(Attribute {
    path,
    arguments,
    string_value,
    span
});

structure!(Block { statements, span });

structure!(Stmt { kind, span });

enumeration!(StmtKind {
    0 => Binding {mutable, pattern, ty, value},
    1 => Return(v0),
    2 => Yield(v0),
    3 => Break,
    4 => Continue,
    5 => Defer(v0),
    6 => If {condition, then_body, else_body},
    7 => While {condition, body},
    8 => For {pattern, iterable, body},
    9 => Loop(v0),
    10 => Match {value, arms},
    11 => With {name, value, body},
    12 => Unsafe(v0),
    13 => Assign {target, op, value},
    14 => Expression(v0),
    15 => Meta(v0)
});

enumeration!(Condition {
    0 => Expression(v0),
    1 => Let {pattern, value}
});

enumeration!(ElseBranch {
    0 => Block(v0),
    1 => If(v0)
});

structure!(MatchArm {
    pattern,
    body,
    span
});

enumeration!(AssignOp {
    0 => Set,
    1 => Add,
    2 => Subtract,
    3 => Multiply,
    4 => Divide,
    5 => Remainder,
    6 => BitAnd,
    7 => BitOr,
    8 => BitXor
});

structure!(Pattern { kind, span });

enumeration!(PatternKind {
    0 => Name(v0),
    1 => Wildcard,
    2 => Literal(v0),
    3 => Reference {mutable, pattern},
    4 => Tuple(v0),
    5 => Constructor {path, arguments}
});

structure!(Expr { kind, span });

enumeration!(ExprKind {
    0 => Literal(v0),
    1 => Name(v0),
    2 => ReceiverMember(v0),
    3 => Group(v0),
    4 => Tuple(v0),
    5 => Array(v0),
    6 => Unary {op, operand},
    7 => Binary {op, left, right},
    8 => Pipeline {input, target},
    9 => Member {object, member},
    10 => TypeApply {callee, arguments},
    11 => Call {callee, arguments},
    12 => Index {object, index},
    13 => Slice {object, start, end},
    14 => Try(v0),
    15 => Record {ty, base, fields},
    16 => Closure {move_capture, params, return_type, body},
    17 => If {condition, then_value, else_value},
    18 => MetaEval(v0),
    19 => MetaBlock(v0),
    20 => SyntaxIsland {path, tree}
});

enumeration!(Literal {
    0 => Integer(v0),
    1 => Float(v0),
    2 => String(v0),
    3 => Char(v0),
    4 => Bool(v0)
});

enumeration!(UnaryOp {
    0 => Not,
    1 => Negate,
    2 => BitNot,
    3 => Dereference,
    4 => Borrow,
    5 => BorrowMut,
    6 => Give
});

enumeration!(BinaryOp {
    0 => Multiply,
    1 => Divide,
    2 => Remainder,
    3 => MultiplyWrapping,
    4 => Add,
    5 => Subtract,
    6 => AddWrapping,
    7 => SubtractWrapping,
    8 => ShiftLeft,
    9 => ShiftRight,
    10 => BitAnd,
    11 => BitXor,
    12 => BitOr,
    13 => Range,
    14 => Equal,
    15 => NotEqual,
    16 => Less,
    17 => LessEqual,
    18 => Greater,
    19 => GreaterEqual,
    20 => And,
    21 => Or
});

enumeration!(MemberName {
    0 => Name(v0),
    1 => TupleIndex(v0)
});

structure!(Argument {
    spread,
    value,
    span
});

structure!(RecordField { name, value, span });

structure!(ClosureParam { name, ty, span });

enumeration!(ArrowBody {
    0 => Expression(v0),
    1 => Block(v0)
});

structure!(TokenTree {
    delimiter,
    children,
    span
});

enumeration!(Delimiter {
    0 => Parenthesis,
    1 => Bracket,
    2 => Brace
});

enumeration!(TokenTreeEntry {
    0 => Token {text, span},
    1 => Tree(v0)
});

enumeration!(MetaItem {
    0 => If {condition, then_items, else_items},
    1 => For {binding, iterable, items},
    2 => Emit(v0)
});

enumeration!(MetaStatement {
    0 => If {condition, then_body, else_body},
    1 => For {binding, iterable, body},
    2 => Emit(v0)
});

structure!(Token { kind, span });

enumeration!(TokenKind {
    0 => Identifier,
    1 => Integer,
    2 => Float,
    3 => String,
    4 => Char,
    5 => Lifetime,
    6 => TupleIndex,
    7 => Whitespace,
    8 => Newline,
    9 => LineComment,
    10 => BlockComment,
    11 => Symbol(v0),
    12 => Invalid,
    13 => Eof
});

enumeration!(Symbol {
    0 => LParen,
    1 => RParen,
    2 => LBracket,
    3 => RBracket,
    4 => LBrace,
    5 => RBrace,
    6 => Comma,
    7 => Semicolon,
    8 => Colon,
    9 => Dot,
    10 => DotDot,
    11 => Ellipsis,
    12 => ColonColon,
    13 => Hash,
    14 => Question,
    15 => Arrow,
    16 => FatArrow,
    17 => Pipe,
    18 => Plus,
    19 => Minus,
    20 => Star,
    21 => Slash,
    22 => Percent,
    23 => PlusWrapping,
    24 => MinusWrapping,
    25 => StarWrapping,
    26 => Amp,
    27 => Bar,
    28 => Caret,
    29 => Tilde,
    30 => Bang,
    31 => AmpAmp,
    32 => BarBar,
    33 => Eq,
    34 => EqEq,
    35 => BangEq,
    36 => Lt,
    37 => LtEq,
    38 => Gt,
    39 => GtEq,
    40 => Shl,
    41 => Shr,
    42 => PlusEq,
    43 => MinusEq,
    44 => StarEq,
    45 => SlashEq,
    46 => PercentEq,
    47 => AmpEq,
    48 => BarEq,
    49 => CaretEq
});

structure!(DeferredAttribute { attachment, reason });
enumeration!(DeferredReason {0 => Configuration, 1 => CompileTimeSelection});
