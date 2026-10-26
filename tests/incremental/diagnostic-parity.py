#!/usr/bin/env python3
"""Compare actual CLI and editor-adapter processes, not copied diagnostic oracles."""
import json, os, pathlib, subprocess, tempfile
ROOT=pathlib.Path(__file__).resolve().parents[2]
TARGET=pathlib.Path(os.environ.get('CARGO_TARGET_DIR',ROOT/'target'))
if not TARGET.is_absolute(): TARGET=ROOT/TARGET
ZIL=pathlib.Path(os.environ.get('ZIL_BIN',TARGET/'debug/zil')).resolve()
build=subprocess.run(['cargo','build','--locked','--offline','-p','zil-ide','--example','diagnostics'],cwd=ROOT,text=True,capture_output=True)
if build.returncode: raise RuntimeError(build.stdout+'\n'+build.stderr)
IDE=TARGET/'debug/examples/diagnostics'
cases=[
 ('valid','fn main() -> i32 { return 0 }','Success'),
 ('entry','fn main() -> bool => true','SafeReject'),
 ('duplicate','fn main() -> i32 { return 0 }\nfn main() -> i32 { return 1 }','SafeReject'),
 ('type','fn main() -> i32 { return true }','SafeReject'),
 ('name','fn main() -> i32 { return missing }','SafeReject'),
 ('syntax','let x = 1','SafeReject'),
 ('ct','fn check() -> i64 { return #(1 / 0) }','SafeReject'),
 ('unsupported','fn main() -> i32 { var value = 1; let p = &mut value; let pp = &p; return **pp; }','Unsupported'),
]
executed=assertions=0
with tempfile.TemporaryDirectory(prefix='zil-parity-') as temporary:
    for name,text,expected in cases:
        source=pathlib.Path(temporary)/(name+'.zil');source.write_text(text)
        cli=subprocess.run([str(ZIL),'check',str(source),'--json'],cwd=ROOT,text=True,capture_output=True,timeout=30)
        ide=subprocess.run([str(IDE),str(source)],cwd=ROOT,text=True,capture_output=True,timeout=30)
        a=json.loads(cli.stdout);b=json.loads(ide.stdout);executed+=2
        assert a['status']==b['status']==expected,(name,a,b);assertions+=1
        assert a['phase']==b['phase'],(name,a,b);assertions+=1
        assert a['diagnostics']==b['diagnostics'],(name,a,b);assertions+=1
        assert (cli.returncode==0)==(ide.returncode==0)==(expected=='Success');assertions+=1
print(json.dumps({'executed':executed,'assertions':assertions,'scope':'actual CLI/editor process diagnostic parity','cases':len(cases)}))
