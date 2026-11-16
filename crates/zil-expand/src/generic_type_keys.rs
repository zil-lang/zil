//! Prepay complete generic keys before rendering or sorting any full strings.
//! Nominal identity stays in the key; the estimate never establishes a type fact.
use super::*;

fn exhausted(span: Span) -> Diagnostic {
    Diagnostic::limit(
        Phase::Expand,
        "structural type key size/work overflow",
        Some(span),
    )
}
fn add(a: usize, b: usize, span: Span) -> R<usize> {
    a.checked_add(b).ok_or_else(|| exhausted(span))
}
fn mul(a: usize, b: usize, span: Span) -> R<usize> {
    a.checked_mul(b).ok_or_else(|| exhausted(span))
}
fn digits(n: usize) -> usize {
    if n == 0 { 1 } else { n.ilog10() as usize + 1 }
}
// Parse the scalar spelling without allocating the renderer's underscore
// removal buffer. This pass only measures identity; const validation remains
// with generic_const and the verified CT path.
fn integer_value(text: &str, span: Span) -> R<(u128, Option<&str>)> {
    let (body, suffix) = [
        "u16", "u8", "u32", "i8", "i16", "i32", "i64", "isize", "u64", "usize",
    ]
    .into_iter()
    .find_map(|suffix| text.strip_suffix(suffix).map(|body| (body, Some(suffix))))
    .unwrap_or((text, None));
    let mut bytes = body.bytes().filter(|b| *b != b'_').peekable();
    let mut radix = 10u128;
    let mut count = 0usize;
    if bytes.peek() == Some(&b'0') {
        bytes.next();
        count = 1;
        if let Some(prefix @ (b'x' | b'b' | b'o')) = bytes.peek().copied() {
            bytes.next();
            radix = match prefix {
                b'x' => 16,
                b'b' => 2,
                _ => 8,
            };
            count = 0;
        }
    }
    // from_str_radix accepts a leading plus in the remaining digit sequence.
    if count == 0 && bytes.peek() == Some(&b'+') {
        bytes.next();
    }
    let mut value = 0u128;
    for byte in bytes {
        let digit = match byte {
            b'0'..=b'9' => u128::from(byte - b'0'),
            b'a'..=b'f' => u128::from(byte - b'a' + 10),
            b'A'..=b'F' => u128::from(byte - b'A' + 10),
            _ => {
                return Err(Diagnostic::error(
                    Phase::Expand,
                    "E-GENERIC-ARGUMENT",
                    Some(span),
                    "invalid const integer",
                ));
            }
        };
        if digit >= radix {
            return Err(Diagnostic::error(
                Phase::Expand,
                "E-GENERIC-ARGUMENT",
                Some(span),
                "invalid const integer",
            ));
        }
        value = value
            .checked_mul(radix)
            .and_then(|n| n.checked_add(digit))
            .ok_or_else(|| {
                Diagnostic::error(
                    Phase::Expand,
                    "E-GENERIC-ARGUMENT",
                    Some(span),
                    "invalid const integer",
                )
            })?;
        count += 1;
    }
    if count == 0 {
        return Err(Diagnostic::error(
            Phase::Expand,
            "E-GENERIC-ARGUMENT",
            Some(span),
            "invalid const integer",
        ));
    }
    Ok((value, suffix))
}
fn scalar_shape(expression: &Expr, typed: bool, budget: &mut Budget<'_>, depth: usize) -> R<Shape> {
    let span = expression.span;
    match &expression.kind {
        ExprKind::Literal(Literal::Integer(text)) => {
            let work = mul(text.len(), 4, span)?;
            budget.identity_work(work, span)?;
            let (value, domain) = integer_value(text, span)?;
            let decimal = if value == 0 {
                1
            } else {
                value.ilog10() as usize + 1
            };
            let bytes = if typed {
                add(
                    decimal,
                    add(domain.unwrap_or("integer").len(), 1, span)?,
                    span,
                )?
            } else {
                decimal
            };
            Ok(Shape { bytes, work })
        }
        ExprKind::Literal(Literal::Bool(value)) if typed => Ok(Shape {
            bytes: if *value { 9 } else { 10 },
            work: 0,
        }),
        ExprKind::Name(name) => Ok(Shape {
            bytes: add(name.len(), add(digits(name.len()), 8, span)?, span)?,
            work: 0,
        }),
        ExprKind::Group(inner) => {
            budget.identity_node(depth, span)?;
            scalar_shape(inner, typed, budget, depth + 1)
        }
        _ => Err(unsupported(
            span,
            "const expression requires verified CT evaluation",
        )),
    }
}
#[derive(Default)]
struct Shape {
    bytes: usize,
    work: usize,
}
impl Shape {
    fn child(&mut self, child: Shape, span: Span) -> R<()> {
        self.bytes = add(self.bytes, child.bytes, span)?;
        self.work = add(self.work, child.work, span)?;
        Ok(())
    }
    fn finish(mut self, overhead: usize, slots: usize, span: Span) -> R<Self> {
        self.bytes = add(self.bytes, overhead, span)?;
        // Covers formatting, copies into parent strings, amortized vector/string
        // growth and temporary String headers before the renderer allocates.
        self.work = add(
            self.work,
            add(mul(self.bytes, 8, span)?, mul(slots, 64, span)?, span)?,
            span,
        )?;
        Ok(self)
    }
}
#[derive(Default)]
struct Sum {
    count: usize,
    bytes: usize,
    wrapped: usize,
    work: usize,
}
fn sum_member(ty: &TypeRef, budget: &mut Budget<'_>, depth: usize, sum: &mut Sum) -> R<()> {
    match &ty.kind {
        TypeKind::Group(inner) => {
            budget.identity_node(depth, ty.span)?;
            sum_member(inner, budget, depth + 1, sum)
        }
        TypeKind::Union(parts) => {
            budget.identity_node(depth, ty.span)?;
            for member in parts {
                sum_member(member, budget, depth + 1, sum)?;
            }
            Ok(())
        }
        _ => {
            let child = shape(ty, budget, depth)?;
            sum.count = add(sum.count, 1, ty.span)?;
            sum.bytes = add(sum.bytes, child.bytes, ty.span)?;
            sum.wrapped = add(
                sum.wrapped,
                add(child.bytes, add(digits(child.bytes), 1, ty.span)?, ty.span)?,
                ty.span,
            )?;
            sum.work = add(sum.work, child.work, ty.span)?;
            Ok(())
        }
    }
}
fn shape(ty: &TypeRef, budget: &mut Budget<'_>, depth: usize) -> R<Shape> {
    budget.identity_node(depth, ty.span)?;
    let mut result = Shape::default();
    let (overhead, slots) = match &ty.kind {
        TypeKind::Named { path, args } if path.as_slice() == ["void"] && args.is_empty() => (2, 0),
        TypeKind::Named { path, args }
        | TypeKind::Dyn {
            path,
            args,
            bindings: _,
        } => {
            if matches!(&ty.kind, TypeKind::Dyn { bindings, .. } if !bindings.is_empty()) {
                return Err(unsupported(
                    ty.span,
                    "dyn binding needs its canonical identity consumer",
                ));
            }
            budget.identity_work(path.len().saturating_add(args.len()), ty.span)?;
            let mut name = path.len().saturating_sub(1);
            for part in path {
                name = add(name, part.len(), ty.span)?;
            }
            for arg in args {
                let child = match arg {
                    GenericArg::AssociatedBinding(binding) => {
                        return Err(error(
                            binding.span,
                            "associated equations require the original trait application obligation consumer",
                        ));
                    }
                    GenericArg::Lifetime(_) => {
                        return Err(unsupported(
                            ty.span,
                            "lifetime instance identity requires scoped canonical storage relation",
                        ));
                    }
                    GenericArg::Type(child) => shape(child, budget, depth + 1)?,
                    GenericArg::Const(expression) => {
                        scalar_shape(expression, true, budget, depth + 1)?.finish(1, 0, ty.span)?
                    }
                };
                result.child(child, ty.span)?;
            }
            (
                add(
                    add(name, add(digits(name), 2, ty.span)?, ty.span)?,
                    mul(args.len(), 2, ty.span)?,
                    ty.span,
                )?,
                args.len(),
            )
        }
        TypeKind::Group(inner) => return shape(inner, budget, depth + 1),
        TypeKind::ForLifetime { .. } => {
            let mut inner = ty;
            let mut binders = 0usize;
            let mut names = 0usize;
            while let TypeKind::ForLifetime {
                lifetime,
                inner: next,
            } = &inner.kind
            {
                budget.identity_node(add(depth, binders, ty.span)?, inner.span)?;
                budget.identity_work(lifetime.len(), inner.span)?;
                binders = add(binders, 1, ty.span)?;
                names = add(names, lifetime.len(), ty.span)?;
                inner = next;
            }
            let TypeKind::Function { params, .. } = &inner.kind else {
                return Err(unsupported(
                    ty.span,
                    "borrowed code key requires a function signature",
                ));
            };
            let slots = add(params.len(), 1, ty.span)?;
            // Long common binder names must be charged before lexical lookup;
            // labels and remap vectors need at most 24 bytes per relation slot.
            budget.identity_work(mul(add(names, binders, ty.span)?, slots, ty.span)?, ty.span)?;
            let mut body = shape(inner, budget, add(depth, binders, ty.span)?)?;
            let extra = add(
                32,
                mul(add(binders, slots, ty.span)?, 24, ty.span)?,
                ty.span,
            )?;
            body.bytes = add(body.bytes, extra, ty.span)?;
            body.work = add(body.work, mul(extra, 4, ty.span)?, ty.span)?;
            return Ok(body);
        }

        TypeKind::Reference { mutable, inner, .. } => {
            result.child(shape(inner, budget, depth + 1)?, ty.span)?;
            (if *mutable { 7 } else { 8 }, 1)
        }
        TypeKind::Union(parts) => {
            let mut sum = Sum::default();
            for member in parts {
                sum_member(member, budget, depth + 1, &mut sum)?;
            }
            let bytes = add(sum.wrapped, add(digits(sum.count), 2, ty.span)?, ty.span)?;
            // Every possible full-key comparison is bounded, including long
            // common prefixes. Nested sums inside other constructors contribute
            // their own comparison work through child Shape values.
            let comparisons = mul(sum.bytes, add(sum.count, 1, ty.span)?, ty.span)?;
            let work = add(
                sum.work,
                add(comparisons, mul(sum.wrapped, 4, ty.span)?, ty.span)?,
                ty.span,
            )?;
            return Shape { bytes, work }.finish(0, sum.count, ty.span);
        }
        TypeKind::Tuple(fields) => {
            for field in fields {
                result.child(shape(field, budget, depth + 1)?, ty.span)?;
            }
            (
                add(2, fields.len().saturating_sub(1), ty.span)?,
                fields.len(),
            )
        }
        TypeKind::Array { element, length } => {
            result.child(shape(element, budget, depth + 1)?, ty.span)?;
            if let Some(length) = length {
                let scalar = scalar_shape(length, false, budget, depth + 1)?;
                result.work = add(result.work, scalar.work, ty.span)?;
                (add(3, scalar.bytes, ty.span)?, 1)
            } else {
                (3, 1)
            }
        }
        TypeKind::MetaEval(expression) => {
            return scalar_shape(expression, true, budget, depth + 1)?.finish(1, 0, ty.span);
        }
        TypeKind::Optional(inner) => {
            result.child(shape(inner, budget, depth + 1)?, ty.span)?;
            (1, 1)
        }
        TypeKind::Error {
            error,
            result: success,
        } => {
            result.child(shape(error, budget, depth + 1)?, ty.span)?;
            result.child(shape(success, budget, depth + 1)?, ty.span)?;
            (5, 2)
        }
        TypeKind::Function {
            params,
            result: success,
        }
        | TypeKind::ForeignFunction {
            params,
            result: success,
            ..
        } => {
            for parameter in params {
                result.child(shape(parameter, budget, depth + 1)?, ty.span)?;
            }
            result.child(shape(success, budget, depth + 1)?, ty.span)?;
            let prefix = if let TypeKind::ForeignFunction { abi, .. } = &ty.kind {
                add(add(6, digits(abi.len()), ty.span)?, abi.len(), ty.span)?
            } else {
                5
            };
            (
                add(prefix, params.len().saturating_sub(1), ty.span)?,
                add(params.len(), 1, ty.span)?,
            )
        }
        _ => {
            return Err(unsupported(
                ty.span,
                "this generic type argument needs additional type resolution",
            ));
        }
    };
    result.finish(overhead, slots, ty.span)
}
pub(super) fn prepay(ty: &TypeRef, budget: &mut Budget<'_>) -> R<()> {
    let planned = shape(ty, budget, 0)?;
    budget.identity_work(planned.work, ty.span)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
    #[test]
    fn scalar_keys_composed_with_anonymous_members_are_prepaid() {
        for source in [
            "type Key=Phantom<18446744073709551615u64, true, bool|i32>|[i32;0x10];",
            "type Key=Phantom<0b1_0u16, false>|[bool|i32;N];",
        ] {
            let input = SourceUnit::new(FileId(0), ModuleId(0), "keys.zil", source);
            let syntax = zil_syntax::parse(&input).into_result().unwrap();
            let ItemKind::TypeAlias(alias) = &syntax.items[0].kind else {
                unreachable!()
            };
            let ty = alias.value.as_ref().unwrap();
            let limits = Limits::default();
            let mut sizing = Budget::new(&limits);
            sizing.ty(ty).unwrap();
            let layout = shape(ty, &mut sizing, 0).unwrap();
            let rendered = render_type_key(ty).unwrap();
            assert_eq!(layout.bytes, rendered.len(), "{source}");
            let mut first = Budget::new(&limits);
            first.ty(ty).unwrap();
            assert_eq!(type_key_bounded(ty, &mut first).unwrap(), rendered);
            let exact = limits.max_analysis_steps - first.remaining_limits().max_analysis_steps;
            for (amount, good) in [(exact, true), (exact - 1, false), (exact, true)] {
                let limits = Limits {
                    max_analysis_steps: amount,
                    ..limits.clone()
                };
                let mut budget = Budget::new(&limits);
                budget.ty(ty).unwrap();
                let result = type_key_bounded(ty, &mut budget);
                assert_eq!(result.is_ok(), good, "{source}");
                if let Err(error) = result {
                    assert_eq!(error.category, ResultCategory::ResourceLimit);
                }
            }
        }
        // Internal normalized const carriers use the same full typed key.
        let span = Span::new(FileId(0), 0, 1);
        let expression = Expr {
            semantic: Default::default(),
            span,
            kind: ExprKind::Literal(Literal::Integer("0xffu64".into())),
        };
        let ty = TypeRef {
            span,
            kind: TypeKind::MetaEval(Box::new(expression)),
        };
        let limits = Limits::default();
        assert_eq!(
            shape(&ty, &mut Budget::new(&limits), 0).unwrap().bytes,
            render_type_key(&ty).unwrap().len()
        );
    }
    #[test]
    fn nested_complete_key_layout_and_exact_work_boundary() {
        let input = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "keys.zil",
            "type Key=(Phantom<i32|bool>,((bool|u64)|i32),[bool|u64;1_2])|(fn(i32|bool)->(u64|i32)); ",
        );
        let syntax = zil_syntax::parse(&input).into_result().unwrap();
        let ItemKind::TypeAlias(alias) = &syntax.items[0].kind else {
            unreachable!()
        };
        let ty = alias.value.as_ref().unwrap();
        let limits = Limits::default();
        let mut sizing = Budget::new(&limits);
        sizing.ty(ty).unwrap();
        let layout = shape(ty, &mut sizing, 0).unwrap();
        let rendered = render_type_key(ty).unwrap();
        assert_eq!(layout.bytes, rendered.len());
        let mut first = Budget::new(&limits);
        first.ty(ty).unwrap();
        assert_eq!(type_key_bounded(ty, &mut first).unwrap(), rendered);
        let exact = limits.max_analysis_steps - first.remaining_limits().max_analysis_steps;
        for (amount, good) in [(exact, true), (exact - 1, false), (exact, true)] {
            let limits = Limits {
                max_analysis_steps: amount,
                ..limits.clone()
            };
            let mut budget = Budget::new(&limits);
            budget.ty(ty).unwrap();
            let result = type_key_bounded(ty, &mut budget);
            assert_eq!(result.is_ok(), good);
            if let Err(error) = result {
                assert_eq!(error.category, ResultCategory::ResourceLimit);
                assert!(error.message.contains("structural type identity work"));
            }
        }
        println!("344 complete nested key exact work N={exact}; pre-render N-1 and fresh checked");
    }
}

// A single reserved output replaces the per-child temporary strings of the
// recursive renderer. Sorted sums and quantified keys retain their canonical
// renderer and its complete comparison/remapping prepayment.
pub(super) fn streamed(ty: &TypeRef, budget: &mut Budget<'_>) -> R<Option<String>> {
    if !streamable(ty, budget, 0)? {
        return Ok(None);
    }
    let planned = shape(ty, budget, 0)?;
    let work = add(
        mul(planned.bytes, 2, ty.span)?,
        std::mem::size_of::<String>(),
        ty.span,
    )?;
    budget.identity_work(work, ty.span)?;
    let mut text = String::new();
    text.try_reserve_exact(planned.bytes)
        .map_err(|_| exhausted(ty.span))?;
    let mut out = KeyOutput {
        text,
        limit: planned.bytes,
        span: ty.span,
    };
    stream_type(ty, &mut out)?;
    Ok(Some(out.text))
}
fn streamable(ty: &TypeRef, budget: &mut Budget<'_>, depth: usize) -> R<bool> {
    budget.identity_node(depth, ty.span)?;
    Ok(match &ty.kind {
        TypeKind::Named { args, .. } | TypeKind::Dyn { args, .. } => {
            if matches!(&ty.kind, TypeKind::Dyn { bindings, .. } if !bindings.is_empty()) {
                return Ok(false);
            }
            for argument in args {
                match argument {
                    GenericArg::Type(ty) if !streamable(ty, budget, depth + 1)? => {
                        return Ok(false);
                    }
                    GenericArg::Type(_) | GenericArg::Const(_) => {}
                    _ => return Ok(false),
                }
            }
            true
        }
        TypeKind::Group(inner)
        | TypeKind::Reference { inner, .. }
        | TypeKind::Optional(inner)
        | TypeKind::Array { element: inner, .. } => streamable(inner, budget, depth + 1)?,
        TypeKind::Tuple(fields) => {
            for ty in fields {
                if !streamable(ty, budget, depth + 1)? {
                    return Ok(false);
                }
            }
            true
        }
        TypeKind::Error { error, result } => {
            streamable(error, budget, depth + 1)? && streamable(result, budget, depth + 1)?
        }
        TypeKind::Function { params, result }
        | TypeKind::ForeignFunction { params, result, .. } => {
            for ty in params {
                if !streamable(ty, budget, depth + 1)? {
                    return Ok(false);
                }
            }
            streamable(result, budget, depth + 1)?
        }
        TypeKind::MetaEval(_) => true,
        _ => false,
    })
}
struct KeyOutput {
    text: String,
    limit: usize,
    span: Span,
}
impl std::fmt::Write for KeyOutput {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        if value.len() > self.limit.saturating_sub(self.text.len()) {
            return Err(std::fmt::Error);
        }
        self.text.push_str(value);
        Ok(())
    }
}
impl KeyOutput {
    fn put(&mut self, value: &str) -> R<()> {
        std::fmt::Write::write_str(self, value).map_err(|_| exhausted(self.span))
    }
    fn format(&mut self, value: std::fmt::Arguments<'_>) -> R<()> {
        std::fmt::Write::write_fmt(self, value).map_err(|_| exhausted(self.span))
    }
}
fn stream_scalar(value: &Expr, typed: bool, out: &mut KeyOutput) -> R<()> {
    match &value.kind {
        ExprKind::Group(value) => stream_scalar(value, typed, out),
        ExprKind::Literal(Literal::Integer(text)) => {
            let (value, suffix) = integer_value(text, value.span)?;
            if typed {
                out.format(format_args!("{}:", suffix.unwrap_or("integer")))?
            }
            out.format(format_args!("{value}"))
        }
        ExprKind::Literal(Literal::Bool(value)) if typed => {
            out.format(format_args!("bool:{value}"))
        }
        ExprKind::Name(name) => out.format(format_args!("binder:{}:{name}", name.len())),
        _ => Err(unsupported(
            value.span,
            "const expression requires verified CT evaluation",
        )),
    }
}
fn stream_type(ty: &TypeRef, out: &mut KeyOutput) -> R<()> {
    match &ty.kind {
        TypeKind::Group(inner) => stream_type(inner, out),
        TypeKind::Named { path, args } if path.as_slice() == ["void"] && args.is_empty() => {
            out.put("()")
        }
        TypeKind::Named { path, args } | TypeKind::Dyn { path, args, .. } => {
            let mut length = path.len().saturating_sub(1);
            for part in path {
                length = add(length, part.len(), ty.span)?
            }
            let tag = if matches!(ty.kind, TypeKind::Dyn { .. }) {
                'D'
            } else {
                'N'
            };
            out.format(format_args!("{tag}{length}:"))?;
            for (i, part) in path.iter().enumerate() {
                if i != 0 {
                    out.put(".")?
                }
                out.put(part)?
            }
            for argument in args {
                out.put("<")?;
                match argument {
                    GenericArg::Type(ty) => stream_type(ty, out)?,
                    GenericArg::Const(value) => {
                        out.put("#")?;
                        stream_scalar(value, true, out)?
                    }
                    _ => {
                        return Err(error(
                            ty.span,
                            "streamed key lost its prepaid argument kind",
                        ));
                    }
                }
                out.put(">")?;
            }
            Ok(())
        }
        TypeKind::Reference { mutable, inner, .. } => {
            out.format(format_args!("R{mutable}<"))?;
            stream_type(inner, out)?;
            out.put(">")
        }
        TypeKind::Tuple(fields) => {
            out.put("(")?;
            stream_fields(fields, out)?;
            out.put(")")
        }
        TypeKind::Array { element, length } => {
            if let Some(length) = length {
                out.put("A")?;
                stream_scalar(length, false, out)?;
                out.put("<")?
            } else {
                out.put("S<")?
            }
            stream_type(element, out)?;
            out.put(">")
        }
        TypeKind::MetaEval(value) => {
            out.put("#")?;
            stream_scalar(value, true, out)
        }
        TypeKind::Optional(inner) => {
            out.put("?")?;
            stream_type(inner, out)
        }
        TypeKind::Error { error, result } => {
            out.put("E<")?;
            stream_type(error, out)?;
            out.put("><")?;
            stream_type(result, out)?;
            out.put(">")
        }
        TypeKind::Function { params, result }
        | TypeKind::ForeignFunction { params, result, .. } => {
            if let TypeKind::ForeignFunction { abi, .. } = &ty.kind {
                out.format(format_args!("C{}:{abi}(", abi.len()))?
            } else {
                out.put("F(")?
            }
            stream_fields(params, out)?;
            out.put(")->")?;
            stream_type(result, out)
        }
        _ => Err(error(
            ty.span,
            "streamed key lost its prepaid type constructor",
        )),
    }
}
fn stream_fields(fields: &[TypeRef], out: &mut KeyOutput) -> R<()> {
    for (i, field) in fields.iter().enumerate() {
        if i != 0 {
            out.put(",")?
        }
        stream_type(field, out)?
    }
    Ok(())
}

#[cfg(test)]
mod streamed_tests {
    use super::*;
    use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};

    #[test]
    fn streamed_keys_preserve_complete_identity_and_exact_work_boundary() {
        for text in [
            "Map<ByteBuffer,(u64,[i32;0x10])>",
            "&mut [i64]",
            "extern \"C\" fn(i32,bool)->i64",
            "Phantom<18446744073709551615u64,true,usize>",
            "Widths<1u8,2u16,3u32,4u64,5usize,6i8,7i16,8i32,9i64,10isize>",
            "(void,fn(i32)->bool)",
            "i32|bool",
            "for<life a>fn(&'a i32)->&'a i32",
        ] {
            let source = SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "type-key.zil",
                format!("type Key={text};"),
            );
            let syntax = zil_syntax::parse(&source).into_result().unwrap();
            let ItemKind::TypeAlias(alias) = &syntax.items[0].kind else {
                unreachable!()
            };
            let ty = alias.value.as_ref().unwrap();
            let expected = render_type_key(ty).unwrap();
            let limits = Limits::default();
            let mut budget = Budget::new(&limits);
            let Some(actual) = streamed(ty, &mut budget).unwrap() else {
                assert!(text.contains('|') || text.starts_with("for<"));
                continue;
            };
            assert_eq!(actual, expected, "{text}");
            let paid = limits.max_analysis_steps - budget.remaining_limits().max_analysis_steps;
            let exact = Limits {
                max_analysis_steps: paid,
                ..limits.clone()
            };
            assert_eq!(
                streamed(ty, &mut Budget::new(&exact)).unwrap(),
                Some(expected)
            );
            let low = Limits {
                max_analysis_steps: paid - 1,
                ..limits
            };
            assert_eq!(
                streamed(ty, &mut Budget::new(&low)).unwrap_err().category,
                ResultCategory::ResourceLimit
            );
        }
    }
}
