#!/usr/bin/env python3
"""Research only: keep the full original O2 pipeline after a cheap inline sweep.
This does not replace any mandatory verifier or declare functions equivalent itself.
"""
from pathlib import Path
import json,subprocess,os
root=Path('.task/merge-order');p=json.loads((root/'pipelines.json').read_text());parts=p['top_level_parts'];i=p['inliner_index'];assert parts[i].startswith('cgscc(')
variants={'raw':'mergefunc,default<O2>','default-expanded':p['default_pipeline']}
for name,first in [
 ('inline-first','cgscc(inline)'),
 ('inline-attrs','cgscc(inline,function-attrs)'),
 ('inline-clean','cgscc(inline,function(sroa,early-cse<memssa>,instcombine<max-iterations=1;no-verify-fixpoint>))'),
]:
    variants[name]=','.join(parts[:i]+[first,'mergefunc']+parts[i:])
opt=Path(os.environ['ZIL_CLANG']).with_name('opt');empty=root/'empty.ll'
for name,text in variants.items():
    result=subprocess.run([str(opt),'-passes='+text,'-enable-merge-functions','-disable-output',str(empty)],capture_output=True,text=True)
    (root/(name+'.parse.stderr')).write_text(result.stderr);assert result.returncode==0,(name,result.stderr)
rows=[]
for name,value in variants.items():rows.append(f'        {json.dumps(name)} => (c{json.dumps(value)}, {json.dumps("research-inline-equivalence-v1|"+value+"|merge-functions=after|aliases=false")}),')
source='''// Research-only fixed pipeline choices. Never extract this selector into main.
pub(crate) fn pipeline() -> (&'static std::ffi::CStr, &'static str) {
    static VALUE: std::sync::OnceLock<(&'static std::ffi::CStr, &'static str)> = std::sync::OnceLock::new();
    *VALUE.get_or_init(|| match std::env::var("ZIL_RESEARCH_PASS_POSITION").unwrap_or_else(|_| "raw".into()).as_str() {
'''+ '\n'.join(rows)+'''
        other => panic!("unknown inline-equivalence research choice: {other}"),
    })
}
'''
Path('crates/zil-llvm-api/src/research_position.rs').write_text(source)
s=(root/'screen_positions.py').read_text();old="ARMS=['base','raw','normalize','before-inline','after-inline','after-cleanup','default-expanded']";assert s.count(old)==1
s=s.replace(old,"ARMS=['base','raw','inline-first','inline-attrs','inline-clean','default-expanded']")
s=s.replace('len(rows)==224 and len(runtime)==896','len(rows)==192 and len(runtime)==768')
(root/'screen_positions.py').write_text(s)
p['variants']=variants;p['policy']='extra cheap inline sweep followed by equivalence and the complete original optimization pipeline; no removed original pass'
(root/'pipelines.json').write_text(json.dumps(p,indent=2)+'\n')
print(json.dumps({'variants':list(variants),'original_tail_retained':True,'main_changed':False}))
