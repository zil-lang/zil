"""Unchanged nineteen historical cells plus predeclared 123 CFG growth families."""
from support import BASE, extract

ORDINARY = ["cold", "unchanged", "private_edit", "public_edit"]
USED_BODY = ["cold", "unchanged", "body_edit", "body_restore"]

def historical_inputs(repo, out):
    root = out / "inputs"; root.mkdir()
    extract(repo, BASE, ["examples/mainline/owned_component", "examples/mainline/byte_buffer", "examples/mainline/byte_buffer_owned"], root)
    cases=[]
    for name, n in [("tiny",0), ("modules512",512)]:
        folder=root/name;folder.mkdir()
        text="".join(f"import m{i}\n" for i in range(n))
        text += "fn main()->i32=>"+("m0.value()" if n else "0")+"\n"
        (folder/"main.zil").write_text(text)
        for i in range(n): (folder/f"m{i}.zil").write_text("pub fn value()->i32=>0\n")
        cases.append(dict(name=name,root=f"{name}/main.zil",edit=f"{name}/m0.zil" if n else f"{name}/main.zil",
                          modes=["verify"],capacity=32 if n==0 else 1024,stdin="",stdout=""))
    for reverse in [False,True]:
        name="reverse128" if reverse else "forward128"
        folder=root/name;folder.mkdir();text="resource Token{n:i32}\n"
        for i in (range(128) if reverse else reversed(range(128))):
            body=f"f{i+1}(r)" if i+1<128 else "r.n+=1"
            text+=f"fn f{i}(r:Token){{{body}}}\n"
        text+="fn main()->i32{let r=Token{n:0};let c=()=>{f0(r);r.n};return c()+c()-3;}\n"
        (folder/"main.zil").write_text(text)
        cases.append(dict(name=name,root=f"{name}/main.zil",edit=f"{name}/main.zil",modes=["verify"],capacity=1024,stdin="",stdout=""))
    for name,directory,edit,stdin,stdout in [
        ("owned-main","owned_component","window.zil","+r-q","0\n1\nS1\n0\n#4\nMSU"),
        ("bytes-main","byte_buffer","buffer.zil","ab\x00z\n","AB\x00Z\n"),
        ("owned-bytes-main","byte_buffer_owned","state.zil","\x01"*20+"\x00"+"\x02"*4,"".join(chr(2*i) for i in range(1,21))+"\x00"+"".join(chr(3*i) for i in range(1,5))),
    ]:
        cases.append(dict(name=name,root=f"examples/mainline/{directory}/main.zil",edit=f"examples/mainline/{directory}/{edit}",
                          modes=["verify","build"],capacity=1024,stdin=stdin,stdout=stdout))
    for case in cases:
        target=root/case["edit"];original=target.read_text()
        folder=out/"edit-manifests"/case["name"];folder.mkdir(parents=True)
        private=original+"\nfn research100_private()->i32=>17\n"
        for scenario,text in [("private_edit",private),("public_edit",private+"\npub fn research100_public()->i32=>29\n")]:
            replacement=folder/f"{scenario}.zil";replacement.write_text(text)
            (folder/f"{scenario}.tsv").write_text(f"{target}\t{replacement}\n")
    assert sum(len(c["modes"]) for c in cases)==10
    return cases

def manifest(out, case, scenario, replacement):
    folder = out / "edit-manifests" / case["name"]
    folder.mkdir(parents=True, exist_ok=True)
    path = folder / f"{scenario}.zil"
    path.write_text(replacement)
    (folder / f"{scenario}.tsv").write_text(f"{out / 'inputs' / case['edit']}\t{path}\n")


def historical_nineteen(repo, out):
    cases = historical_inputs(repo, out)
    for case in cases:
        case["scenarios"] = ORDINARY
    for name in ["swap16", "alias-cycle16", "fanjoin16"]:
        text = "resource Token{n:i32}\n"
        for i in range(16):
            if name == "swap16":
                body = f"f{i+1}(b,a)" if i < 15 else "a.n+=1"
                text += f"fn f{i}(a:Token,b:Token){{{body}}}\n"
            elif name == "alias-cycle16":
                body = "let alias=a;for step in 0..1 {if n>0 {" + f"f{(i+1)%16}(b,alias,n-1)" + "}}"
                if i == 15:
                    body += ";a.n+=1"
                text += f"fn f{i}(a:Token,b:Token,n:i32){{{body}}}\n"
            else:
                body = f"f{i+1}(a,b);f{i+2}(b,a)" if i < 14 else ("f15(a,b)" if i == 14 else "a.n+=1")
                text += f"fn f{i}(a:Token,b:Token){{{body}}}\n"
        args = "a,b,0" if name == "alias-cycle16" else "a,b"
        text += "fn main()->i32{let a=Token{n:0};let b=Token{n:0};let cb=()=>{" + f"f0({args});a.n+b.n" + "};return cb()+cb()-3;}\n"
        folder = out / "inputs" / name
        folder.mkdir()
        (folder / "main.zil").write_text(text)
        case = dict(name=name, root=f"{name}/main.zil", edit=f"{name}/main.zil", modes=["verify"],
                    capacity=1024, stdin="", stdout="", scenarios=ORDINARY)
        private = text + "\nfn research101_private()->i32=>17\n"
        manifest(out, case, "private_edit", private)
        manifest(out, case, "public_edit", private + "\npub fn research101_public()->i32=>29\n")
        cases.append(case)
    changes = {
        "owned-main": ("state.count += 1", "state.count += 2", "0\n2\nS2\n1\n#4\nMSU"),
        "bytes-main": ("var next: i64 = 8", "var next: i64 = 16", "AB\x00Z\n"),
        "owned-bytes-main": ("state.sum += i64(byte)", "state.sum += i64(byte) * 2",
                             "".join(chr(3*i) for i in range(1,21)) + "\x00" + "".join(chr(5*i) for i in range(1,5))),
    }
    for parent in list(cases):
        if parent["name"] not in changes:
            continue
        case = dict(parent, name=parent["name"]+"-used", scenarios=USED_BODY)
        before, after, expected = changes[parent["name"]]
        original = (out / "inputs" / case["edit"]).read_text()
        assert original.count(before) == 1, (parent["name"], before)
        case["stdout_by_scenario"] = {s: expected if s == "body_edit" else parent["stdout"] for s in USED_BODY}
        case["actual_used_edit"] = dict(before=before, after=after)
        manifest(out, case, "body_edit", original.replace(before, after))
        manifest(out, case, "body_restore", original)
        cases.append(case)
    for case in cases:
        (out / "edit-manifests" / case["name"] / "scenarios.txt").write_text("\n".join(case["scenarios"]) + "\n")
    assert sum(len(c["modes"]) for c in cases) == 19
    return cases



def growth_source(family, n):
    nested = "s+=1;"
    for _ in range(n):
        nested = "if e>=0 {" + nested + "}else{s-=1;}"
    diamonds = "if e>=0{s+=1;}else{s-=1;}" * n
    if family == "nested-if":
        body, expected = nested, 1
    elif family == "sequential-diamonds":
        body, expected = diamonds, n
    elif family == "loop-body":
        body, expected = "for turn in 0..2 {" + nested + "}", 2
    else:
        raise ValueError(family)
    return "fn event(e:i32)->i32{var s:i32=0;" + body + "return s;}\nfn main()->i32=>event(1)-" + str(expected) + "\n"


def inputs(repo, out):
    cases = historical_nineteen(repo, out)
    for case in cases:
        case["family"] = "historical"
    for family in ["nested-if", "sequential-diamonds", "loop-body"]:
        for n in [16, 32, 64]:
            name = f"{family}{n}"
            folder = out / "inputs" / name
            folder.mkdir()
            source = growth_source(family, n)
            (folder / "main.zil").write_text(source)
            case = dict(name=name, root=f"{name}/main.zil", edit=f"{name}/main.zil", modes=["verify"],
                        capacity=1024, stdin="", stdout="", scenarios=ORDINARY, family=family, n=n)
            private = source + "\nfn research123_private()->i32=>17\n"
            manifest(out, case, "private_edit", private)
            manifest(out, case, "public_edit", private + "\npub fn research123_public()->i32=>29\n")
            (out / "edit-manifests" / name / "scenarios.txt").write_text("\n".join(ORDINARY) + "\n")
            cases.append(case)
    assert sum(len(c["modes"]) for c in cases) == 28
    return cases
