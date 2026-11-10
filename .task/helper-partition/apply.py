from pathlib import Path
import hashlib
p=Path('crates/zil-backend-llvm/src/machine_cgu.rs');raw=p.read_bytes()
assert hashlib.sha1(b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest()=='f8bef18a6234e8f74719784ff078e1ad2e3df541'
s=raw.decode();needle='pub(crate) mod layout;';assert s.count(needle)==1;s=s.replace(needle,needle+'\nmod resource_helpers;')
needle='''    let cache = cache_root.and_then(|root| {
        layout::Cache::new('''
replacement='''    // Shared helper code alone must not force otherwise independent resource
    // modules into one object. The optional plan retains complete direct-call
    // visibility, identity-bearing local components and the original copy bound.
    if import_complete_call_closures
        && let Some(recipe) = resource_helpers::prepare(&module, lir, entry, count)
    {
        return (recipe.replay(module), None);
    }
    let cache = cache_root.and_then(|root| {
        layout::Cache::new('''
assert s.count(needle)==1;s=s.replace(needle,replacement);p.write_text(s)
p=Path('crates/zil-backend-llvm/src/machine_cgu/resource_helpers.rs');s=p.read_text()
s=s.replace('count < 2 || count > 64','!(2..=64).contains(&count)')
s=s.replace('let owners=defs.iter().enumerate().map(|(id,d)| {','let owners=defs.iter().enumerate().map(|(id,_)| {')
p.write_text(s)
print('Connected optional multi-file closure planner; single-file and rejected plans stay unchanged')
