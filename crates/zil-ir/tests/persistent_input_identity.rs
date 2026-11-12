//! Independent integration tests for the public persistent input API.
//! Not executed by the audit agent. Root may run these in its single test window.
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Fingerprint, ModuleId, SCHEMA_VERSION, Span, TargetProfile};
use zil_ir::*;

#[derive(Debug)]
struct Item {
    value: u32,
    clones: Arc<AtomicUsize>,
}
impl Clone for Item {
    fn clone(&self) -> Self {
        self.clones.fetch_add(1, Ordering::SeqCst);
        Self {
            value: self.value,
            clones: self.clones.clone(),
        }
    }
}
impl PersistentItem for Item {
    fn persistent_fingerprint(&self) -> Fingerprint {
        Fingerprint::of(&self.value.to_le_bytes())
    }
}
fn sequence(len: usize, clones: &Arc<AtomicUsize>) -> PersistentVec<Item> {
    (0..len)
        .map(|value| Item {
            value: value as u32,
            clones: clones.clone(),
        })
        .collect()
}
#[test]
fn independent_owned_consumption_moves_unique_leaves_and_preserves_shared_snapshot() {
    let clones = Arc::new(AtomicUsize::new(0));
    let values = sequence(1025, &clones);
    assert_eq!(
        values.into_iter().map(|v| v.value).sum::<u32>(),
        (0..1025).sum()
    );
    assert_eq!(clones.load(Ordering::SeqCst), 0);
    let original = sequence(65, &clones);
    let retained = original.clone();
    assert_eq!(
        original.into_iter().map(|v| v.value).collect::<Vec<_>>(),
        (0..65).collect::<Vec<_>>()
    );
    assert_eq!(retained.get(64).unwrap().value, 64);
    assert_eq!(clones.load(Ordering::SeqCst), 65);
}
#[test]
fn independent_diff_matches_all_actual_owners_across_capacity_and_prefix_changes() {
    let clones = Arc::new(AtomicUsize::new(0));
    let whole = sequence(257, &clones);
    let sizes = [
        0, 1, 2, 3, 7, 8, 9, 15, 16, 17, 63, 64, 65, 127, 128, 129, 255, 256, 257,
    ];
    for &a in &sizes {
        let mut left = whole.clone();
        left.truncate(a);
        for &b in &sizes {
            let mut right = whole.clone();
            right.truncate(b);
            if b > 2 {
                right[b / 2].value += 1234;
            }
            let expected = (0..a.max(b))
                .filter(|&i| match (left.owner(i), right.owner(i)) {
                    (Some(x), Some(y)) => !Arc::ptr_eq(x, y),
                    _ => true,
                })
                .collect::<Vec<_>>();
            assert_eq!(left.changed_indices(&right), expected, "lengths {a}/{b}");
            assert_eq!(
                right.changed_indices(&left),
                expected,
                "reverse lengths {a}/{b}"
            );
        }
    }
}
#[test]
fn independent_ranges_survive_interleaved_front_back_and_partial_mutation_keeps_hash_current() {
    let clones = Arc::new(AtomicUsize::new(0));
    let original = sequence(129, &clones);
    for start in 0..=129 {
        for end in [start, (start + 1).min(129), 129] {
            let mut iter = original.range(start..end);
            let mut lo = start;
            let mut hi = end;
            while lo < hi {
                assert_eq!(iter.next().unwrap().value, lo as u32);
                lo += 1;
                if lo < hi {
                    hi -= 1;
                    assert_eq!(iter.next_back().unwrap().value, hi as u32);
                }
                assert_eq!(iter.len(), hi - lo);
            }
            assert!(iter.next().is_none());
            assert!(iter.next_back().is_none());
        }
    }
    assert_eq!(clones.load(Ordering::SeqCst), 0);
    let old = original.structural_fingerprint();
    let mut edited = original.clone();
    edited.iter_mut().nth(64).unwrap().value = 999;
    let rebuilt: PersistentVec<Item> = edited
        .iter()
        .map(|x| Item {
            value: x.value,
            clones: clones.clone(),
        })
        .collect();
    assert_eq!(
        edited.structural_fingerprint(),
        rebuilt.structural_fingerprint()
    );
    assert_ne!(old, edited.structural_fingerprint());
    assert_eq!(original.structural_fingerprint(), old);
    let mut appended = original.clone();
    appended.push(Item {
        value: 129,
        clones: clones.clone(),
    });
    assert_eq!(appended.changed_indices(&original), vec![129]);
    appended.pop();
    assert_eq!(
        appended.structural_fingerprint(),
        original.structural_fingerprint()
    );
    assert_eq!(appended.changed_indices(&original), Vec::<usize>::new());
}
fn raw_module() -> TypedModule {
    let span = Span::new(FileId(1), 10, 20);
    TypedModule {
        dyn_declarations: vec![],
        dyn_interfaces: vec![],
        dyn_vtables: vec![],
        life_declarations: vec![],
        life_instances: vec![],
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(1),
        source_fingerprint: Fingerprint::of(b"audit"),
        target: TargetProfile::host_llvm(),
        functions: vec![TypedFunction {
            storage_life: None,
            call_kind: CallKind::Direct,
            reference_return_bound: None,
            callable_return_contract: None,
            callable_contract: None,
            protocol_contract: None,
            id: FunctionId(0),
            name: "raw".into(),
            foreign: None,
            environment: None,
            parameters: vec![],
            return_type: Type::I32,
            result_mode: PassingMode::Infer,
            locals: vec![LocalDecl {
                id: LocalId(0),
                name: "local".into(),
                ty: Type::I32,
                mutable: false,
                span,
            }]
            .into(),
            values: vec![Type::I32].into(),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction {
                    result: Some(ValueId(0)),
                    kind: InstructionKind::Constant(Constant::I32(1)),
                    span,
                }],
                terminator: Terminator::Return(Some(ValueId(0))),
            }]
            .into(),
            entry: BlockId(0),
            span,
        }]
        .into(),
        closures: vec![],
        resources: vec![],
        unsupported: vec![],
    }
}
#[test]
fn independent_structural_digest_binds_raw_headers_spans_values_and_new_integer_inputs() {
    let original = raw_module();
    let digest = original.structural_fingerprint();
    type Mutation = Box<dyn Fn(&mut TypedModule)>;
    let mutations: Vec<Mutation> = vec![
        Box::new(|m| m.schema_version += 1),
        Box::new(|m| m.module_id.0 += 1),
        Box::new(|m| m.source_fingerprint = Fingerprint::ZERO),
        Box::new(|m| m.target.checked_arithmetic = !m.target.checked_arithmetic),
        Box::new(|m| m.functions[0].name.push('x')),
        Box::new(|m| m.functions[0].id.0 += 1),
        Box::new(|m| m.functions[0].span.end += 1),
        Box::new(|m| m.functions[0].locals[0].span.start += 1),
        Box::new(|m| m.functions[0].locals[0].mutable = true),
        Box::new(|m| m.functions[0].values[0] = Type::U32),
        Box::new(|m| {
            m.functions[0].blocks[0].instructions[0].kind =
                InstructionKind::Constant(Constant::U32(1))
        }),
        Box::new(|m| m.functions[0].blocks[0].instructions[0].span.end += 1),
        Box::new(|m| m.functions[0].blocks[0].terminator = Terminator::Trap),
        Box::new(|m| m.functions[0].entry.0 += 1),
        Box::new(|m| m.functions[0].reference_return_bound = Some(vec![0])),
        Box::new(|m| {
            m.resources.push(ResourceDefinition {
                name: "resource".into(),
                payload: Type::I32,
                teardown: None,
            })
        }),
    ];
    for (index, mutate) in mutations.into_iter().enumerate() {
        let mut changed = original.clone();
        mutate(&mut changed);
        assert_ne!(changed.structural_fingerprint(), digest, "mutation {index}");
        assert_eq!(original.structural_fingerprint(), digest);
    }
}
