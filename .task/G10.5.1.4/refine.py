#!/usr/bin/env python3
"""Remove duplicate guards only from the private, sole-caller grow transaction."""
from pathlib import Path
import hashlib
p=Path('crates/zil-backend-llvm/src/emit/byte_buffer.rs');s=p.read_text()
assert hashlib.sha1(b'blob '+str(len(p.read_bytes())).encode()+b'\0'+p.read_bytes()).hexdigest()=='8922e0eccded8e5800bac50a2bb29e055f3f0f15'
start=s.index('fn storage_operations()')
entry=s.index('        m::Event::Label("entry".into()),',start)
cap=s.index('        m::Event::Instruction(m::Inst::new(\n            Some("cap.field".into()),',entry)
load=s.index('        m::Event::Instruction(m::Inst::new(\n            Some("cap".into()),',cap)
prepare=s.index('        m::Event::Label("prepare".into()),',load)
s=s[:entry]+'''        // The only caller is reserve_front's checked growth branch. Retain the
        // header address for publication, but do not repeat its negative and
        // capacity guards. This is private control flow, not an LLVM assumption.
        m::Event::Label("entry".into()),
'''+s[cap:load]+s[prepare+len('        m::Event::Label("prepare".into()),\n'):]
p.write_text(s)
p=Path('crates/zil-backend-llvm/src/emit/byte_buffer/reserve_tests.rs');s=p.read_text()
a=s.index('    assert_eq!(\n        fb[0], gb[0],');b=s.index('    for label in ["done", "failed"] {',a)
s=s[:a]+'''    // The public-facing helper rejects negative input before dereferencing the
    // header, and reaches the private grow only from the failed capacity test.
    assert_eq!(fb[0].name, "entry");
    assert!(matches!(&fb[0].instructions[0].op,
        m::Op::Compare { predicate, ty:m::Ty::Int(64), left:m::Val::Local(x), right:m::Val::Int(0) }
        if predicate == "slt" && x == "requested"));
    assert!(matches!(&fb[0].instructions[1].op,
        m::Op::CondBranch { condition:m::Val::Local(c), yes, no }
        if c == "negative" && yes == "failed" && no == "capacity"));
    let capacity = fb.iter().find(|b| b.name == "capacity").unwrap();
    assert!(matches!(&capacity.instructions.last().unwrap().op,
        m::Op::CondBranch { condition:m::Val::Local(c), yes, no }
        if c == "fits" && yes == "done" && no == "growth"));
    assert!(capacity.instructions.iter().any(|i| matches!(&i.op,
        m::Op::Compare { predicate, ty:m::Ty::Int(64), left:m::Val::Local(x), right:m::Val::Local(cap) }
        if predicate == "ule" && x == "requested" && cap == "cap")));
    assert_eq!(gb[0].name, "entry");
    assert!(!gb.iter().any(|b| b.name == "capacity" || b.name == "prepare"));
    assert!(!gb.iter().flat_map(|b| &b.instructions).any(|i|
        matches!(&i.op, m::Op::Compare { ty:m::Ty::Int(64), .. })));
    let callers = fs.iter().filter(|f| f.blocks.as_ref().unwrap().iter()
        .flat_map(|b| &b.instructions).any(|i| matches!(&i.op,
            m::Op::Call { callee:m::Val::Global(name), .. } if name == "zil.bytes.grow")))
        .map(|f| f.name.as_str()).collect::<Vec<_>>();
    assert_eq!(callers, vec!["zil.bytes.reserve"]);
'''+s[b:]
p.write_text(s)
for f in ['crates/zil-backend-llvm/src/emit/byte_buffer.rs','crates/zil-backend-llvm/src/emit/byte_buffer/reserve_tests.rs']:
    print(f,hashlib.sha256(Path(f).read_bytes()).hexdigest())
