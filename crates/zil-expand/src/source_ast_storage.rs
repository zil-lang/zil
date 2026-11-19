//! Heap storage retained by exactly one derived clone of an original Module.
//! Inline enum/struct fields already occupy their enclosing Vec/Box/Arc slot.
//! This traversal grants no syntax, name, type or lifetime authority. Its
//! caller first bounds every syntax node and recursive depth independently.
use super::*;
use std::mem::size_of;
use zil_syntax::ast as s;

type R<T> = Result<T, zil_base::Diagnostic>;
struct Cost<'a, 'l> {
    budget: &'a mut Budget<'l>,
    span: Span,
}
impl Cost<'_, '_> {
    fn step(&mut self) -> R<()> {
        self.budget.identity_work(1, self.span)
    }
    fn bytes(&mut self, bytes: usize) -> R<()> {
        self.budget.lineage_bytes(bytes, self.span)
    }
}
// Private structural clone accounting, not a semantic interpreter. The
// exhaustive AST matches below make each newly added constructor explicit.
trait Retained {
    fn retained(&self, cost: &mut Cost<'_, '_>) -> R<()>;
}
impl Retained for String {
    fn retained(&self, cost: &mut Cost<'_, '_>) -> R<()> {
        cost.bytes(self.len())
    }
}
impl<T: Retained> Retained for Vec<T> {
    fn retained(&self, cost: &mut Cost<'_, '_>) -> R<()> {
        let bytes = self
            .len()
            .checked_mul(size_of::<T>())
            .ok_or_else(|| bad(cost.span, "original AST vector storage overflow"))?;
        cost.bytes(bytes)?;
        for child in self {
            child.retained(cost)?;
        }
        Ok(())
    }
}
impl<T: Retained> Retained for Box<T> {
    fn retained(&self, cost: &mut Cost<'_, '_>) -> R<()> {
        cost.bytes(size_of::<T>())?;
        self.as_ref().retained(cost)
    }
}
impl<T: Retained> Retained for Option<T> {
    fn retained(&self, cost: &mut Cost<'_, '_>) -> R<()> {
        cost.step()?;
        if let Some(value) = self {
            value.retained(cost)?;
        }
        Ok(())
    }
}
macro_rules! fields {
    ($cost:ident; $($field:ident),* $(,)?) => {{ $( $field.retained($cost)?; )* Ok(()) }};
}
macro_rules! field {
    ($cost:ident, $name:ident, heap) => {
        $name.retained($cost)?;
    };
    ($cost:ident, $name:ident, inline) => {
        let _ = $name;
    };
}
macro_rules! record {
    ($name:ident { $($field:ident: $kind:ident),* $(,)? }) => {
        impl Retained for s::$name {
            fn retained(&self, cost:&mut Cost<'_, '_>)->R<()> {
                cost.step()?;
                let s::$name { $($field),* }=self;
                $(field!(cost,$field,$kind);)*
                Ok(())
            }
        }
    };
}
record!(Module {
    id: inline,
    items: heap,
    span: inline
});
record!(Item {
    kind: heap,
    attributes: heap,
    public: inline,
    span: inline
});
record!(Import {
    path: heap,
    alias: heap,
    syntax: inline
});
record!(Function {
    semantic: inline,
    name: heap,
    generics: heap,
    params: heap,
    return_type: heap,
    where_clause: heap,
    contract: heap,
    body: heap,
    span: inline
});
record!(Param {
    name: heap,
    ty: heap,
    give: inline,
    attributes: heap,
    span: inline
});
record!(GenericParam {
    name: heap,
    kind: heap,
    span: inline
});
record!(BoundClause {
    subject: heap,
    bounds: heap,
    lifetime_bound: heap,
    span: inline
});
record!(TypeRef {
    kind: heap,
    span: inline
});
record!(DynBinding {
    kind: heap,
    span: inline
});
record!(Aggregate {
    kind: inline,
    name: heap,
    generics: heap,
    fields: heap,
    methods: heap
});
record!(Field {
    name: heap,
    ty: heap,
    default: heap,
    public: inline,
    manages: inline,
    attributes: heap,
    span: inline
});
record!(EnumDecl {
    name: heap,
    generics: heap,
    variants: heap
});
record!(Variant {
    name: heap,
    fields: heap,
    attributes: heap,
    span: inline
});
record!(VariantTupleField {
    ty: heap,
    manages: inline,
    span: inline
});
record!(TraitDecl {
    name: heap,
    generics: heap,
    bounds: heap,
    members: heap
});
record!(ImplBlock {
    generics: heap,
    trait_type: heap,
    target: heap,
    receiver_default: heap,
    where_clause: heap,
    members: heap
});
record!(TypeAlias {
    name: heap,
    generics: heap,
    bounds: heap,
    value: heap
});
record!(ConstDecl {
    name: heap,
    ty: heap,
    value: heap
});
record!(ContractClause {
    name: heap,
    terms: heap,
    span: inline
});
record!(ContractTerm {
    path: heap,
    type_args: heap,
    arguments: heap,
    index: heap,
    span: inline
});
record!(Attribute {
    path: heap,
    arguments: heap,
    string_value: heap,
    span: inline
});
record!(Block {
    statements: heap,
    span: inline
});
record!(Stmt {
    kind: heap,
    span: inline,
    semantic: inline
});
record!(MatchArm {
    pattern: heap,
    body: heap,
    span: inline
});
record!(Pattern {
    kind: heap,
    span: inline
});
record!(RecordPatternField {
    name: heap,
    pattern: heap,
    span: inline
});
record!(Expr {
    kind: heap,
    span: inline,
    semantic: inline
});
record!(QuoteHole {
    kind: inline,
    value: heap,
    span: inline
});
record!(QuoteTemplate {
    tree: heap,
    holes: heap,
    name_sites: heap
});
record!(QuoteNameSite {
    span: inline,
    token_index: inline
});
record!(QuoteTokenTree {
    delimiter: inline,
    children: heap,
    span: inline
});
record!(Argument {
    spread: inline,
    value: heap,
    span: inline
});
record!(RecordField {
    name: heap,
    value: heap,
    span: inline
});
record!(ClosureParam {
    name: heap,
    ty: heap,
    span: inline
});
record!(TokenTree {
    delimiter: inline,
    children: heap,
    span: inline
});

impl Retained for s::ItemKind {
    fn retained(&self, cost: &mut Cost<'_, '_>) -> R<()> {
        cost.step()?;
        match self {
            Self::Import(v) => v.retained(cost),
            Self::Function(v) => v.retained(cost),
            Self::Aggregate(v) => v.retained(cost),
            Self::Enum(v) => v.retained(cost),
            Self::Trait(v) => v.retained(cost),
            Self::Impl(v) => v.retained(cost),
            Self::TypeAlias(v) => v.retained(cost),
            Self::Const(v) => v.retained(cost),
            Self::Extern { abi, function } => fields!(cost;abi,function),
            Self::Meta(v) => v.retained(cost),
        }
    }
}
impl Retained for s::FunctionBody {
    fn retained(&self, cost: &mut Cost<'_, '_>) -> R<()> {
        cost.step()?;
        match self {
            Self::Block(v) => v.retained(cost),
            Self::Expression(v) => v.retained(cost),
            Self::Declaration => Ok(()),
        }
    }
}
impl Retained for s::GenericParamKind {
    fn retained(&self, cost: &mut Cost<'_, '_>) -> R<()> {
        cost.step()?;
        match self {
            Self::Type(v) => v.retained(cost),
            Self::Const(v) => v.retained(cost),
            Self::Lifetime => Ok(()),
        }
    }
}
impl Retained for s::BoundSubject {
    fn retained(&self, cost: &mut Cost<'_, '_>) -> R<()> {
        cost.step()?;
        match self {
            Self::Type(v) => v.retained(cost),
            Self::Lifetime(v) => v.retained(cost),
        }
    }
}
impl Retained for s::GenericArg {
    fn retained(&self, cost: &mut Cost<'_, '_>) -> R<()> {
        cost.step()?;
        match self {
            Self::Type(v) => v.retained(cost),
            Self::Lifetime(v) => v.retained(cost),
            Self::Const(v) => v.retained(cost),
            Self::AssociatedBinding(v) => v.retained(cost),
        }
    }
}
impl Retained for s::TypeKind {
    fn retained(&self, cost: &mut Cost<'_, '_>) -> R<()> {
        cost.step()?;
        match self {
            Self::Named { path, args } => fields!(cost;path,args),
            Self::Reference {
                lifetime,
                mutable: _,
                inner,
            } => fields!(cost;lifetime,inner),
            Self::Tuple(v) | Self::Union(v) | Self::Impl(v) => v.retained(cost),
            Self::Group(v) | Self::Optional(v) => v.retained(cost),
            Self::Array { element, length } => fields!(cost;element,length),
            Self::Function { params, result } => fields!(cost;params,result),
            Self::ForeignFunction {
                abi,
                params,
                result,
            } => fields!(cost;abi,params,result),
            Self::Error { error, result } => fields!(cost;error,result),
            Self::Dyn {
                path,
                args,
                bindings,
            } => fields!(cost;path,args,bindings),
            Self::ForLifetime { lifetime, inner } => fields!(cost;lifetime,inner),
            Self::MetaEval(v) | Self::MetaSplice(v) => v.retained(cost),
            Self::MetaBlock(v) => v.retained(cost),
        }
    }
}
impl Retained for s::DynBindingKind {
    fn retained(&self, cost: &mut Cost<'_, '_>) -> R<()> {
        cost.step()?;
        match self {
            Self::Type { member, value } => fields!(cost;member,value),
            Self::Constant { member, ty, value } => fields!(cost;member,ty,value),
            Self::Method { member, arguments } => fields!(cost;member,arguments),
        }
    }
}
impl Retained for s::VariantFields {
    fn retained(&self, cost: &mut Cost<'_, '_>) -> R<()> {
        cost.step()?;
        match self {
            Self::Unit => Ok(()),
            Self::Tuple(v) => v.retained(cost),
            Self::Record(v) => v.retained(cost),
        }
    }
}
impl Retained for s::StmtKind {
    fn retained(&self, cost: &mut Cost<'_, '_>) -> R<()> {
        cost.step()?;
        match self {
            Self::Binding {
                mutable: _,
                pattern,
                ty,
                value,
            } => fields!(cost;pattern,ty,value),
            Self::Return(v) => v.retained(cost),
            Self::Yield(v) | Self::Expression(v) => v.retained(cost),
            Self::Break | Self::Continue => Ok(()),
            Self::Defer(v) | Self::Loop(v) | Self::Unsafe(v) => v.retained(cost),
            Self::If {
                condition,
                then_body,
                else_body,
            } => fields!(cost;condition,then_body,else_body),
            Self::While { condition, body } => fields!(cost;condition,body),
            Self::For {
                pattern,
                iterable,
                body,
            } => fields!(cost;pattern,iterable,body),
            Self::Match { value, arms } => fields!(cost;value,arms),
            Self::With { name, value, body } => fields!(cost;name,value,body),
            Self::Assign {
                target,
                op: _,
                value,
            } => fields!(cost;target,value),
            Self::Meta(v) => v.retained(cost),
        }
    }
}
impl Retained for s::Condition {
    fn retained(&self, cost: &mut Cost<'_, '_>) -> R<()> {
        cost.step()?;
        match self {
            Self::Expression(v) => v.retained(cost),
            Self::Let { pattern, value } => fields!(cost;pattern,value),
        }
    }
}
impl Retained for s::ElseBranch {
    fn retained(&self, cost: &mut Cost<'_, '_>) -> R<()> {
        cost.step()?;
        match self {
            Self::Block(v) => v.retained(cost),
            Self::If(v) => v.retained(cost),
        }
    }
}
impl Retained for s::PatternKind {
    fn retained(&self, cost: &mut Cost<'_, '_>) -> R<()> {
        cost.step()?;
        match self {
            Self::Name(v) => v.retained(cost),
            Self::Literal(v) => v.retained(cost),
            Self::Wildcard => Ok(()),
            Self::Reference {
                mutable: _,
                pattern,
            }
            | Self::ErrorArm {
                is_error: _,
                pattern,
            } => pattern.retained(cost),
            Self::Tuple(v) => v.retained(cost),
            Self::Constructor { path, arguments } => fields!(cost;path,arguments),
            Self::RecordConstructor { path, fields } => fields!(cost;path,fields),
        }
    }
}
impl Retained for s::ExprKind {
    fn retained(&self, cost: &mut Cost<'_, '_>) -> R<()> {
        enum Child<'a> {
            Kind(&'a s::ExprKind),
            Boxed(&'a s::Expr),
            Arguments(&'a [s::Argument]),
            Remaining(&'a [s::Argument]),
        }
        let mut pending = vec![Child::Kind(self)];
        while let Some(child) = pending.pop() {
            match child {
                Child::Boxed(expr) => {
                    cost.bytes(size_of::<s::Expr>())?;
                    cost.step()?; // inline Expr record
                    let s::Expr {
                        kind,
                        semantic: _,
                        span: _,
                    } = expr;
                    pending.push(Child::Kind(kind));
                }
                Child::Arguments(args) => {
                    let bytes = args
                        .len()
                        .checked_mul(size_of::<s::Argument>())
                        .ok_or_else(|| bad(cost.span, "original AST vector storage overflow"))?;
                    cost.bytes(bytes)?;
                    pending.push(Child::Remaining(args));
                }
                Child::Remaining(args) => {
                    if let Some((arg, rest)) = args.split_first() {
                        cost.step()?; // Argument
                        cost.step()?; // inline Expr
                        pending.push(Child::Remaining(rest));
                        let s::Argument {
                            spread: _,
                            value,
                            span: _,
                        } = arg;
                        let s::Expr {
                            kind,
                            semantic: _,
                            span: _,
                        } = value;
                        pending.push(Child::Kind(kind));
                    }
                }
                Child::Kind(kind) => {
                    cost.step()?;
                    match kind {
                        Self::Group(inner)
                        | Self::Try(inner)
                        | Self::MetaEval(inner)
                        | Self::MetaSplice(inner)
                        | Self::Unary { operand: inner, .. } => {
                            pending.push(Child::Boxed(inner));
                            continue;
                        }
                        Self::Binary { left, right, .. }
                        | Self::Pipeline {
                            input: left,
                            target: right,
                        }
                        | Self::Index {
                            object: left,
                            index: right,
                        } => {
                            pending.push(Child::Boxed(right));
                            pending.push(Child::Boxed(left));
                            continue;
                        }
                        Self::Call { callee, arguments } => {
                            pending.push(Child::Arguments(arguments));
                            pending.push(Child::Boxed(callee));
                            continue;
                        }
                        Self::Literal(v) => v.retained(cost),
                        Self::Name(v) | Self::ReceiverMember(v) => v.retained(cost),
                        Self::Ascribed { ty, value } => fields!(cost;ty,value),
                        Self::Tuple(v) | Self::Array(v) => v.retained(cost),
                        Self::Member { object, member } => fields!(cost;object,member),
                        Self::TypeApply { callee, arguments } => fields!(cost;callee,arguments),
                        Self::Slice { object, start, end } => fields!(cost;object,start,end),
                        Self::Record { ty, base, fields } => fields!(cost;ty,base,fields),
                        Self::Closure {
                            context: _,
                            move_capture: _,
                            params,
                            return_type,
                            body,
                        } => fields!(cost;params,return_type,body),
                        Self::If {
                            condition,
                            then_value,
                            else_value,
                        } => fields!(cost;condition,then_value,else_value),
                        Self::Quote {
                            category: _,
                            template,
                        } => template.retained(cost),
                        Self::MetaBlock(v) => v.retained(cost),
                        Self::SyntaxIsland { path, tree } => fields!(cost;path,tree),
                    }?;
                }
            }
        }
        Ok(())
    }
}
impl Retained for s::Literal {
    fn retained(&self, cost: &mut Cost<'_, '_>) -> R<()> {
        cost.step()?;
        match self {
            Self::Integer(v) | Self::Float(v) | Self::String(v) | Self::Char(v) => v.retained(cost),
            Self::Bool(_) => Ok(()),
        }
    }
}
impl Retained for s::MemberName {
    fn retained(&self, cost: &mut Cost<'_, '_>) -> R<()> {
        cost.step()?;
        match self {
            Self::Name(v) => v.retained(cost),
            Self::Constrained { name, owner } => fields!(cost;name,owner),
            Self::TupleIndex(_) => Ok(()),
        }
    }
}
impl Retained for s::ArrowBody {
    fn retained(&self, cost: &mut Cost<'_, '_>) -> R<()> {
        cost.step()?;
        match self {
            Self::Expression(v) => v.retained(cost),
            Self::Block(v) => v.retained(cost),
        }
    }
}
impl Retained for s::QuoteTokenTreeEntry {
    fn retained(&self, cost: &mut Cost<'_, '_>) -> R<()> {
        cost.step()?;
        match self {
            Self::Token { text, span: _ } => text.retained(cost),
            Self::Tree(v) => v.retained(cost),
            Self::Hole { index: _, span: _ } => Ok(()),
        }
    }
}
impl Retained for s::TokenTreeEntry {
    fn retained(&self, cost: &mut Cost<'_, '_>) -> R<()> {
        cost.step()?;
        match self {
            Self::Token { text, span: _ } => text.retained(cost),
            Self::Tree(v) => v.retained(cost),
        }
    }
}
impl Retained for s::MetaItem {
    fn retained(&self, cost: &mut Cost<'_, '_>) -> R<()> {
        cost.step()?;
        match self {
            Self::If {
                condition,
                then_items,
                else_items,
            } => fields!(cost;condition,then_items,else_items),
            Self::For {
                binding,
                iterable,
                items,
            } => fields!(cost;binding,iterable,items),
            Self::Emit(v) => v.retained(cost),
        }
    }
}
impl Retained for s::MetaStatement {
    fn retained(&self, cost: &mut Cost<'_, '_>) -> R<()> {
        cost.step()?;
        match self {
            Self::If {
                condition,
                then_body,
                else_body,
            } => fields!(cost;condition,then_body,else_body),
            Self::For {
                binding,
                iterable,
                body,
            } => fields!(cost;binding,iterable,body),
            Self::Emit(v) => v.retained(cost),
        }
    }
}

pub(super) fn module(module: &s::Module, budget: &mut Budget<'_>) -> R<()> {
    // Arc's two atomic counters precede its inline Module payload. Include
    // alignment padding, then each clone-owned allocation exactly once.
    let (allocation, _) = std::alloc::Layout::new::<[std::sync::atomic::AtomicUsize; 2]>()
        .extend(std::alloc::Layout::new::<s::Module>())
        .map_err(|_| bad(module.span, "original Module Arc layout overflow"))?;
    let mut cost = Cost {
        budget,
        span: module.span,
    };
    cost.bytes(allocation.pad_to_align().size())?;
    module.retained(&mut cost)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parsed(text: &str) -> s::Module {
        let source = zil_base::SourceUnit::new(
            zil_base::FileId(0),
            zil_base::ModuleId(0),
            "snapshot.zil",
            text,
        );
        zil_syntax::parse(&source).into_result().unwrap()
    }
    fn arc_bytes() -> usize {
        std::alloc::Layout::new::<[std::sync::atomic::AtomicUsize; 2]>()
            .extend(std::alloc::Layout::new::<s::Module>())
            .unwrap()
            .0
            .pad_to_align()
            .size()
    }
    fn check_exact(module: &s::Module, expected: usize) {
        for _ in 0..2 {
            let limits = Limits {
                max_source_bytes: expected,
                ..Limits::default()
            };
            let mut budget = Budget::new(&limits);
            budget
                .metadata_scope(module.span, false, |budget| super::module(module, budget))
                .unwrap();
            assert_eq!(budget.remaining_limits().max_source_bytes, 0);
            let short = Limits {
                max_source_bytes: expected - 1,
                ..limits
            };
            let mut budget = Budget::new(&short);
            let error = budget
                .metadata_scope(module.span, false, |budget| super::module(module, budget))
                .unwrap_err();
            assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
        }
    }
    #[test]
    fn cloned_inline_nodes_are_not_separate_heap_allocations() {
        let mut module = parsed("fn f(){}");
        module.items.reserve(100);
        let s::ItemKind::Function(function) = &mut module.items[0].kind else {
            panic!()
        };
        function.name.reserve(100);
        let expected = arc_bytes() + size_of::<s::Item>() + 1;
        check_exact(&module, expected);
        // Derived Clone copies live lengths, not the parser's spare capacity.
        let cloned = module.clone();
        let s::ItemKind::Function(function) = &cloned.items[0].kind else {
            panic!()
        };
        assert_eq!(
            expected,
            arc_bytes() + cloned.items.capacity() * size_of::<s::Item>() + function.name.capacity()
        );
    }
    #[test]
    fn complete_mixed_type_life_and_constant_subtrees_have_real_storage() {
        let module = parsed("fn f<life a,T,#N:usize>(x:&'a T)->[T;N]{return [x]}");
        let expected = arc_bytes()+size_of::<s::Item>()+1
            +3*size_of::<s::GenericParam>()+3
            +size_of::<String>()+5 // usize const carrier path
            +size_of::<s::Param>()+1 // x
            +1+size_of::<s::TypeRef>()+size_of::<String>()+1 // &'a T
            +size_of::<s::TypeRef>()+size_of::<String>()+1 // array T
            +size_of::<s::Expr>()+1 // boxed array length N
            +size_of::<s::Stmt>()+size_of::<s::Expr>()+1; // return [x]
        check_exact(&module, expected);
    }
    #[test]
    fn quote_tokens_holes_and_hygiene_sites_are_all_retained() {
        let mut module = parsed("fn f(){}");
        let span = module.span;
        let s::ItemKind::Function(function) = &mut module.items[0].kind else {
            panic!()
        };
        function.body = s::FunctionBody::Expression(s::Expr {
            span,
            semantic: Default::default(),
            kind: s::ExprKind::Quote {
                category: s::SyntaxCategory::Expr,
                template: s::QuoteTemplate {
                    tree: s::QuoteTokenTree {
                        delimiter: s::Delimiter::Brace,
                        span,
                        children: vec![
                            s::QuoteTokenTreeEntry::Token {
                                text: "abc".into(),
                                span,
                            },
                            s::QuoteTokenTreeEntry::Hole { index: 0, span },
                            s::QuoteTokenTreeEntry::Tree(s::QuoteTokenTree {
                                delimiter: s::Delimiter::Parenthesis,
                                children: Vec::new(),
                                span,
                            }),
                        ],
                    },
                    holes: vec![s::QuoteHole {
                        kind: s::QuoteHoleKind::Expr,
                        span,
                        value: Box::new(s::Expr {
                            kind: s::ExprKind::Name("captured".into()),
                            span,
                            semantic: Default::default(),
                        }),
                    }],
                    name_sites: vec![s::QuoteNameSite {
                        span,
                        token_index: 0,
                    }],
                },
            },
        });
        let expected = arc_bytes()
            + size_of::<s::Item>()
            + 1
            + 3 * size_of::<s::QuoteTokenTreeEntry>()
            + 3
            + size_of::<s::QuoteHole>()
            + size_of::<s::Expr>()
            + 8
            + size_of::<s::QuoteNameSite>();
        check_exact(&module, expected);
    }
}
