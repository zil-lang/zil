#!/usr/bin/env python3
"""Research only. Preserve the pinned default pipeline, insert equivalence once.
The experimental selector is frozen per process and included in the cache key.
No generated probe module is eligible for production extraction.
"""
from pathlib import Path
import os,subprocess,json,hashlib
root=Path('.task/merge-order');opt=Path(os.environ['ZIL_CLANG']).with_name('opt')
empty=root/'empty.ll';empty.write_text('target triple = "x86_64-unknown-linux-gnu"\ndefine i32 @main(){ret i32 0}\n')
args=[str(opt),'-passes=default<O2>','-enable-merge-functions','-print-pipeline-passes','-disable-output',str(empty)]
r=subprocess.run(args,capture_output=True,text=True,check=True)
(root/'default-pipeline.stdout').write_text(r.stdout);(root/'default-pipeline.stderr').write_text(r.stderr)
pipeline=r.stdout.strip();assert '\n' not in pipeline and 'inline' in pipeline and 'mergefunc' in pipeline,pipeline
# Angle brackets may contain pass parameters, including commas; respect them.
def split(s):
    result=[];start=0;depth=0;angle=0
    for i,ch in enumerate(s):
        if ch=='(':depth+=1
        elif ch==')':depth-=1
        elif ch=='<':angle+=1
        elif ch=='>':angle-=1
        elif ch==',' and depth==0 and angle==0:result.append(s[start:i]);start=i+1
    result.append(s[start:]);assert depth==0 and angle==0;return result
parts=split(pipeline)
# opt prints an implicit module wrapper on some supported point releases.
wrapped=len(parts)==1 and parts[0].startswith('module(') and parts[0].endswith(')')
if wrapped:parts=split(parts[0][7:-1])
inline=[i for i,p in enumerate(parts) if ('cgscc(' in p or 'inline(' in p or p.startswith('module-inline')) and 'inline' in p]
assert inline,parts
idx=inline[0]
def inject(pos):
    text=','.join(parts[:pos]+['mergefunc']+parts[pos:])
    return 'module('+text+')' if wrapped else text
variants={
 'raw':'mergefunc,default<O2>',
 'normalize':'function(sroa,early-cse,instcombine<max-iterations=1;no-verify-fixpoint>,simplifycfg),mergefunc,default<O2>',
 'before-inline':inject(idx),
 'after-inline':inject(idx+1),
 'after-cleanup':inject(next(i+1 for i in range(idx+1,len(parts)) if parts[i].startswith('function'))),
 'default-expanded':pipeline,
}
# Parse and execute every candidate on a real, tiny valid module first.
for name,value in variants.items():
    q=subprocess.run([str(opt),'-passes='+value,'-enable-merge-functions','-disable-output',str(empty)],capture_output=True,text=True)
    (root/(name+'.parse.stderr')).write_text(q.stderr);assert q.returncode==0,(name,q.stderr)
(root/'pipelines.json').write_text(json.dumps({'opt':str(opt),'default_argv':args,'default_pipeline':pipeline,'top_level_parts':parts,'inliner_index':idx,'variants':variants},indent=2)+'\n')
rows=[]
for name,value in variants.items():
    identity='research-position-v1|'+value+'|merge-functions=after|aliases=false'
    rows.append(f'        {json.dumps(name)} => (c{json.dumps(value)}, {json.dumps(identity)}),')
source='''// Research-only exact pass placement probe; never a production runtime option.
pub(crate) fn pipeline() -> (&'static std::ffi::CStr, &'static str) {
    static VALUE: std::sync::OnceLock<(&'static std::ffi::CStr, &'static str)> = std::sync::OnceLock::new();
    *VALUE.get_or_init(|| match std::env::var("ZIL_RESEARCH_PASS_POSITION").unwrap_or_else(|_| "raw".into()).as_str() {
'''+ '\n'.join(rows)+'''
        other => panic!("unrecognized research pass position: {other}"),
    })
}
'''
p=Path('crates/zil-llvm-api/src/research_position.rs');assert not p.exists();p.write_text(source)
p=Path('crates/zil-llvm-api/src/lib.rs');s=p.read_text();assert s.count('pub const fn pipeline_identity(')==1
s=s.replace('pub const fn pipeline_identity(', 'pub fn pipeline_identity(',1)
a='''        (Optimization::O2, FunctionMerging::BeforeAndAfter) => {
            "mergefunc,default<O2>|merge-functions=after|aliases=false"
        }'''
assert s.count(a)==1;s=s.replace(a,'''        (Optimization::O2, FunctionMerging::BeforeAndAfter) => research_position::pipeline().1,''')
s=s.replace('pub mod model;','pub mod model;\nmod research_position;',1);p.write_text(s)
p=Path('crates/zil-llvm-api/src/native.rs');s=p.read_text();a='(true, FunctionMerging::BeforeAndAfter) => c"mergefunc,default<O2>",';assert s.count(a)==1;s=s.replace(a,'(true, FunctionMerging::BeforeAndAfter) => super::research_position::pipeline().0,');p.write_text(s)
print(json.dumps({'positions':list(variants),'default_parts':len(parts),'inline_index':idx,'production_modified':False,'research_adapter_modified':True}))
