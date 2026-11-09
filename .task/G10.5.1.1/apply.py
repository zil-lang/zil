#!/usr/bin/env python3
"""Apply to the exact pinned main. This changes an optimization policy, not validation."""
from pathlib import Path
import hashlib, subprocess
ROOT=Path(__file__).resolve().parents[2]
p=ROOT/'crates/zil-backend-llvm/src/machine_cgu.rs'
old=p.read_text()
blob=hashlib.sha1(b'blob '+str(len(old.encode())).encode()+b'\0'+old.encode()).hexdigest()
assert blob=='c0a16e723af2a306d24792172a23d91acd5a2691',blob
anchor='fn partition_view_recorded(\n'
assert old.count(anchor)==1
wrapper='''// Complete implementation import is an optional optimization. A rejected copy
// budget must not turn an already valid ordinary multi-unit plan into a single
// expensive object. Reuse the original ordinary planner, without partial imports:
// canonical calls, local-state components and current preflight stay intact.
// Runtime quality is a separate acceptance gate for this experimental policy.
fn partition_view_recorded(
    module: Module,
    lir: &LirView<'_>,
    entry: EntryPoint,
    count: usize,
    split_large_files: bool,
    import_complete_call_closures: bool,
    recorded: &mut Option<layout::Layout>,
) -> Plan {
    let plan = partition_view_once(
        module, lir, entry, count, split_large_files,
        import_complete_call_closures, recorded,
    );
    if plan.fallback != Some("complete O2 call closure exceeds total copy cost") {
        return plan;
    }
    // The budget rejection occurs before replay and returns the untouched model.
    // Move it, rather than cloning or attempting to finish a partial import set.
    assert_eq!(plan.units.len(), 1);
    let module = plan.units.into_iter().next().expect("complete input module");
    *recorded = None;
    partition_view_once(module, lir, entry, count, split_large_files, false, recorded)
}

fn partition_view_once(
'''
new=old.replace(anchor,wrapper,1)
start=new.index('    #[test]\n    fn multiply_shared_closure_falls_back_before_copying_any_body()')
end=new.index('    #[test]\n    fn canonical_owner_can_be_first_middle_or_last_consumer()',start)
new=new[:start]+'''    #[test]
    fn expensive_complete_import_keeps_the_exact_ordinary_plan() {
        let source = candidate_lir(&(0..600).collect::<Vec<_>>());
        let module = m((0..600).map(|id| call_function(
            &format!("zil.fn.{id}"), Ty::Int(32),
            &format!("zil.fn.{}", (id + 1) % 600),
        )).collect());
        let mut recorded = None;
        let rejected = partition_view_once(
            module.clone(), &source.as_view(), EntryPoint::None, 8, true, true,
            &mut recorded,
        );
        assert_eq!(rejected.fallback, Some("complete O2 call closure exceeds total copy cost"));
        assert_eq!(rejected.units, vec![module.clone()]);
        assert!(recorded.is_none());
        let ordinary = partition_view(
            module.clone(), &source.as_view(), EntryPoint::None, 8, true, false,
        );
        let adaptive = partition_view_recorded(
            module.clone(), &source.as_view(), EntryPoint::None, 8, true, true,
            &mut recorded,
        );
        assert!(adaptive.fallback.is_none());
        assert_eq!(adaptive.units.len(), 3);
        assert_eq!(adaptive.units, ordinary.units);
        assert_eq!(recorded.unwrap().replay(module), ordinary.units);
        for unit in &adaptive.units {
            zil_llvm_api::validate_module(unit).unwrap();
            assert!(unit.items.iter().all(|item| !matches!(item,
                Item::Function(function) if function.name.starts_with("zil.import."))));
        }
        for id in 0..600 {
            let name = format!("zil.fn.{id}");
            assert_eq!(adaptive.units.iter().flat_map(|unit| &unit.items)
                .filter(|item| matches!(item, Item::Function(function)
                    if function.name == name && function.blocks.is_some())).count(), 1);
        }
    }

'''+new[end:]
p.write_text(new)
# Keep option documentation honest: this is NOT the previous complete-import promise.
for name,before,after in [
    ('crates/zil-cli/src/main.rs','O2 build: complete call closures with bounded total copying','O2 build: bounded complete imports or ordinary multi-unit fallback'),
    ('crates/zil-driver/src/lib.rs','Explicit O2 partitioning with complete direct-call implementation copies.','Explicit O2 bounded complete imports; copy-cost overflow keeps ordinary units.'),
]:
    path=ROOT/name; text=path.read_text(); assert text.count(before)==1,(name,before)
    path.write_text(text.replace(before,after))
print('candidate_machine_sha256='+hashlib.sha256(p.read_bytes()).hexdigest())
