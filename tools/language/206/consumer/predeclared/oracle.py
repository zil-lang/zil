#!/usr/bin/env python3
"""Predeclared ownership-order and allocation oracle, not ZIL execution.

This independent model does not invoke a compiler or read native observations.
The source draft uses no allocator except one bytes_reserve per Leaf factory.
"""
import json
from pathlib import Path


def expected(mode, fail_at=0):
    assert mode in range(8) and fail_at >= 0
    trace = []
    markers = {}
    buffers = {}
    calls = allocated = frees = failures = peak_live = peak_bytes = 0

    def marker(name, code):
        assert name not in markers
        markers[name] = chr(code)
        return name

    def drop_marker(name):
        trace.append(markers.pop(name))

    def make_leaf(label, size):
        nonlocal calls, allocated, failures, peak_live, peak_bytes
        code = ord(label)
        mark = marker("leaf-" + label, code)
        calls += 1
        if calls == fail_at:
            failures += 1
            trace.append("F")
            drop_marker(mark)
            return None
        assert label not in buffers
        buffers[label] = size
        allocated += 1
        peak_live = max(peak_live, len(buffers))
        peak_bytes = max(peak_bytes, sum(buffers.values()))
        return {"marker": mark, "buffer": label}

    def drop_leaf(value):
        nonlocal frees
        if value is None:
            return
        # Source declaration: lead, Marker, wide, ByteBuffer, tail.
        drop_marker(value["marker"])
        del buffers[value["buffer"]]
        frees += 1

    def make_middle(left, right, base):
        return {"before": marker("middle-" + chr(base), base), "left": left,
                "between": marker("middle-" + chr(base + 1), base + 1),
                "right": right, "after": marker("middle-" + chr(base + 2), base + 2)}

    def drop_middle(value):
        if value is None:
            return
        drop_marker(value["before"])
        drop_leaf(value["left"])
        drop_marker(value["between"])
        drop_leaf(value["right"])
        drop_marker(value["after"])

    def report(exit_code):
        assert not markers and not buffers
        assert allocated == frees
        return {"stdout": "".join(trace), "exit": exit_code,
                "allocation": {"alloc_calls": calls, "allocated": allocated,
                               "frees": frees, "failures": failures, "live": 0,
                               "peak_live": peak_live, "peak_bytes": peak_bytes}}

    left = make_leaf("A", 8)
    if left is None:
        return report(1)
    right = make_leaf("B", 16)
    if right is None:
        # The first complete Leaf was staged as an earlier actual argument.
        drop_leaf(left)
        return report(1)
    root = {"child": make_middle(left, right, 73)}

    if mode == 1:
        middle, root["child"] = root["child"], None
        current, middle["left"] = middle["left"], None
        old = current["marker"]
        current["marker"] = marker("replacement-X", 88)
        trace.append("t")
        drop_marker(old)
        middle["left"] = current
        root["child"] = middle
    elif mode in (2, 3):
        replacement_left = make_leaf("C", 24)
        if replacement_left is None:
            drop_middle(root["child"])
            return report(1)
        replacement_right = make_leaf("D", 32)
        if replacement_right is None:
            drop_leaf(replacement_left)
            drop_middle(root["child"])
            return report(1)
        if mode == 3:
            # The third Copy argument errors before middle() begins: L/M/N
            # have not been created; later prepared values clean up first.
            trace.append("E")
            drop_leaf(replacement_right)
            drop_leaf(replacement_left)
            drop_middle(root["child"])
            return report(1)
        prepared = make_middle(replacement_left, replacement_right, 76)
        drop_middle(root["child"])
        root["child"] = prepared
    elif mode == 4:
        middle, root["child"] = root["child"], None
        trace.append("Q")
        drop_middle(middle)
        drop_middle(root["child"])
        return report(0)
    elif mode == 5:
        middle, root["child"] = root["child"], None
        left, middle["left"] = middle["left"], None
        trace.append("R")
        drop_leaf(left)
        drop_middle(middle)
        drop_middle(root["child"])
        return report(0)
    elif mode == 6:
        # Complete Root transfer through an ordinary function and assignment.
        trace.append("H")
    elif mode == 7:
        middle, root["child"] = root["child"], None
        prepared = make_leaf("C", 24)
        if prepared is None:
            drop_middle(middle)
            drop_middle(root["child"])
            return report(1)
        drop_leaf(middle["left"])
        middle["left"] = prepared
        root["child"] = middle

    # checked() takes/refills complete children without a new allocation,
    # destructor or visible event. A value/layout mismatch emits '!'.
    trace.append("P")
    drop_middle(root["child"])
    return report(0)


def declaration():
    names = ["whole-three-level-close", "take-refill-independent-leaf",
             "replace-complete-middle", "rhs-last-copy-argument-failure",
             "take-child-early-return", "partial-child-early-return",
             "whole-parent-relay-reassignment", "replace-child-in-independent-local"]
    manual_stdout = ["PIAJBK", "tAPIXJBK", "IAJBKPLCMDN", "EDCIAJBK",
                     "QIAJBK", "RAIJBK", "HPIAJBK", "APICJBK"]
    rows = []
    for mode, name in enumerate(names):
        ordinary = expected(mode)
        assert ordinary["stdout"] == manual_stdout[mode]
        count = ordinary["allocation"]["alloc_calls"]
        probes = [{"name": "allocation", "fail_at": 0, **ordinary}]
        probes += [{"name": "failure-" + str(at), "fail_at": at, **expected(mode, at)}
                   for at in range(1, count + 1)]
        probes.append({"name": "after-last-allocation", "fail_at": count + 1,
                       **expected(mode, count + 1)})
        rows.append({"name": name, "input": str(mode),
                     "stdout": ordinary["stdout"], "exit": ordinary["exit"], "probes": probes})
    rows += [{"name": "invalid-input", "input": "9", "stdout": "", "exit": 9, "probes": []},
             {"name": "eof", "input": "", "stdout": "", "exit": 9, "probes": []}]
    instrumented = 2 * sum(len(row["probes"]) for row in rows)
    failed = 2 * sum(probe["allocation"]["failures"] for row in rows for probe in row["probes"])
    assert instrumented == 74 and failed == 42
    return {"schema_version": 1, "status": "PredeclaredModelOnly",
            "language_compilation": "NotRun", "language_execution": "NotRun",
            "model_is_not_language_safety_evidence": True,
            "platform_plan": ["Linux native", "Win64/Wine compatibility"],
            "native_windows": "NotRun", "optimization_plan": ["O0", "O2"],
            "planned_executions_per_platform": {"ordinary": 20, "instrumented": instrumented,
                                                 "total": 20 + instrumented, "actual_allocation_failures": failed},
            "allocation_scope": "Requested live ByteBuffer payload; excludes stack/inline ABI and process RSS",
            "expected_windows_bad_operations": 0, "oracles": rows}


if __name__ == "__main__":
    output = Path(__file__).with_name("ORACLE.json")
    assert not output.exists(), "the predeclared oracle must not be overwritten"
    output.write_text(json.dumps(declaration(), ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"status": "PredeclaredModelOnly", "language_execution": "NotRun",
                      "oracles": str(output), "planned_per_platform": 94}))
