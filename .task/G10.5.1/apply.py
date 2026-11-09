#!/usr/bin/env python3
"""Materialize a bounded CGU-coarsening experiment on an exact source blob.

No toolchain, cache limit, language rule, LLVM pipeline or validation pass changes.
Without --apply, only print the unified source diff. The research recipe itself
is NOT an applied or accepted production optimization.
"""
from __future__ import annotations
import argparse
import difflib
import hashlib
from pathlib import Path

PATH = Path("crates/zil-backend-llvm/src/machine_cgu.rs")
EXPECTED_BLOB = "c0a16e723af2a306d24792172a23d91acd5a2691"
ANCHOR = """fn partition_view_recorded(
    module: Module,
    lir: &LirView<'_>,
    entry: EntryPoint,
    count: usize,
    split_large_files: bool,
    import_complete_call_closures: bool,
    recorded: &mut Option<layout::Layout>,
) -> Plan {
"""
REPLACEMENT = """// A copy-cost rejection is a partitioning-policy failure, not a language or
// machine-validation failure. Retry a geometrically smaller partition count
// before abandoning parallel code generation. Every attempt still constructs a
// complete call closure and applies the original total-copy-cost bound.
fn partition_view_recorded(
    mut module: Module,
    lir: &LirView<'_>,
    entry: EntryPoint,
    mut count: usize,
    split_large_files: bool,
    import_complete_call_closures: bool,
    recorded: &mut Option<layout::Layout>,
) -> Plan {
    loop {
        *recorded = None;
        let plan = partition_view_once(
            module,
            lir,
            entry,
            count,
            split_large_files,
            import_complete_call_closures,
            recorded,
        );
        if count <= 2
            || plan.fallback != Some("complete O2 call closure exceeds total copy cost")
        {
            return plan;
        }
        // This rejection precedes replay: the sole unit is the untouched input,
        // not a partial split. Move it into the next attempt without cloning IR.
        assert_eq!(plan.units.len(), 1);
        module = plan.units.into_iter().next().expect("complete input module");
        count = count.div_ceil(2);
    }
}

fn partition_view_once(
    module: Module,
    lir: &LirView<'_>,
    entry: EntryPoint,
    count: usize,
    split_large_files: bool,
    import_complete_call_closures: bool,
    recorded: &mut Option<layout::Layout>,
) -> Plan {
"""
OLD_TEST = """        assert_eq!(
            plan.fallback,
            Some("complete O2 call closure exceeds total copy cost")
        );
        assert_eq!(plan.units, vec![module]);
        zil_llvm_api::validate_module(&plan.units[0]).unwrap();
"""
NEW_TEST = """        assert!(plan.fallback.is_none());
        assert_eq!(plan.units.len(), 2);
        for unit in &plan.units {
            zil_llvm_api::validate_module(unit).unwrap();
        }
        // Preserve the old rejection as an explicit one-attempt oracle. The
        // new behavior must not be obtained by raising the bound or truncating.
        let mut recorded = None;
        let rejected = partition_view_once(
            module.clone(),
            &source.as_view(),
            EntryPoint::None,
            8,
            true,
            true,
            &mut recorded,
        );
        assert_eq!(
            rejected.fallback,
            Some("complete O2 call closure exceeds total copy cost")
        );
        assert_eq!(rejected.units, vec![module.clone()]);
        assert!(recorded.is_none());

        let accepted = partition_view_recorded(
            module.clone(),
            &source.as_view(),
            EntryPoint::None,
            8,
            true,
            true,
            &mut recorded,
        );
        assert_eq!(accepted.units, plan.units);
        assert_eq!(recorded.unwrap().replay(module.clone()), plan.units);
        let original_cost = module
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Function(function) => Some(function.cost()),
                _ => None,
            })
            .sum::<usize>();
        let copied_cost = plan
            .units
            .iter()
            .flat_map(|unit| &unit.items)
            .filter_map(|item| match item {
                Item::Function(function)
                    if function.blocks.is_some()
                        && function.name.starts_with("zil.import.") =>
                {
                    Some(function.cost())
                }
                _ => None,
            })
            .sum::<usize>();
        assert!(copied_cost <= original_cost);
        for id in 0..600 {
            let name = format!("zil.fn.{id}");
            assert_eq!(
                plan.units
                    .iter()
                    .flat_map(|unit| &unit.items)
                    .filter(|item| matches!(item, Item::Function(function)
                        if function.name == name && function.blocks.is_some()))
                    .count(),
                1
            );
        }
        // Odd requests converge without skipping the two-unit candidate.
        let odd = partition_view(
            module,
            &source.as_view(),
            EntryPoint::None,
            3,
            true,
            true,
        );
        assert!(odd.fallback.is_none());
        assert_eq!(odd.units, plan.units);
"""


def transform(text: str) -> str:
    for literal in (ANCHOR, OLD_TEST, "fn multiply_shared_closure_falls_back_before_copying_any_body()"):
        if text.count(literal) != 1:
            raise ValueError("source anchor absent or ambiguous; review the new baseline")
    return (text.replace(ANCHOR, REPLACEMENT, 1)
            .replace(OLD_TEST, NEW_TEST, 1)
            .replace("fn multiply_shared_closure_falls_back_before_copying_any_body()",
                     "fn multiply_shared_closure_coarsens_without_weakening_copy_bound()", 1))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path("."))
    parser.add_argument("--apply", action="store_true")
    args = parser.parse_args()
    path = args.repo.resolve() / PATH
    raw = path.read_bytes()
    identity = hashlib.sha1(b"blob " + str(len(raw)).encode() + b"\0" + raw).hexdigest()
    if identity != EXPECTED_BLOB:
        parser.error(f"exact source blob required: {EXPECTED_BLOB}; observed {identity}")
    old = raw.decode("utf-8")
    new = transform(old)
    print("".join(difflib.unified_diff(old.splitlines(True), new.splitlines(True),
                                    fromfile="a/" + str(PATH), tofile="b/" + str(PATH))), end="")
    if args.apply:
        path.write_bytes(new.encode("utf-8"))
        print(f"candidate_source_sha256={hashlib.sha256(new.encode('utf-8')).hexdigest()}")


if __name__ == "__main__":
    main()
