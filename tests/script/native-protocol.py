#!/usr/bin/env python3
"""Native scalar run/CLI protocol sub-evidence; not resource/import acceptance."""
import json, os, pathlib, subprocess, tempfile
ROOT=pathlib.Path(__file__).resolve().parents[2]
TARGET=pathlib.Path(os.environ.get('CARGO_TARGET_DIR',ROOT/'target'))
if not TARGET.is_absolute(): TARGET=ROOT/TARGET
ZIL=pathlib.Path(os.environ.get('ZIL_BIN',TARGET/'debug/zil')).resolve()
executed=assertions=0
with tempfile.TemporaryDirectory(prefix='zil-native-protocol-') as temporary:
    root=pathlib.Path(temporary);source=root/'program.zil'
    source.write_text('fn main() -> i32 { return 7 }\nfn wide() -> i64 { return 1099511628800 }\n')
    def run(*arguments):
        global executed
        p=subprocess.run([str(ZIL),*map(str,arguments),'--json'],cwd=ROOT,text=True,capture_output=True,timeout=30)
        executed+=1
        return p,json.loads(p.stdout)
    output=root/'compiled'
    p,build=run('build',source,'--output',output);assert p.returncode==0 and build['status']=='Success';assertions+=1
    direct=subprocess.run([str(output)],capture_output=True,text=True,timeout=10);executed+=1
    p,wrapped=run('run',source)
    assert p.returncode==direct.returncode==wrapped['run']['exit_code']==7;assertions+=1
    assert wrapped['run']['stdout']==direct.stdout=='';assertions+=1
    p,wide=run('run',source,'--entry','wide')
    assert p.returncode==0 and wide['run']['stdout']=='1099511628800\n' and wide['run']['exit_code']==0;assertions+=1
    p,managed=run('check',source,'--profile','managed')
    assert p.returncode==3 and managed['status']=='Unsupported';assertions+=1
    p,cancelled=run('build',source,'--output',root/'cancelled','--compile-timeout-ms','0')
    assert p.returncode==5 and cancelled['status']=='Cancelled' and not (root/'cancelled').exists();assertions+=1
    p,missing=run('build',source,'--output',root/'missing','--clang',root/'does-not-exist')
    assert p.returncode==2 and missing['status']=='InfraError' and not (root/'missing').exists();assertions+=1
print(json.dumps({'executed':executed,'assertions':assertions,'scope':'native scalar CLI protocol only; import/cleanup remains pending'}))
