#!/usr/bin/env python3
from pathlib import Path
import hashlib
p=Path('crates/zil-backend-llvm/src/emit/resource_closures.rs');raw=p.read_bytes()
assert hashlib.sha1(b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest()=='e8013b2aeabe931d5926fe693068ce583eeb5b64'
s=raw.decode()
anchor='''/// Field cleanup follows the fixed source capture order. The local containing
'''
helper='''// Current independent Verify requires every owned capture/direct Package field
// to be LIVE at the return of Fn/FnMut entries. Whole capture construction also
// requires complete owned values. Therefore their final cleanup need not test
// each capture again after the caller has tested/surrendered the environment.
// Keep this first specialization to byte storage and direct Package fields;
// FnOnce, lending, nested ownership and user teardown retain dynamic guards.
fn complete_byte_environment(definition: &ClosureDefinition) -> bool {
    fn field(ty: &Type) -> bool {
        flag_count(ty) == 0
            || matches!(ty, Type::Managed(payload) if **payload == Type::ByteBuffer)
    }
    definition.kind != CallableKind::FnOnce
        && definition.lending_captures.is_empty()
        && definition.captures.iter().all(|ty| match ty {
            Type::Package { fields, .. } => fields.iter().all(|(_, ty)| field(ty)),
            ty => field(ty),
        })
}

'''
assert s.count(anchor)==1;s=s.replace(anchor,helper+anchor,1)
old='''    for definition in closures {
        let env = environment_type(definition.id);'''
new='''    for definition in closures {
        let env = environment_type(definition.id);
        let complete = complete_byte_environment(definition);'''
assert s.count(old)==1;s=s.replace(old,new,1)
# Split the original event vector around each unchanged dynamic guard. The flag
# address and surrender store stay in both paths, in the exact original order.
a='''                m::Event::Instruction(m::Inst::new(
                    Some(format!("live.{}", index)),'''
b='''                m::Event::Instruction(m::Inst::new(
                    None,
                    m::Op::Store {
                        arg: m::Arg::new(m::Ty::Int(1), m::Val::Bool(false)),
                        ptr: m::local(format!("flag.ptr.{}", index)),'''
i=s.index(a);j=s.index(b,i)
s=s[:i]+'''            ]));
            if !complete {
                text.append(m::Code::from(vec![
'''+s[i:j]+'''                ]));
            }
            text.append(m::Code::from(vec![
'''+s[j:]
a='''                            m::Event::Instruction(m::Inst::new(
                                Some(format!("field.live.{}.{}", index, field)),'''
b='''                            m::Event::Instruction(m::Inst::new(
                                None,
                                m::Op::Store {
                                    arg: m::Arg::new(m::Ty::Int(1), m::Val::Bool(false)),
                                    ptr: m::local(format!("field.flag.{}.{}", index, field)),'''
i=s.index(a);j=s.index(b,i)
s=s[:i]+'''                        ]));
                        if !complete {
                            text.append(m::Code::from(vec![
'''+s[i:j]+'''                            ]));
                        }
                        text.append(m::Code::from(vec![
'''+s[j:]
old='''                        text.append(m::Code::from(vec![
                            m::Event::Instruction(m::Inst::new(
                                None,
                                m::Op::Branch(format!("field.done.{}.{}", index, field)),
                            )),
                            m::Event::Label(format!("field.done.{}.{}", index, field)),
                        ]));'''
assert s.count(old)==1;s=s.replace(old,'''                        if !complete {
'''+old+'''
                        }''')
old='''            text.append(m::Code::from(vec![
                m::Event::Instruction(m::Inst::new(None, m::Op::Branch(format!("done.{}", index)))),
                m::Event::Label(format!("done.{}", index)),
            ]));'''
assert s.count(old)==1;s=s.replace(old,'''            if !complete {
'''+old+'''
            }''')
s+='\n#[cfg(test)]\nmod complete_drop_tests;\n'
p.write_text(s)
print('Only guarded cleanup shape changed; FnOnce and unsupported capture shapes unchanged')
