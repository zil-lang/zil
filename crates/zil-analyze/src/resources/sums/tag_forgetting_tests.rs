//! The scheduled tag operation must preserve the complete old tree semantics.
use super::*;

fn scalar_enum(name: &str) -> Type {
    Type::Enum {
        name: name.into(),
        variants: vec![("First".into(), vec![]), ("Second".into(), vec![])],
    }
}

fn vary(tree: &mut Arc<Tree>, tags: usize, missing: usize, at: &mut usize) {
    let node = Arc::make_mut(tree);
    node.missing = missing & (1 << (*at % 8)) != 0;
    *at += 1;
    match &mut node.node {
        Node::Product(fields) => {
            for field in fields {
                vary(field, tags, missing, at);
            }
        }
        Node::Sum(choices) => {
            // Every retained alternative keeps its original index and fields.
            // Zero is the ordinary unrefined state; the other two choices are
            // the actual states obtained by either outcome of a tag guard.
            let selected = (tags >> (*at % 4)) % 3;
            if selected != 0 {
                choices.retain(|tag, _| *tag == (selected - 1) as u32);
            }
            for fields in choices.values_mut() {
                for field in fields {
                    vary(field, tags, missing, at);
                }
            }
        }
        _ => {}
    }
}

#[test]
fn scheduled_forgetting_matches_full_reconstruction_with_nested_tags_and_moves() {
    let inner = scalar_enum("Inner");
    let outer = Type::Enum {
        name: "Outer".into(),
        variants: vec![
            ("Pair".into(), vec![inner.clone(), Type::I32]),
            ("Other".into(), vec![inner.clone()]),
        ],
    };
    let union = Type::ErrorUnion {
        error: Box::new(inner.clone()),
        result: Box::new(outer.clone()),
    };
    let owner = Type::Package {
        name: "Owner".into(),
        fields: vec![
            ("data".into(), Type::Managed(Box::new(Type::ByteBuffer))),
            ("state".into(), union.clone()),
        ],
    };
    for ty in [inner, outer, union, owner] {
        for tags in 0..16 {
            for missing in 0..32 {
                let mut budget = Budget::new(&Limits::default());
                let mut original = Tree::unknown(&ty, &mut budget).unwrap();
                vary(&mut original, tags, missing, &mut 0);
                let mut scheduled = Arc::clone(&original);
                original
                    .forget_addressed_copy_tags(&ty, &mut Budget::new(&Limits::default()))
                    .unwrap();
                let mut budget = Budget::new(&Limits::default());
                budget.store_option_results = true;
                budget.ordinary_error_places = true;
                scheduled
                    .forget_addressed_copy_tags(&ty, &mut budget)
                    .unwrap();
                assert_eq!(
                    original, scheduled,
                    "complete tag/move/resource tree differs: {ty:?}, tags={tags}, missing={missing}"
                );
            }
        }
    }
}
