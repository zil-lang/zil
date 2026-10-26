//! Fixed source shapes for inference only. Types and Verify independently
//! admit and check every concrete operation and ownership transition.
use super::*;

pub(super) fn signature(name: &str, span: Span) -> Option<(Vec<TypeRef>, TypeRef)> {
    let buffer = || named("ByteBuffer", span);
    let (parameters, result) = match name {
        "bytes_new" => (vec![], buffer()),
        "bytes_reserve" => (vec![buffer(), named("i64", span)], named("bool", span)),
        "bytes_push" => (vec![buffer(), named("u8", span)], named("bool", span)),
        "bytes_len" | "bytes_capacity" => (vec![buffer()], named("i64", span)),
        "bytes_view" | "bytes_view_mut" => (
            vec![buffer()],
            TypeRef {
                span,
                kind: TypeKind::Reference {
                    mutable: name == "bytes_view_mut",
                    lifetime: None,
                    inner: Box::new(TypeRef {
                        span,
                        kind: TypeKind::Array {
                            element: Box::new(named("u8", span)),
                            length: None,
                        },
                    }),
                },
            },
        ),
        _ => return None,
    };
    Some((parameters, result))
}
