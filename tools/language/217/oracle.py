#!/usr/bin/env python3
"""Independent source-operation model; never reads compiler/native output.

The model predicts resource markers and actual realloc/free counts, including
every single allocation failure. It deliberately does not predict byte sizes:
the probe records requested payload peaks separately from these assertions.
"""
import json


def expected(mode, fail_at=0):
    trace, live = [], set()
    calls = allocated = frees = failures = peak = 0

    def reserve(key):
        nonlocal calls, allocated, failures, peak
        calls += 1
        if calls == fail_at:
            failures += 1
            return False
        if key not in live:
            allocated += 1
            live.add(key)
            peak = max(peak, len(live))
        return True

    def free(key):
        nonlocal frees
        if key in live:
            live.remove(key)
            frees += 1

    def node(identifier):
        return {"id": identifier, "active": identifier, "once": True}

    def drop_active(value):
        trace.append(chr(96 + value["active"]))
        free(("active", value["active"]))

    def drop(value):
        trace.append(chr(64 + value["id"]))
        drop_active(value)
        if value["once"]:
            trace.append(chr(48 + value["id"]))

    def finish(exit_code, nodes=()):
        for value in nodes:
            drop(value)
        free("outer")
        assert not live, live
        assert allocated == frees
        return {"exit": exit_code, "stdout": "".join(trace), "fail_at": fail_at,
                "allocation": {"alloc_calls": calls, "allocated": allocated,
                               "frees": frees, "failures": failures, "live": 0,
                               "peak_live": peak}}

    if mode is None:
        return finish(9)
    first = node(1)
    if not reserve("outer"):
        drop(first)  # seed's still-owned argument, before main sees empty Store
        trace.append("F")
        return finish(1)
    nodes = [first, node(2), node(3)]
    trace.append("S")
    if mode == 4:
        drop(node(4))  # full push returns the complete rejected value
        trace.append("Z")
        return finish(0, nodes)
    for _ in range(mode):
        for value in nodes:
            if not reserve(("active", value["active"])):
                trace.append("F")
                return finish(1, nodes)
        trace.append(".")
    if mode:
        if not reserve("suffix"):
            trace.append("F")
            return finish(1, nodes)
        drop(nodes.pop(1))
        free("suffix")  # the surviving suffix is moved back, not destroyed
        trace.append("D")
        if mode == 3:
            trace.append("E")
            return finish(0, nodes)
        if mode == 2:
            drop_active(nodes[1])
            nodes[1]["active"] = 9
            trace.append("R")
        for value in nodes:
            if not reserve(("active", value["active"])):
                trace.append("F")
                return finish(1, nodes)
    while nodes:
        value = nodes.pop()
        trace.append(">")
        key = ("once", value["id"])
        okay = reserve(key)
        free(key)
        value["once"] = False
        trace.append(chr(48 + value["id"]))
        if not okay:
            trace.append("F")
        drop(value)
        if not okay:
            return finish(1, nodes)
    trace.append("Z")
    return finish(0)


def declaration():
    rows = []
    for name, stdin, mode in [(f"mode-{n}", str(n), n) for n in range(5)] + [
            ("invalid", "9", None), ("eof", "", None)]:
        ordinary = expected(mode)
        calls = ordinary["allocation"]["alloc_calls"]
        rows.append({"name": name, "input": stdin, "mode": mode,
                     "ordinary": {"exit": ordinary["exit"], "stdout": ordinary["stdout"]},
                     "probes": [expected(mode, fail) for fail in range(calls + 2)]})
    return {"status": "PredeclaredModelOnly", "kind": "source-operation-oracle",
            "scope": "finite synchronous five-module consumer; no product or Windows claim",
            "allocation_fields_observed_only": ["peak_bytes"], "rows": rows,
            "planned_executions": {
                "ordinary": len(rows) * 2,
                "instrumented": sum(len(r["probes"]) for r in rows) * 2,
                "allocation_failures": sum(p["allocation"]["failures"] for r in rows for p in r["probes"]) * 2}}


if __name__ == "__main__":
    print(json.dumps(declaration(), ensure_ascii=False, indent=2))
