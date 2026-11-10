#!/usr/bin/env python3
from pathlib import Path
import hashlib

def edit(path, expected, replacements):
    p=Path(path); raw=p.read_bytes()
    assert hashlib.sha1(b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest()==expected,path
    s=raw.decode()
    for old,new in replacements:
        assert s.count(old)==1,(path,old,s.count(old))
        s=s.replace(old,new,1)
    p.write_text(s)

edit('crates/zil-backend-llvm/src/lib.rs','701e47530843c4e863bcafa1a74218829702c109',[
    ('mod machine_cgu;', 'mod machine_cgu;\nmod machine_closure_linkage;'),
])
edit('crates/zil-backend-llvm/src/build.rs','9b1c89575e68257c8eaf6239ebeb2ce0e76667f7',[
    ('    let mut cache_summary = ObjectCacheSummary {', '''    // Anonymous closure bodies are not named source exports. Decide only after
    // partition so a cross-unit caller/address retains its canonical symbol.
    let private_closures = crate::machine_closure_linkage::internalize(&mut plan.units, lir.closures);
    let mut cache_summary = ObjectCacheSummary {'''),
    ('''        // Keep one clang invocation, but an explicit closed build must consume
        // the transformed unit rather than the public diagnostic sidecar.
        let input = if options.closed_executable {''', '''        // Keep one clang invocation, but linkage-transformed builds must consume
        // the actual unit rather than the public pre-partition diagnostic text.
        let input = if options.closed_executable || private_closures != 0 {'''),
    ('io_error("write closed codegen unit", &input, e)', 'io_error("write transformed codegen unit", &input, e)'),
])
# Align tests with the current private machine model; no production assumptions.
p=Path('crates/zil-backend-llvm/src/machine_closure_linkage.rs');s=p.read_text()
assert s.count('align:8,')==2
s=s.replace('align:8,','unnamed_addr:false, alignment:8,')
p.write_text(s)
print('Post-partition anonymous linkage connected; no source export or inline/safety policy changed')
