#!/usr/bin/env python3
"""Deterministic committed source controls, independent of candidate results."""
from pathlib import Path
import json

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "tests/perf/impl080"
OUT.mkdir(parents=True, exist_ok=True)
cases = []

def add(name, source, output="", exit_code=0):
    folder = OUT / name
    folder.mkdir(exist_ok=True)
    (folder / "main.zil").write_text(source)
    cases.append(dict(name=name, root=f"tests/perf/impl080/{name}/main.zil",
                      edit=f"tests/perf/impl080/{name}/main.zil", modes=["verify"],
                      stdin="", stdout=output, exit=exit_code))

def graph_source(edges, writer, entry=0):
    lines = ["resource Token { n: i32 }"]
    for i, neighbors in enumerate(edges):
        body = ["r.n = 1"] if i in writer else []
        body += [f"f{j}(r)" for j in neighbors]
        # SCC examples terminate dynamically on a flag, while their structural
        # dependency graph remains cyclic. No recursive execution is benchmarked.
        if any(j <= i for j in neighbors) and i in writer:
            body = ["if r.n == 0 { " + "; ".join(body) + " }"]
        lines.append(f"fn f{i}(r: Token) {{ " + "; ".join(body) + " }")
    lines.append(f"fn main() -> i32 {{ let r = Token {{ n: 0 }}; f{entry}(r); return r.n - 1 }}")
    return "\n".join(lines) + "\n"

for n in [4, 64, 128]:
    add(f"reverse{n}", graph_source([[i+1] if i+1<n else [] for i in range(n)], {n-1}))
add("forward64", graph_source([[i-1] if i else [] for i in range(64)], {0}, 63))
add("star64", graph_source([list(range(1,64))] + [[] for _ in range(63)], set(range(1,64))))
edges = [[i+1] if i%8 != 7 else [] for i in range(64)]
add("disconnected64", graph_source(edges, set(range(7,64,8))))
add("cycle16", graph_source([[(i+1)%16] for i in range(16)], {15}))
add("no_resource", "fn main() -> i32 => 0\n")
add("unused_resource", "resource Unused { n: i32 }\nfn main() -> i32 {\n" + "\n".join(f"let x{i} = {i}" for i in range(64)) + "\nreturn x63 - 63\n}\n")
add("borrowed_closure", """resource Token { n: i32 }
fn write(r: Token) { r.n += 1 }
fn forward(r: Token) { write(r) }
fn main() -> i32 {
    let r = Token { n: 0 }
    let callback = () => { forward(r); r.n }
    return callback() + callback() - 3
}
""")
for name, root, edit, stdin, stdout in [
    ("owned-main", "main.zil", "window.zil", "+r-q", "0\n1\nS1\n0\n#4\nMSU"),
    ("owned-lifecycle", "lifecycle.zil", "lifecycle.zil", "7", "ABC68DEF71GBCZ"),
    ("owned-fd", "fd.zil", "fd.zil", "", "F"),
]:
    cases.append(dict(name=name, root=f"examples/mainline/owned_component/{root}",
                      edit=f"examples/mainline/owned_component/{edit}",
                      modes=["verify", "build"], stdin=stdin, stdout=stdout, exit=0))
cases.append(dict(name="public-helper", root="examples/mainline/helper_ui/main.zil",
                  edit="examples/mainline/helper_ui/helpers.zil", modes=["verify"],
                  stdin="++q", stdout="0\n1\n2\n#3\nMSU", exit=0))
(OUT / "cases.json").write_text(json.dumps(cases, indent=2) + "\n")
