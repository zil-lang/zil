#!/usr/bin/env python3
from pathlib import Path
import json,subprocess,os
root=Path('.task/merge-order');p=json.loads((root/'pipelines.json').read_text());parts=p['top_level_parts'];i=p['inliner_index']
variants={
 'inline-first':p['variants']['inline-first'],
 'minimal-prefix':'function(sroa,early-cse),cgscc(inline),mergefunc,default<O2>',
 'normalized-prefix':'function(sroa,early-cse),ipsccp,globalopt,function(mem2reg,instcombine<max-iterations=1;no-verify-fixpoint>,simplifycfg),cgscc(inline),mergefunc,default<O2>',
 'full-prefix':','.join(parts[:i]+['cgscc(inline)','mergefunc','default<O2>']),
}
opt=Path(os.environ['ZIL_CLANG']).with_name('opt')
for name,text in variants.items():
 r=subprocess.run([str(opt),'-passes='+text,'-enable-merge-functions','-disable-output',str(root/'empty.ll')],capture_output=True,text=True)
 (root/(name+'.parse.stderr')).write_text(r.stderr);assert r.returncode==0,(name,r.stderr)
rows=[f'        {json.dumps(name)} => (c{json.dumps(value)}, {json.dumps("research-inline-prefix-v1|"+value+"|merge-functions=after|aliases=false")}), ' for name,value in variants.items()]
source='''// Research-only selection. Production must use a fixed documented policy.
pub(crate) fn pipeline() -> (&'static std::ffi::CStr, &'static str) {
    static VALUE: std::sync::OnceLock<(&'static std::ffi::CStr, &'static str)> = std::sync::OnceLock::new();
    *VALUE.get_or_init(|| match std::env::var("ZIL_RESEARCH_PASS_POSITION").unwrap_or_else(|_| "minimal-prefix".into()).as_str() {
'''+ '\n'.join(rows)+'''
        other => panic!("unknown inline-prefix choice: {other}"),
    })
}
'''
Path('crates/zil-llvm-api/src/research_position.rs').write_text(source)
s=(root/'screen_positions.py').read_text();old="ARMS=['base','raw','inline-first','inline-attrs','inline-clean','default-expanded']";assert s.count(old)==1
s=s.replace(old,"ARMS=['base','inline-first','minimal-prefix','normalized-prefix','full-prefix']")
s=s.replace("arm if arm!='base' else 'raw'","arm if arm!='base' else 'minimal-prefix'")
s=s.replace('len(rows)==192 and len(runtime)==768','len(rows)==160 and len(runtime)==640')
(root/'screen_positions.py').write_text(s)
p['variants']=variants;p['scope']='Short target-neutral prefix followed by original target-specific default<O2>; previous expanded-inline pipeline retained only as a reference arm'
(root/'pipelines.json').write_text(json.dumps(p,indent=2)+'\n');print(json.dumps({'variants':list(variants)}))
