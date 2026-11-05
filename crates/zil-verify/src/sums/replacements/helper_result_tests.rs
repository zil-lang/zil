//! Private result-shape/cost components; no Source or public Verify claim.
use super::*;

#[test]
fn finite_helper_result_recurses_without_borrowed_owned_or_ffi_escape() {
    let defaults = Limits::default();
    let nested = |payload| Type::ErrorUnion {
        error: Box::new(Type::Enum {
            name: "Fault".into(),
            variants: vec![("Bad".into(), vec![])],
        }),
        result: Box::new(Type::Record {
            name: "Reply".into(),
            fields: vec![("data".into(), Type::Tuple(vec![Type::Unit, payload]))],
        }),
    };
    assert!(
        helper_copy_result(
            &nested(Type::Array {
                element: Box::new(Type::I32),
                length: 2
            }),
            &mut Budget::new(&defaults)
        )
        .unwrap()
    );
    for payload in [
        Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        },
        Type::Desc(Box::new(Type::Record {
            name: "R".into(),
            fields: vec![],
        })),
        Type::Managed(Box::new(Type::Record {
            name: "R".into(),
            fields: vec![],
        })),
        Type::ForeignHandle,
        Type::CWideString,
        Type::FunctionPointer {
            parameters: vec![Type::I32],
            result: Box::new(Type::I32),
        },
    ] {
        assert!(!helper_copy_result(&nested(payload), &mut Budget::new(&defaults)).unwrap());
    }
    let empty = Limits {
        max_analysis_steps: 0,
        ..defaults
    };
    assert!(helper_copy_result(&Type::Unit, &mut Budget::new(&empty)).unwrap());
    assert_eq!(
        helper_copy_result(&Type::I32, &mut Budget::new(&empty))
            .unwrap_err()
            .category,
        zil_base::ResultCategory::ResourceLimit
    );
}
