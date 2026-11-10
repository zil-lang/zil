#!/usr/bin/env python3
from pathlib import Path
import hashlib
p=Path('crates/zil-backend-llvm/src/emit/byte_buffer.rs');s=p.read_text()
assert hashlib.sha256(p.read_bytes()).hexdigest()=='b5c81e87e0b6c04ed233a3b18b8689ef46d31dc02e3c6abae5a39512abb7fd0a'
start=s.index('fn storage_operations() -> m::Code {')
name=s.index('name: "zil.bytes.reserve".into()',start)
attrs=s.index('            attrs: vec![],\n            blocks: None,',name)
s=s[:attrs]+s[attrs:].replace('            attrs: vec![],\n            blocks: None,','            attrs: vec![m::Attr::NoInline],\n            blocks: None,',1)
s=s[:name]+s[name:].replace('name: "zil.bytes.reserve".into()','name: "zil.bytes.grow".into()',1)
s=s.replace('fn storage_operations() -> m::Code {\n    m::Code::from(vec![','''fn storage_operations() -> m::Code {
    // Keep the common capacity check inline. The unchanged allocation/publish
    // transaction is a shared out-of-line slow path, not repeated in each
    // inlined closure. No cold-frequency, alias or success facts are asserted.
    let mut code = reserve_front();
    code.append(m::Code::from(vec![''',1)
end=s.index('\n}\n',start)+2;assert s[:end].endswith('])\n}');tail=s[end:]
s=s[:end-len('])\n}')]+''']));
    code
}

fn reserve_front() -> m::Code {
    let header = m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64), m::Ty::Int(64)]);
    let arg = |ty, name: &str| m::Arg::new(ty, m::local(name.into()));
    m::Code::from(vec![
        m::Event::Begin(m::Function {
            name: "zil.bytes.reserve".into(),
            linkage: m::Linkage::Internal,
            signature: m::Signature {
                result: m::Ty::Int(1), result_attrs: vec![], variadic: false,
                params: vec![
                    m::Param {ty:m::Ty::Ptr,name:Some("head".into()),attrs:vec![]},
                    m::Param {ty:m::Ty::Int(64),name:Some("requested".into()),attrs:vec![]},
                ],
            },
            attrs: vec![], blocks: None,
        }),
        m::Event::Label("entry".into()),
        m::Event::Instruction(m::Inst::new(Some("negative".into()),m::Op::Compare {
            predicate:"slt".into(),ty:m::Ty::Int(64),left:m::local("requested".into()),right:m::Val::Int(0),
        })),
        m::Event::Instruction(m::Inst::new(None,m::Op::CondBranch {
            condition:m::local("negative".into()),yes:"failed".into(),no:"capacity".into(),
        })),
        m::Event::Label("capacity".into()),
        m::Event::Instruction(m::Inst::new(Some("cap.field".into()),m::Op::Gep {
            ty:header,ptr:m::local("head".into()),indices:vec![m::Arg::new(m::Ty::Int(32),m::Val::Int(0)),m::Arg::new(m::Ty::Int(32),m::Val::Int(2))],
        })),
        m::Event::Instruction(m::Inst::new(Some("cap".into()),m::Op::Load {
            ty:m::Ty::Int(64),ptr:m::local("cap.field".into()),align:8,
        })),
        m::Event::Instruction(m::Inst::new(Some("fits".into()),m::Op::Compare {
            predicate:"ule".into(),ty:m::Ty::Int(64),left:m::local("requested".into()),right:m::local("cap".into()),
        })),
        m::Event::Instruction(m::Inst::new(None,m::Op::CondBranch {
            condition:m::local("fits".into()),yes:"done".into(),no:"growth".into(),
        })),
        m::Event::Label("growth".into()),
        m::Event::Instruction(m::Inst::new(Some("result".into()),m::Op::Call {
            result:m::Ty::Int(1),result_attrs:vec![],callee:m::global("zil.bytes.grow".into()),
            args:vec![arg(m::Ty::Ptr,"head"),arg(m::Ty::Int(64),"requested")],vararg:None,
        })),
        m::Event::Instruction(m::Inst::new(None,m::Op::Return(Some(arg(m::Ty::Int(1),"result"))))),
        m::Event::Label("done".into()),
        m::Event::Instruction(m::Inst::new(None,m::Op::Return(Some(m::Arg::new(m::Ty::Int(1),m::Val::Bool(true)))))),
        m::Event::Label("failed".into()),
        m::Event::Instruction(m::Inst::new(None,m::Op::Return(Some(m::Arg::new(m::Ty::Int(1),m::Val::Bool(false)))))),
        m::Event::End,
    ])
}

#[cfg(test)]
mod reserve_tests;
'''+tail
p.write_text(s)
t=Path('crates/zil-backend-llvm/src/emit/byte_buffer/reserve_tests.rs');t.parent.mkdir(exist_ok=True)
t.write_text('''use super::*;

fn functions() -> Vec<m::Function> {
    storage_operations().into_items().unwrap().into_iter().filter_map(|i| {
        if let m::Item::Function(f)=i {Some(f)} else {None}
    }).collect()
}

#[test]
fn reserve_fast_path_keeps_checks_without_inline_allocation_or_public_symbols() {
    let fs=functions();
    let front=fs.iter().find(|f|f.name=="zil.bytes.reserve").unwrap();
    let grow=fs.iter().find(|f|f.name=="zil.bytes.grow").unwrap();
    assert_eq!(front.signature,grow.signature);
    assert!(front.attrs.is_empty());
    assert_eq!(grow.attrs,vec![m::Attr::NoInline]);
    assert!(fs.iter().all(|f|f.linkage==m::Linkage::Internal));
    assert_eq!(fs.iter().filter(|f|f.attrs.contains(&m::Attr::NoInline)).count(),1);
    let fb=front.blocks.as_ref().unwrap();let gb=grow.blocks.as_ref().unwrap();
    assert_eq!(fb[0],gb[0],"negative requests still reject before accessing the header");
    let mut old_capacity=gb[1].clone();
    if let m::Op::CondBranch{no,..}=&mut old_capacity.instructions.last_mut().unwrap().op {
        assert_eq!(no,"prepare");*no="growth".into();
    } else {panic!("capacity branch missing")}
    assert_eq!(fb[1],old_capacity);
    for label in ["done","failed"] {
        assert_eq!(fb.iter().find(|b|b.name==label),gb.iter().find(|b|b.name==label));
    }
    let calls=fb.iter().flat_map(|b|&b.instructions).filter_map(|i| {
        if let m::Op::Call{callee,..}=&i.op {Some(callee)} else {None}
    }).collect::<Vec<_>>();
    assert_eq!(calls,vec![&m::Val::Global("zil.bytes.grow".into())]);
    assert!(!fb.iter().flat_map(|b|&b.instructions).any(|i|matches!(i.op,m::Op::Store{..})));
    let stores=gb.iter().flat_map(|b|b.instructions.iter().filter_map(move |i| {
        matches!(i.op,m::Op::Store{..}).then_some(b.name.as_str())
    })).collect::<Vec<_>>();
    assert_eq!(stores,vec!["publish","publish"],"pointer/capacity are published only after success");
}

#[test]
fn outlined_storage_helpers_pass_complete_machine_preflight() {
    let mut code=m::Code::new();
    for (name,result,params) in [
        ("realloc",m::Ty::Ptr,vec![m::Ty::Ptr,m::Ty::Int(64)]),
        ("free",m::Ty::Void,vec![m::Ty::Ptr]),
    ] {
        code.push(m::Event::Declare(m::Function {
            name:name.into(),linkage:m::Linkage::External,
            signature:m::Signature {result,result_attrs:vec![],params:params.into_iter().map(m::Param::unnamed).collect(),variadic:false},
            attrs:vec![],blocks:None,
        }));
    }
    code.append(linux_allocation());code.append(storage_operations());
    let module=m::Module::from_code("x86_64-unknown-linux-gnu".into(),"reserve-control".into(),code).unwrap();
    zil_llvm_api::validate_module(&module).unwrap();
}
''')
print('outlined_source_sha256='+hashlib.sha256(p.read_bytes()).hexdigest())
