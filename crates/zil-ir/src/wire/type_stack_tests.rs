//! Regression for the reproduced default-thread wire walker exhaustion.
use super::*;

fn chain(levels: usize) -> Type {
    let mut ty = Type::I32;
    for _ in 0..levels {
        ty = Type::Tuple(vec![ty]);
    }
    ty
}

fn chain_bytes(levels: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    for _ in 0..levels {
        bytes.extend_from_slice(&[4, 1, 0, 0, 0]);
    }
    bytes.push(2);
    bytes
}

#[test]
fn default_type_depth_boundary_is_a_result_on_the_normal_test_stack() {
    let limits = Limits::default();
    assert_eq!(limits.max_call_depth, 256);
    let ty = chain(255);
    let expected = chain_bytes(255);
    let mut writer = Writer::new(&limits);
    writer.ty(&ty, 0).unwrap();
    assert_eq!(writer.output, expected);
    let mut reader = Reader::new(&expected, &limits).unwrap();
    assert_eq!(reader.ty(0).unwrap(), ty);
    assert_eq!(reader.remaining(), 0);
    assert_eq!(reader.nodes, writer.nodes);

    let too_deep = chain(256);
    let err = Writer::new(&limits).ty(&too_deep, 0).unwrap_err();
    assert_eq!(err.code, "E-RESOURCE-LIMIT");
    let bytes = chain_bytes(256);
    let err = Reader::new(&bytes, &limits).unwrap().ty(0).unwrap_err();
    assert_eq!(err.code, "E-RESOURCE-LIMIT");
}

#[test]
fn independently_encoded_258_level_input_rejects_before_host_stack_exhaustion() {
    let limits = Limits::default();
    let bytes = chain_bytes(258);
    let err = Reader::new(&bytes, &limits).unwrap().ty(0).unwrap_err();
    assert_eq!(err.code, "E-RESOURCE-LIMIT");
    let raised = Limits {
        max_call_depth: 266,
        ..limits
    };
    let ty = chain(258);
    let mut writer = Writer::new(&raised);
    writer.ty(&ty, 0).unwrap();
    assert_eq!(writer.output, bytes);
    let mut reader = Reader::new(&bytes, &raised).unwrap();
    assert_eq!(reader.ty(0).unwrap(), ty);
    assert_eq!(reader.nodes, writer.nodes);
}

#[test]
fn type_nodes_keep_the_exact_preexisting_collection_and_child_charge() {
    let ty = Type::Tuple(vec![
        Type::I32,
        Type::Array {
            element: Box::new(Type::Bool),
            length: 3,
        },
    ]);
    // Tuple node + two collection entries + I32 + Array + Bool = six.
    let expected = vec![4, 2, 0, 0, 0, 2, 14, 1, 3, 0, 0, 0];
    let exact = Limits {
        max_nodes: 6,
        ..Limits::default()
    };
    let mut writer = Writer::new(&exact);
    writer.ty(&ty, 0).unwrap();
    assert_eq!(writer.output, expected);
    assert_eq!(writer.nodes, 0);
    let mut reader = Reader::new(&expected, &exact).unwrap();
    assert_eq!(reader.ty(0).unwrap(), ty);
    assert_eq!(reader.nodes, 0);
    let below = Limits {
        max_nodes: 5,
        ..exact
    };
    assert_eq!(
        Writer::new(&below).ty(&ty, 0).unwrap_err().code,
        "E-RESOURCE-LIMIT"
    );
    assert_eq!(
        Reader::new(&expected, &below)
            .unwrap()
            .ty(0)
            .unwrap_err()
            .code,
        "E-RESOURCE-LIMIT"
    );
}
