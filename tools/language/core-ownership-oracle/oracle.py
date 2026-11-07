#!/usr/bin/env python3
"""Frozen source-operation oracle. Does not read compiler or native results.

Predicts observable marker order and actual realloc/free counts. Byte sizes are
reported by the shim, not predicted from an assumed machine layout.
"""
import json


def expected(count, mode, fail_at=0):
    trace, live = [], set()
    calls = allocated = frees = failures = peak = 0

    def reserve(key):
        nonlocal calls, allocated, failures, peak
        calls += 1
        if calls == fail_at:
            failures += 1
            return False
        if key not in live:
            live.add(key)
            allocated += 1
            peak = max(peak, len(live))
        return True

    def free(key):
        nonlocal frees
        if key in live:
            live.remove(key)
            frees += 1

    def prepare(kind, start):
        values = []
        if count and not reserve((kind, "store")):
            trace.append("F")
            return values, False
        for index in range(count):
            # The marker is constructed before its ByteBuffer preparation.
            if not reserve((kind, index)):
                trace.extend(("F", chr(start + index)))
                return values, False
            values.append((index, start + index))
        return values, True

    def drop(kind, values):
        for index, code in values:
            trace.append(chr(code))
            free((kind, index))
        free((kind, "store"))

    def result(code):
        assert not live, live
        assert allocated == frees
        return {"fail_at": fail_at, "exit": code, "stdout": "".join(trace),
                "allocation": {"alloc_calls": calls, "allocated": allocated,
                               "frees": frees, "failures": failures, "live": 0,
                               "peak_live": peak}}

    if count is None:
        return result(9)
    nodes, ready = prepare("nodes", 65)
    if not ready:
        drop("nodes", nodes)
        return result(1)
    subscriptions, ready = prepare("subscriptions", 97)
    if not ready:
        drop("subscriptions", subscriptions)
        drop("nodes", nodes)
        return result(1)

    def finish(code):
        trace.extend(("L", "Q"))
        drop("subscriptions", subscriptions)
        drop("nodes", nodes)
        return result(code)

    trace.append("S")
    if mode == 1:
        trace.append("E")
        return finish(0)
    if mode == 2:
        trace.append("F")
        return finish(1)
    if mode == 3:
        if not reserve(("nodes", "store")):
            trace.append("F")
            return finish(1)
        trace.append("ixixix")
    if mode == 4:
        replacement, ready = prepare("replacement", 109)
        if not ready:
            drop("replacement", replacement)
            return finish(1)
        drop("subscriptions", subscriptions)
        # Preserve the distinct instance while reusing the shutdown function.
        for index, _ in replacement:
            live.remove(("replacement", index))
            live.add(("subscriptions", index))
        if ("replacement", "store") in live:
            live.remove(("replacement", "store"))
            live.add(("subscriptions", "store"))
        subscriptions = replacement
        trace.append("R")
    trace.append("N")
    return finish(0)


def oracle():
    rows = []
    for count in range(4):
        for mode in range(5):
            normal = expected(count, mode)
            calls = normal["allocation"]["alloc_calls"]
            rows.append({"name": f"count-{count}-mode-{mode}", "input": f"{count}{mode}",
                         "ordinary": normal,
                         "probes": [expected(count, mode, fail) for fail in range(calls + 2)]})
    for name, data in (("eof", ""), ("short-input", "2"), ("bad-count", "90"), ("bad-mode", "29")):
        rows.append({"name": name, "input": data, "ordinary": expected(None, None),
                     "probes": [expected(None, None, fail) for fail in (0, 1)]})
    return {"schema_version": 1, "scope": "236 ordinary synchronous owning consumer",
            "ordinary_per_optimization": len(rows),
            "probes_per_optimization": sum(len(row["probes"]) for row in rows),
            "rows": rows}


if __name__ == "__main__":
    print(json.dumps(oracle(), indent=2) + "\n", end="")
