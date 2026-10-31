"""Usage: python3 compile7_scalar.py REPOSITORY OUTPUT [32,256,1024]."""
import importlib.util, shutil, sys
from pathlib import Path
repo, out = map(Path, sys.argv[1:3])
spec = importlib.util.spec_from_file_location("corpus210", repo / "tools/research/210/corpus.py")
corpus = importlib.util.module_from_spec(spec)
spec.loader.exec_module(corpus)
for size in map(int, (sys.argv[3] if len(sys.argv) > 3 else "32,256,1024").split(",")):
    root = out / f"scalar{size}x8"
    base, private, public = (root / name / "zil" for name in ("base", "private", "public"))
    corpus.tree_sources(base, "zil", size, 8)
    (base / "io.zil").unlink()
    (base / "main.zil").write_text("import effect\nimport m0\nfn main()->i32 { return i32(m0.run(effect.adjust(i64(1)))); }\n")
    shutil.copytree(base, private)
    corpus.replace_once(private / "effect.zil", "y+i+7", "y+i+11")
    shutil.copytree(private, public)
    corpus.replace_once(public / "effect.zil", "adjust(x:i64)", "adjust(x:i64,extra:i64)")
    corpus.replace_once(public / "effect.zil", "y+i+11", "y+i+11+extra")
    corpus.replace_once(public / "main.zil", "effect.adjust(i64(1))", "effect.adjust(i64(1),3)")
