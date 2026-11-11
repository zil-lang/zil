#!/usr/bin/env python3
from pathlib import Path
import hashlib
OLD='mergefunc,default<O2>'
NEW='function(sroa,early-cse),ipsccp,globalopt,function(mem2reg,instcombine<max-iterations=1;no-verify-fixpoint>,simplifycfg),cgscc(inline),mergefunc,default<O2>'
for name,digest in [
 ('crates/zil-llvm-api/src/lib.rs','db5463aa7c2ff94a3c7f0893818eca77ef52b4e9'),
 ('crates/zil-llvm-api/src/native.rs','ba063ec988dcae8d0b055e27c14d7d527ef84a8b'),
 ('crates/zil-cli/tests/function_merging_research.rs','4ce408c8e549b071c07788abe2cc3fe20c071b56'),
]:
 p=Path(name);data=p.read_bytes();assert hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()==digest,name
 s=data.decode();assert s.count(OLD)==1,(name,s.count(OLD));p.write_text(s.replace(OLD,NEW))
p=Path('crates/zil-llvm-api/src/lib.rs');s=p.read_text();old='    BeforeAndAfter,';assert s.count(old)==1
s=s.replace(old,'''    /// Normalize and expose small implementations before early equivalence,
    /// then retain the complete target-specific O2 pipeline and its late merge.
    /// No distinct-address aliasing or verification permission is introduced.
    BeforeAndAfter,''')
s+='''

#[cfg(test)]
mod pipeline_policy_tests {
    use super::*;
    #[test]
    fn normalized_early_policy_keeps_the_complete_default_and_a_distinct_cache_identity() {
        let early = pipeline_identity(Optimization::O2, FunctionMerging::BeforeAndAfter);
        assert!(early.starts_with("function(sroa,early-cse),ipsccp,globalopt,"));
        assert!(early.contains("cgscc(inline),mergefunc,default<O2>"));
        assert!(early.ends_with("|merge-functions=after|aliases=false"));
        assert_ne!(early, "mergefunc,default<O2>|merge-functions=after|aliases=false");
        assert_ne!(early, pipeline_identity(Optimization::O2, FunctionMerging::AfterOptimization));
        assert_eq!(pipeline_identity(Optimization::O2, FunctionMerging::Disabled), "default<O2>");
        assert_eq!(pipeline_identity(Optimization::O0, FunctionMerging::BeforeAndAfter), "unsupported<O0,function-merging>");
    }
}
'''
p.write_text(s)
print('Fixed explicit before-and-after policy upgraded; default/off/after unchanged; no environment selector')
