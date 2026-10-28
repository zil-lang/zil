"""Paired real sources. The Python oracle is never used to compile either language."""
from pathlib import Path
import importlib.util
import shutil

ROOT = Path(__file__).resolve().parents[3]
GROUP_WEIGHTS = {"small": .05, "medium": .15, "large": .60, "consumer": .20}
REQUEST_WEIGHTS = {"cold_session": .10, "unchanged": .35,
                   "private_edit": .40, "public_edit": .15}
SCENARIOS = tuple(REQUEST_WEIGHTS)
SNAPSHOTS = {"cold_session": "base", "unchanged": "base",
             "private_edit": "private", "public_edit": "public"}
SPECS = [("tree4x2", "small", 4, 2), ("tree32x8", "medium", 32, 8),
         ("tree256x8", "large", 256, 8), ("tree1024x8", "large", 1024, 8),
         ("retained", "consumer", None, None)]
REFERENCE = ROOT / "tools/product/109/reference104"
CONSUMER = ROOT / "tools/product/108/programs104/retained"


def reference():
    spec = importlib.util.spec_from_file_location("parity104_oracle", REFERENCE / "run.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def replace_once(path, before, after):
    text = path.read_text()
    if text.count(before) != 1:
        raise ValueError(f"edit must target exactly one used declaration: {path}: {before}")
    path.write_text(text.replace(before, after))


def tree_sources(directory, language, modules, width):
    directory.mkdir(parents=True)
    ext = "zil" if language == "zil" else "rs"
    if language == "zil":
        io = ('extern "C" fn getchar()->i32;\nextern "C" fn putchar(x:i32)->i32;\n'
              'pub fn read_byte()->i32 { unsafe { return getchar() } }\n'
              'pub fn write_byte(x:u8) { unsafe { putchar(i32(x)) } }\n')
        effect = ('pub fn adjust(x:i64)->i64 { var y=x; var i:i64=0; '
                  'while i<3 { y=(y+i+7)%251; i+=1; } return y; }\n')
        main = ('import io\nimport effect\nimport m0\nfn main()->i32 { '
                'var x=io.read_byte(); while x>=0 { '
                'let y=m0.run(effect.adjust(i64(x))); io.write_byte(u8(y)); '
                'x=io.read_byte(); } return 0; }\n')
    else:
        io = ('unsafe extern "C" { fn getchar()->i32; fn putchar(x:i32)->i32; }\n'
              'pub fn read_byte()->i32 { unsafe { getchar() } }\n'
              'pub fn write_byte(x:u8) { unsafe { putchar(i32::from(x)); } }\n')
        effect = ('pub fn adjust(x:i64)->i64 { let mut y=x; let mut i:i64=0; '
                  'while i<3 { y=(y+i+7)%251; i+=1; } y }\n')
        main = 'mod io;\nmod effect;\n' + ''.join(f'mod m{i};\n' for i in range(modules))
        main += ('fn main() { let mut x=io::read_byte(); while x>=0 { '
                 'let y=m0::run(effect::adjust(i64::from(x))); io::write_byte(y as u8); '
                 'x=io::read_byte(); } }\n')
    for name, text in (("io", io), ("effect", effect), ("main", main)):
        (directory / f"{name}.{ext}").write_text(text)
    for i in range(modules):
        children = [j for j in (2*i+1, 2*i+2) if j < modules]
        parts = [f'import m{j}\n' for j in children] if language == "zil" else []
        for k in range(width):
            mul, add = 2 + (i*width+k) % 11, 1 + (i*7+k*3) % 29
            if language == "zil":
                parts.append(f'fn f{k}(x:i64)->i64 {{ return (x*{mul}+{add})%251; }}\n')
            else:
                parts.append(f'fn f{k}(x:i64)->i64 {{ (x*{mul}+{add})%251 }}\n')
        parts.append('pub fn run(x:i64)->i64 { ' + ('var y=x; ' if language == "zil" else 'let mut y=x; '))
        parts.extend(f'y=f{k}(y); ' for k in range(width))
        sep = "." if language == "zil" else "::"
        prefix = "" if language == "zil" else "crate::"
        parts.extend(f'y={prefix}m{j}{sep}run(y); ' for j in children)
        parts.append('return y; }\n')
        (directory / f'm{i}.{ext}').write_text(''.join(parts))


def tree_expected(modules, width, snapshot, data):
    # Compose the affine maps modulo the prime 251, independently of source execution.
    order, stack = [], [0]
    while stack:
        i = stack.pop()
        order.append(i)
        stack.extend(j for j in (2*i+2, 2*i+1) if j < modules)
    scale, offset = 1, 0
    for i in order:
        for k in range(width):
            a, b = 2 + (i*width+k) % 11, 1 + (i*7+k*3) % 29
            scale, offset = scale*a % 251, (offset*a+b) % 251
    bias = 7 if snapshot == "base" else 11
    extra = 3 if snapshot == "public" else 0
    return bytes((scale*((byte + 3*bias + 3 + 3*extra) % 251)+offset) % 251 for byte in data)


def generate(out):
    oracle = reference()
    counts = {g: sum(spec[1] == g for spec in SPECS) for g in GROUP_WEIGHTS}
    cases = []
    for name, group, modules, width in SPECS:
        root = out / "corpus" / name
        for language in ("zil", "rust"):
            ext = "zil" if language == "zil" else "rs"
            base = root / "base" / language
            if modules is None:
                shutil.copytree(CONSUMER / language, base)
            else:
                tree_sources(base, language, modules, width)
            private, public = root / "private" / language, root / "public" / language
            shutil.copytree(base, private)
            if modules is None:
                replace_once(private / f"state.{ext}",
                             "state.sum += i64(byte)" if language == "zil" else "state.sum += i64::from(byte);",
                             "state.sum = state.sum + i64(byte)" if language == "zil" else "state.sum = state.sum + i64::from(byte);")
            else:
                replace_once(private / f"effect.{ext}", "y+i+7", "y+i+11")
            shutil.copytree(private, public)
            if modules is None:
                if language == "zil":
                    replace_once(public / "buffer.zil", "pub fn append(value: ByteBuffer, byte: u8)",
                                 "pub fn append(value: ByteBuffer, byte: u8, extra: u8)")
                    replace_once(public / "buffer.zil", "bytes_push(value, byte)", "bytes_push(value, byte + extra)")
                    replace_once(public / "state.zil", "buffer.append(state.data, byte)", "buffer.append(state.data, byte, 0u8)")
                else:
                    replace_once(public / "buffer.rs", "pub fn append(value: &mut Vec<u8>, byte: u8)",
                                 "pub fn append(value: &mut Vec<u8>, byte: u8, extra: u8)")
                    replace_once(public / "buffer.rs", "value.push(byte);", "value.push(byte + extra);")
                    replace_once(public / "state.rs", "buffer::append(&mut state.data, byte)",
                                 "buffer::append(&mut state.data, byte, 0)")
            else:
                replace_once(public / f"effect.{ext}", "adjust(x:i64)", "adjust(x:i64,extra:i64)")
                replace_once(public / f"effect.{ext}", "y+i+11", "y+i+11+extra")
                replace_once(public / f"main.{ext}",
                             "effect.adjust(i64(x))" if language == "zil" else "effect::adjust(i64::from(x))",
                             "effect.adjust(i64(x),3)" if language == "zil" else "effect::adjust(i64::from(x),3)")
        sizes = [0, 7, 257, 4096] if modules is None else [0, 7, 256]
        inputs = []
        for size in sizes:
            path = root / "io" / f"input-{size}.bin"
            path.parent.mkdir(exist_ok=True)
            data = oracle.input_bytes("retained", size) if modules is None else bytes((i*73+19) % 256 for i in range(size))
            path.write_bytes(data)
            expected = {}
            for snapshot in ("base", "private", "public"):
                output = oracle.model("retained", data)[0] if modules is None else tree_expected(modules, width, snapshot, data)
                dest = path.with_name(f"expected-{snapshot}-{size}.bin")
                dest.write_bytes(output)
                expected[snapshot] = str(dest.relative_to(out))
            inputs.append({"path": str(path.relative_to(out)), "expected": expected, "bytes": size})
        language_sizes = {}
        for language in ("zil", "rust"):
            files = list((root / "base" / language).glob("*"))
            language_sizes[language] = {"modules": len(files), "bytes": sum(p.stat().st_size for p in files),
                "functions": sum(p.read_text().count("fn ") for p in files)}
        cases.append({"id": name, "group": group, "weight": GROUP_WEIGHTS[group]/counts[group],
                      "kind": "existing-retained-consumer" if modules is None else "synthetic-called-module-tree",
                      "modules_requested": modules, "width": width, "source_sizes": language_sizes,
                      "inputs": inputs, "root": str(root.relative_to(out)),
                      "edit_semantics": "used-body-equivalent-rewrite; used-signature-extra-zero-and-caller-migration"
                          if modules is None else "used-loop-body-output-change; used-signature-and-caller-output-change"})
    return cases
