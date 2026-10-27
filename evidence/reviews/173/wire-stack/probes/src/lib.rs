#![cfg(test)]
use zil_base::{Fingerprint, Limits, NodeId, ResultCategory, SCHEMA_VERSION, TargetProfile};
use zil_ir::{ClosureId, Constant, CtInstruction, CtInstructionKind, CtModule, Type, ValueId, wire};

// An independent schema-20 byte oracle derived from the published legacy tag
// table before the traversal rewrite. It is intentionally not used by either
// production codec, and shallow fixtures keep this reference recursion bounded.
fn old_type_bytes(ty: &Type, out: &mut Vec<u8>) -> usize {
    fn number(out: &mut Vec<u8>, n: u32) { out.extend_from_slice(&n.to_le_bytes()); }
    fn text(out: &mut Vec<u8>, value: &str) {
        number(out, value.len() as u32);
        out.extend_from_slice(value.as_bytes());
    }
    let mut cost = 1;
    match ty {
        Type::Unit => out.push(0),
        Type::Bool => out.push(1),
        Type::I32 => out.push(2),
        Type::I64 => out.push(3),
        Type::Tuple(fields) => {
            out.push(4); number(out, fields.len() as u32); cost += fields.len();
            for field in fields { cost += old_type_bytes(field, out); }
        }
        Type::Named(name) => { out.push(5); text(out, name); }
        Type::Address { mutable, inner } => {
            out.extend_from_slice(&[6, u8::from(*mutable)]);
            cost += old_type_bytes(inner, out);
        }
        Type::Managed(inner) | Type::Desc(inner) => {
            out.push(if matches!(ty, Type::Managed(_)) {7} else {8});
            cost += old_type_bytes(inner, out);
        }
        Type::FunctionPointer { parameters, result } | Type::ForeignCallback { parameters, result } => {
            out.push(if matches!(ty, Type::FunctionPointer {..}) {9} else {22});
            number(out, parameters.len() as u32); cost += parameters.len();
            for parameter in parameters { cost += old_type_bytes(parameter, out); }
            cost += old_type_bytes(result, out);
        }
        Type::Closure(id) => { out.push(10); number(out, id.0); }
        Type::Opaque { identity, hidden } => {
            out.push(11); number(out, identity.0); cost += old_type_bytes(hidden, out);
        }
        Type::Unsupported(name) => { out.push(12); text(out, name); }
        Type::Record { name, fields } | Type::Package { name, fields } => {
            out.push(if matches!(ty, Type::Record {..}) {13} else {17});
            text(out, name); number(out, fields.len() as u32); cost += fields.len();
            for (name, field) in fields { text(out, name); cost += old_type_bytes(field, out); }
        }
        Type::Array { element, length } => {
            out.push(14); cost += old_type_bytes(element, out); number(out, *length);
        }
        Type::Enum { name, variants } => {
            out.push(15); text(out, name); number(out, variants.len() as u32); cost += variants.len();
            for (name, fields) in variants {
                text(out, name); number(out, fields.len() as u32); cost += fields.len();
                for field in fields { cost += old_type_bytes(field, out); }
            }
        }
        Type::ErrorUnion { error, result } => {
            out.push(16); cost += old_type_bytes(error, out); cost += old_type_bytes(result, out);
        }
        Type::U8 => out.push(18),
        Type::U16 => out.push(19),
        Type::ForeignHandle => out.push(20),
        Type::CWideString => out.push(21),
        Type::Slice { element } | Type::OwnedStore { element } => {
            out.push(if matches!(ty, Type::Slice {..}) {23} else {25});
            cost += old_type_bytes(element, out);
        }
        Type::ByteBuffer => out.push(24),
    }
    cost
}

fn fixture(ty: Type) -> CtModule {
    CtModule {
        schema_version: SCHEMA_VERSION,
        input_fingerprint: Fingerprint::of(b"independent-wire-traversal-review"),
        target: TargetProfile::host_llvm(),
        instructions: vec![CtInstruction { result: ValueId(0), ty, kind: CtInstructionKind::Constant(Constant::I32(7)) }],
        result: ValueId(0), max_steps: 1, dependencies: vec![],
    }
}

fn fixtures() -> Vec<Type> {
    let package = Type::Package { name: "@model:Leaf".into(), fields: vec![
        ("owner".into(), Type::Managed(Box::new(Type::Record { name: "@model:R".into(), fields: vec![("n".into(), Type::I64)] }))),
        ("bytes".into(), Type::Managed(Box::new(Type::ByteBuffer))),
        ("padding".into(), Type::Array { element: Box::new(Type::U8), length: 13 }),
    ]};
    let mut types = vec![
        Type::Unit, Type::Bool, Type::I32, Type::I64,
        Type::Tuple(vec![Type::Unit, Type::U16]), Type::Named("类型/α".into()),
        Type::Address { mutable: false, inner: Box::new(Type::I32) },
        Type::Managed(Box::new(Type::ByteBuffer)), Type::Desc(Box::new(Type::ByteBuffer)),
        Type::FunctionPointer { parameters: vec![Type::I32, Type::Array { element: Box::new(Type::U8), length: 3 }], result: Box::new(Type::Bool) },
        Type::Closure(ClosureId(0x11223344)),
        Type::Opaque { identity: NodeId(0x11223344), hidden: Box::new(Type::Tuple(vec![])) },
        Type::Unsupported("unsupported/未实现".into()),
        Type::Record { name: "@r".into(), fields: vec![("a".into(), Type::U8), ("b".into(), Type::I64)] },
        Type::Array { element: Box::new(Type::I64), length: 0x12345678 },
        Type::Enum { name: "@E".into(), variants: vec![("A".into(), vec![]), ("B".into(), vec![Type::Tuple(vec![Type::I64, Type::Bool]), Type::U8])] },
        Type::ErrorUnion { error: Box::new(Type::U8), result: Box::new(Type::I64) },
        package.clone(), Type::U8, Type::U16, Type::ForeignHandle, Type::CWideString,
        Type::ForeignCallback { parameters: vec![Type::ForeignHandle, Type::I64], result: Box::new(Type::Unit) },
        Type::Slice { element: Box::new(Type::U16) }, Type::ByteBuffer,
        Type::OwnedStore { element: Box::new(package.clone()) },
        Type::Tuple(vec![]),
        Type::Record { name: "empty-record".into(), fields: vec![] },
        Type::Package { name: "empty-package".into(), fields: vec![] },
        Type::Enum { name: "empty-enum".into(), variants: vec![] },
        Type::Enum { name: "empty-variants".into(), variants: vec![("A".into(), vec![]), ("B".into(), vec![])] },
        Type::FunctionPointer { parameters: vec![], result: Box::new(Type::I32) },
        Type::ForeignCallback { parameters: vec![], result: Box::new(Type::Unit) },
        Type::Address { mutable: true, inner: Box::new(Type::Slice { element: Box::new(Type::U16) }) },
        Type::Enum { name: "outer".into(), variants: vec![
            ("before".into(), vec![]),
            ("middle".into(), vec![
                Type::Record { name: "left".into(), fields: vec![
                    ("nested".into(), Type::Enum { name: "inner".into(), variants: vec![
                        ("filled".into(), vec![Type::Record { name: "deep".into(), fields: vec![("value".into(), Type::Bool)] }]),
                        ("empty".into(), vec![]),
                    ]}),
                    ("tail".into(), Type::I32),
                ]},
                Type::Enum { name: "right".into(), variants: vec![("none".into(), vec![]), ("pair".into(), vec![Type::U16, Type::Tuple(vec![])])] },
            ]),
            ("after".into(), vec![Type::I64]),
        ]},
    ];
    types.push(Type::Record { name: "mixed".into(), fields: vec![
        ("before".into(), Type::Array { element: Box::new(Type::Tuple(types.clone())), length: 2 }),
        ("store".into(), Type::Managed(Box::new(Type::OwnedStore { element: Box::new(package) }))),
        ("after".into(), Type::I32),
    ]});
    types
}

#[test]
fn every_type_tag_and_empty_container_preserves_legacy_bytes_and_exact_node_charge() {
    let mut prefix = wire::encode_ct(&fixture(Type::I32), &Limits::default()).unwrap();
    assert_eq!(prefix.pop(), Some(2));
    let mut truncations = 0;
    for (index, ty) in fixtures().into_iter().enumerate() {
        let mut expected = prefix.clone();
        let type_cost = old_type_bytes(&ty, &mut expected);
        let module = fixture(ty);
        let precise = Limits { max_nodes: 2 + type_cost, ..Limits::default() };
        let actual = wire::encode_ct(&module, &precise).unwrap();
        assert_eq!(actual, expected, "fixture {index}: legacy byte order changed");
        assert_eq!(wire::decode_ct(&expected, &precise).unwrap(), module);
        let short = Limits { max_nodes: precise.max_nodes - 1, ..precise.clone() };
        for errors in [wire::encode_ct(&module, &short).unwrap_err(), wire::decode_ct(&expected, &short).unwrap_err()] {
            assert!(errors.iter().all(|e| e.category == ResultCategory::ResourceLimit), "fixture {index}: {errors:?}");
        }
        for end in prefix.len()..expected.len() {
            assert!(wire::decode_ct(&expected[..end], &Limits::default()).is_err(), "fixture {index} prefix {end} accepted");
            truncations += 1;
        }
        println!("legacy fixture {index}: bytes={}, exact nodes={}", expected.len(), precise.max_nodes);
    }
    println!("independent truncated type records rejected: {truncations}");
}

fn nested(layers: usize) -> Type {
    let mut ty = Type::I32;
    for _ in 0..layers { ty = Type::Tuple(vec![ty]); }
    ty
}

#[test]
fn deep_writer_and_manual_wire_reader_are_bounded_on_a_128k_stack() {
    let mut prefix = wire::encode_ct(&fixture(Type::I32), &Limits::default()).unwrap();
    assert_eq!(prefix.pop(), Some(2));
    for layers in [255usize, 256, 258] {
        let module = fixture(nested(layers));
        let (module, encoded) = std::thread::Builder::new().stack_size(128 * 1024).spawn(move || {
            let encoded = wire::encode_ct(&module, &Limits::default());
            (module, encoded)
        }).unwrap().join().expect("writer must not abort a bounded stack");
        let mut bytes = prefix.clone();
        for _ in 0..layers { bytes.extend_from_slice(&[4, 1, 0, 0, 0]); }
        bytes.push(2);
        let decoded = std::thread::Builder::new().stack_size(128 * 1024).spawn(move || {
            wire::decode_ct(&bytes, &Limits::default())
        }).unwrap().join().expect("reader must not abort a bounded stack");
        if layers == 255 {
            assert!(encoded.is_ok(), "{encoded:?}");
            assert_eq!(decoded.unwrap(), module);
        } else {
            for errors in [encoded.unwrap_err(), decoded.unwrap_err()] {
                assert!(errors.iter().all(|e| e.category == ResultCategory::ResourceLimit && e.message.contains("nesting quota")), "{errors:?}");
            }
        }
        println!("128 KiB stack, {layers} Tuple layers: {}", if layers == 255 {"Pass"} else {"ResourceLimit"});
    }
}

#[test]
fn invalid_tags_counts_booleans_and_utf8_keep_precise_wire_failures() {
    let mut prefix = wire::encode_ct(&fixture(Type::I32), &Limits::default()).unwrap();
    prefix.pop();
    for (raw, code, category) in [
        (vec![26], "E-WIRE-TYPE", ResultCategory::SafeReject),
        (vec![255], "E-WIRE-TYPE", ResultCategory::SafeReject),
        (vec![6, 2, 2], "E-WIRE-BOOL", ResultCategory::SafeReject),
        (vec![5, 1, 0, 0, 0, 255], "E-WIRE-UTF8", ResultCategory::SafeReject),
        (vec![4, 255, 255, 255, 255, 2], "E-RESOURCE-LIMIT", ResultCategory::ResourceLimit),
    ] {
        let mut bytes = prefix.clone(); bytes.extend(raw);
        let errors = wire::decode_ct(&bytes, &Limits::default()).unwrap_err();
        assert!(errors.iter().any(|e| e.code == code && e.category == category), "{errors:?}");
    }
}
