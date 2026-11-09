#!/usr/bin/env python3
"""Apply only to the frozen, already-tested ordinary fallback candidate."""
from pathlib import Path
import hashlib
import sys
p = Path('crates/zil-backend-llvm/src/machine_cgu.rs')
s = p.read_text()
assert hashlib.sha256(p.read_bytes()).hexdigest() == '5bde6e4be6137dd467f29f885973c74ca365972dbfc087388937382134f47f90'
start = s.index('// Complete implementation import is an optional optimization.')
end = s.index('fn partition_view_once(', s.index('\nfn partition_view_once(', start))
s = s[:start] + '''// Import profitability never authorizes a validation shortcut. Both policies
// consume the same immutable graph and emit through the same checked layout.
#[derive(Clone, Copy)]
enum ImportPolicy {
    Disabled,
    Complete { ordinary_on_cost: bool },
}

fn partition_view_recorded(
    module: Module,
    lir: &LirView<'_>,
    entry: EntryPoint,
    count: usize,
    split_large_files: bool,
    import_complete_call_closures: bool,
    recorded: &mut Option<layout::Layout>,
) -> Plan {
    partition_view_planned(module, lir, entry, count, split_large_files,
        if import_complete_call_closures {
            ImportPolicy::Complete { ordinary_on_cost: true }
        } else { ImportPolicy::Disabled }, recorded)
}

// Preserve the original non-adaptive planner as a test oracle. It shares only
// unchanged planning primitives, not the adaptive fallback decision.
#[cfg(test)]
fn partition_view_once(
    module: Module,
    lir: &LirView<'_>,
    entry: EntryPoint,
    count: usize,
    split_large_files: bool,
    import_complete_call_closures: bool,
    recorded: &mut Option<layout::Layout>,
) -> Plan {
    partition_view_planned(module, lir, entry, count, split_large_files,
        if import_complete_call_closures {
            ImportPolicy::Complete { ordinary_on_cost: false }
        } else { ImportPolicy::Disabled }, recorded)
}

''' + s[end:]
old = '''fn partition_view_once(
    module: Module,
    lir: &LirView<'_>,
    entry: EntryPoint,
    count: usize,
    split_large_files: bool,
    import_complete_call_closures: bool,
    recorded: &mut Option<layout::Layout>,
) -> Plan {
    let fallback'''
new = '''fn partition_view_planned(
    module: Module,
    lir: &LirView<'_>,
    entry: EntryPoint,
    count: usize,
    split_large_files: bool,
    import_policy: ImportPolicy,
    recorded: &mut Option<layout::Layout>,
) -> Plan {
    let import_complete_call_closures = matches!(import_policy, ImportPolicy::Complete { .. });
    let fallback'''
assert s.count(old) == 1
s = s.replace(old,new)
old = '    let mut owners = roots.iter().map(|r| buckets[r]).collect::<Vec<_>>();\n'
new = old + '''    // Only the optional small-caller policy changes ordinary ownership. Keep
    // that small index vector when needed, never clone the machine bodies.
    let ordinary_owners = caller_components.as_ref().map(|_| owners.clone());
'''
assert s.count(old) == 1
s = s.replace(old,new)
old = '        let mut copied_cost = 0usize;\n        for (&owner, definitions) in &members {'
new = '''        let mut copied_cost = 0usize;
        let mut over_budget = false;
        'owners: for (&owner, definitions) in &members {'''
assert s.count(old) == 1
s=s.replace(old,new)
old = '''                let Some(total) = copied_cost.checked_add(function.cost()) else {
                    return fallback(module, "complete O2 call closure exceeds total copy cost");
                };
                if total > original_cost {
                    return fallback(module, "complete O2 call closure exceeds total copy cost");
                }'''
new = '''                let Some(total) = copied_cost.checked_add(function.cost()) else {
                    over_budget = true;
                    break 'owners;
                };
                if total > original_cost {
                    over_budget = true;
                    break 'owners;
                }'''
assert s.count(old)==1
s=s.replace(old,new)
old = '''            imports.insert(owner, imported);
        }
    }
    for (&owner, imported) in &imports {'''
new = '''            imports.insert(owner, imported);
        }
        if over_budget {
            if !matches!(import_policy, ImportPolicy::Complete { ordinary_on_cost: true }) {
                return fallback(module, "complete O2 call closure exceeds total copy cost");
            }
            // No replay or mutation has happened. Discard every partial import
            // and recover the exact ordinary owners; reuse symbol/reference,
            // linkage and source indexes instead of scanning all IR again.
            imports.clear();
            if let Some(original) = ordinary_owners {
                owners = original;
                members.clear();
                for (id, &owner) in owners.iter().enumerate() {
                    members.entry(owner).or_default().push(id);
                }
            }
        }
    }
    for (&owner, imported) in &imports {'''
assert s.count(old)==1
s=s.replace(old,new)
anchor = '    #[test]\n    fn canonical_owner_can_be_first_middle_or_last_consumer() {'
tests = '''    #[test]
    fn adaptive_planner_matches_two_pass_oracle_across_graph_families() {
        for shape in 0..5 {
            let source = candidate_lir(&(0..768).collect::<Vec<_>>());
            let items = (0..768).map(|id| {
                let target = match shape {
                    0 => (id + 1) % 768,
                    1 => (id * 17 + 257) % 768,
                    2 => (id / 2 + 512) % 768,
                    3 => (id + 256) % 768,
                    _ => 511,
                };
                call_function(&format!("zil.fn.{id}"), Ty::Int(32),
                    &format!("zil.fn.{target}"))
            }).collect();
            let module = m(items);
            for count in [2, 3, 8, 17, 64] {
                let mut old_layout = None;
                let mut expected = partition_view_once(module.clone(), &source.as_view(),
                    EntryPoint::None, count, true, true, &mut old_layout);
                if expected.fallback == Some("complete O2 call closure exceeds total copy cost") {
                    old_layout = None;
                    expected = partition_view_once(module.clone(), &source.as_view(),
                        EntryPoint::None, count, true, false, &mut old_layout);
                }
                let mut new_layout = None;
                let actual = partition_view_recorded(module.clone(), &source.as_view(),
                    EntryPoint::None, count, true, true, &mut new_layout);
                assert_eq!(actual.fallback, expected.fallback, "shape={shape} count={count}");
                assert_eq!(actual.units, expected.units, "shape={shape} count={count}");
                assert_eq!(new_layout, old_layout);
                for unit in &actual.units { zil_llvm_api::validate_module(unit).unwrap(); }
            }
        }
    }

'''
assert s.count(anchor)==1
s=s.replace(anchor,tests+anchor)
if '--apply' not in sys.argv:
    import difflib
    print(''.join(difflib.unified_diff(p.read_text().splitlines(True),s.splitlines(True),fromfile=str(p),tofile=str(p))))
else:
    p.write_text(s)
    print('candidate_before_rustfmt_sha256='+hashlib.sha256(p.read_bytes()).hexdigest())
