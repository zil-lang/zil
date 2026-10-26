#!/usr/bin/env python3
"""Predeclared exact source edits; never learn expected results from ZIL."""
from pathlib import Path
import json

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "tests/perf/impl084"
OUT.mkdir(parents=True, exist_ok=True)
cases = []

def save(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)

def add(name, root, private, public, expected, modes=("verify",), capacity=1024, stdin=""):
    edits = {}
    for scenario, changes in [("private_edit",private),("public_edit",public)]:
        edits[scenario] = []
        for path, text in sorted(changes.items()):
            relative = f"tests/perf/impl084/edits/{name}/{scenario}/{path}"
            save(ROOT/relative, text)
            edits[scenario].append(dict(target=str(Path(root).parent/path), replacement=relative))
    cases.append(dict(name=name, root=root, edits=edits, modes=list(modes), capacity=capacity,
                      stdin=stdin, expected={s:dict(exit=e,stdout=o,stderr="") for s,(e,o) in zip(
                          ["cold","unchanged","private_edit","public_edit"],expected)}))

small = "fn hidden() -> i32 => 7\npub fn value() -> i32 => hidden()\nfn main() -> i32 => value() - 7\n"
save(OUT/"scalar/main.zil", small)
private = small.replace("=> 7", "=> 8")
public = private.replace("pub fn value() -> i32 => hidden()", "pub fn value(extra: i32) -> i32 => hidden() + extra").replace("=> value() -", "=> value(0) -")
add("scalar", "tests/perf/impl084/scalar/main.zil", {"main.zil":private}, {"main.zil":public}, [(0,"")]*2+[(1,"")]*2)

# Flat import graphs have real Parse queries for every module and complete
# whole-project front-end/Verify work. No direct Database calls in this corpus.
for n in [16,64,128,512]:
    folder = OUT/f"modules{n}"
    main = "".join(f"import m{i}\n" for i in range(n))
    main += "fn main() -> i32 {\nvar sum = 0\n" + "".join(f"sum += m{i}.value()\n" for i in range(n)) + f"return sum - {n}\n}}\n"
    module = "fn hidden() -> i32 => 1\npub fn value() -> i32 => hidden()\n"
    save(folder/"main.zil", main)
    for i in range(n): save(folder/f"m{i}.zil", module)
    private = module.replace("=> 1", "=> 2")
    public = private.replace("pub fn value() -> i32 => hidden()", "pub fn value(extra: i32) -> i32 => hidden() + extra")
    for suffix, capacity in [("",1024)] + ([("-default-capacity",32)] if n==64 else []):
        add(f"modules{n}{suffix}", f"tests/perf/impl084/modules{n}/main.zil", {"m0.zil":private},
            {"m0.zil":public,"main.zil":main.replace("m0.value()","m0.value(0)")}, [(0,"")]*2+[(1,"")]*2, capacity=capacity)

def add_argument(source, callee):
    # Insert before the matching close, retaining nested callback expressions.
    result=[]; start=0
    while (at:=source.find(callee+"(",start)) >= 0:
        end=at+len(callee)+1; depth=1
        while depth:
            if source[end]=="(": depth+=1
            if source[end]==")": depth-=1
            end+=1
        result.append(source[start:end-1]+", 0)")
        start=end
    return "".join(result)+source[start:]

for name in ["owned_component","helper_ui"]:
    folder=ROOT/"examples/mainline"/name
    io=(folder/"io.zil").read_text()
    private=io.replace("emit(48 + digit)","emit(49 + digit)")
    assert private != io
    public=private.replace("pub fn number(value: i32)","pub fn number(value: i32, offset: i32)").replace("    digits(value)\n", "    digits(value + offset)\n")
    changes={"io.zil":public}
    for file in folder.glob("*.zil"):
        text=file.read_text(); edited=add_argument(text,"io.number")
        if text!=edited: changes[file.name]=edited
    add(name,f"examples/mainline/{name}/main.zil",{"io.zil":private},changes,
        [(0,"0\n1\n2\n#3\nMSU")]*2+[(0,"1\n2\n3\n#4\nMSU")]*2,
        modes=("verify","build"),stdin="++q")

folder=ROOT/"examples/mainline/byte_io"
original=(folder/"arithmetic.zil").read_text()
private=original.replace("if a < b { total += 1 }","if a < b { total += 2 }")
assert private != original
io=(folder/"io.zil").read_text()
public_io=io.replace("pub fn number(value: i64) { digits(value);", "pub fn number(value: i64, offset: i64) { digits(value + offset);")
assert io!=public_io
add("byte_arithmetic","examples/mainline/byte_io/arithmetic.zil",{"arithmetic.zil":private},
    {"arithmetic.zil":add_argument(private,"io.number"),"io.zil":public_io},
    [(0,"73740310\n")]*2+[(0,"73772950\n")]*2,modes=("verify","build"))
save(OUT/"cases.json",json.dumps(cases,indent=2)+"\n")
print(json.dumps(dict(cases=len(cases),mode_cells=sum(len(c["modes"]) for c in cases),modules=[16,64,128,512])))
