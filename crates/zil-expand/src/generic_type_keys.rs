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
// Match array_length's underscore removal/decimal parser without allocating
// its temporary String during this layout pass. The real render is prepaid.
fn array_value(length: &Expr) -> R<u32> {
    let ExprKind::Literal(Literal::Integer(text)) = &length.kind else {
        return Err(unsupported(
            length.span,
            "nonliteral array length needs CT type evaluation",
        ));
    };
    let bad = || {
        unsupported(
            length.span,
            "array length must be a bounded integer constant",
        )
    };
    let mut bytes = text.bytes().filter(|byte| *byte != b'_').peekable();
    if bytes.peek() == Some(&b'+') {
        bytes.next();
    }
    let mut count = 0usize;
    let mut value = 0u32;
    for byte in bytes {
        if !byte.is_ascii_digit() {
            return Err(bad());
        }
        value = value
            .checked_mul(10)
            .and_then(|n| n.checked_add(u32::from(byte - b'0')))
            .ok_or_else(bad)?;
        count += 1;
    }
    if count == 0 {
        return Err(bad());
    }
    Ok(value)
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
        TypeKind::Named { path, args } => {
            budget.identity_work(path.len().saturating_add(args.len()), ty.span)?;
            let mut name = path.len().saturating_sub(1);
            for part in path {
                name = add(name, part.len(), ty.span)?;
            }
            for arg in args {
                let GenericArg::Type(child) = arg else {
                    return Err(unsupported(ty.span, "const generic argument"));
                };
                result.child(shape(child, budget, depth + 1)?, ty.span)?;
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
                if let ExprKind::Literal(Literal::Integer(text)) = &length.kind {
                    budget.identity_work(mul(text.len(), 4, ty.span)?, ty.span)?;
                    result.work = add(result.work, mul(text.len(), 4, ty.span)?, ty.span)?;
                }
                (add(3, digits(array_value(length)? as usize), ty.span)?, 1)
            } else {
                (3, 1)
            }
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
