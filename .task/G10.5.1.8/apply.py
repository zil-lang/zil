from pathlib import Path
import hashlib
p=Path('crates/zil-backend-llvm/src/emit/resource_closures.rs');s=p.read_text();raw=s.encode()
assert hashlib.sha1(b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest()=='e8013b2aeabe931d5926fe693068ce583eeb5b64'
s=s.replace('use super::*;','use super::*;\nmod destructor_boundary;',1)
a='''    let mut text = m::Code::new();
    // Every concrete environment'''
b='''    let mut text = m::Code::new();
    let bounded = closures.iter().filter(|d| destructor_boundary::capture(d).is_some()).take(16).count() == 16;
    // Every concrete environment'''
assert s.count(a)==1;s=s.replace(a,b,1)
a='''    for definition in closures {
        let env = environment_type(definition.id);'''
b='''    for definition in closures {
        let boundary = if bounded { destructor_boundary::capture(definition) } else { None };
        if let Some(capture) = boundary {
            text.append(destructor_boundary::entry(definition, capture));
        }
        let env = environment_type(definition.id);'''
assert s.count(a)==1;s=s.replace(a,b,1)
a='''                name: format!("zil.closure.drop.{}", definition.id.0),'''
b='''                name: if boundary.is_some() { destructor_boundary::worker_name(definition.id) } else { format!("zil.closure.drop.{}", definition.id.0) },'''
assert s.count(a)==1;s=s.replace(a,b,1)
pos=s.index('pub(super) fn drop_functions(');a='''                attrs: vec![],
                blocks: None,''';k=s.index(a,pos)
s=s[:k]+s[k:].replace(a,'''                attrs: if boundary.is_some() { vec![m::Attr::NoInline] } else { vec![] },
                blocks: None,''',1)
p.write_text(s)
print('Applied empty-aware boundary; original allocated destructor body retained')
