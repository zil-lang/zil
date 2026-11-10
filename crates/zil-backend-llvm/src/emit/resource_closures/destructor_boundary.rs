//! Keep empty/surrendered cleanup inline; outline only allocated byte storage.
//! No new ownership, aliasing, coldness or allocation-success fact is asserted.
use super::*;

// Outer Option is eligibility; the inner index is an optional direct Package
// field. Other owning fields, nested owners and multiple buffers stay original.
fn byte_field(ty: &Type) -> Option<Option<usize>> {
    match ty {
        Type::Managed(payload) if **payload == Type::ByteBuffer => Some(None),
        Type::Package { fields, .. } => {
            let mut found = None;
            for (i, (_, ty)) in fields.iter().enumerate() {
                if flag_count(ty) == 0 { continue; }
                if !matches!(ty, Type::Managed(payload) if **payload == Type::ByteBuffer)
                    || found.replace(i).is_some() { return None; }
            }
            found.map(Some)
        }
        _ => None,
    }
}
pub(super) fn capture(definition: &ClosureDefinition) -> Option<u32> {
    let mut found = None;
    for (index, ty) in definition.captures.iter().enumerate() {
        if flag_count(ty) == 0 { continue; }
        byte_field(ty)?;
        if found.replace(u32::try_from(index).ok()?).is_some() { return None; }
    }
    found
}
pub(super) fn worker_name(id: ClosureId) -> String {
    format!("zil.closure.drop.allocated.{}", id.0)
}
pub(super) fn entry(definition: &ClosureDefinition, capture: u32) -> m::Code {
    let child = byte_field(&definition.captures[capture as usize]).expect("eligible byte capture");
    let local = |name: &str| m::local(name.into());
    let arg = |ty, name: &str| m::Arg::new(ty, local(name));
    let inst = |name: Option<&str>, op| m::Event::Instruction(m::Inst::new(name.map(str::to_owned), op));
    let field = |index| m::Op::Gep {
        ty: environment_type(definition.id), ptr: local("env"),
        indices: vec![m::Arg::new(m::Ty::Int(32), m::Val::Int(0)), m::Arg::new(m::Ty::Int(32), m::val(index))],
    };
    let clear = |name: &str| inst(None, m::Op::Store {
        arg: m::Arg::new(m::Ty::Int(1), m::Val::Bool(false)), ptr: local(name), align: 1,
    });
    let flag = flag_index(definition, capture);
    let mut code = m::Code::from(vec![
        m::Event::Begin(m::Function {
            name: format!("zil.closure.drop.{}", definition.id.0), linkage: m::Linkage::Internal,
            signature: m::Signature {
                result: m::Ty::Void, result_attrs: vec![],
                params: vec![m::Param { ty: m::Ty::Ptr, name: Some("env".into()), attrs: vec![] }],
                variadic: false,
            }, attrs: vec![], blocks: None,
        }),
        m::Event::Label("entry".into()),
        inst(Some("flag"), field(flag)),
        inst(Some("live"), m::Op::Load { ty: m::Ty::Int(1), ptr: local("flag"), align: 1 }),
        inst(None, m::Op::CondBranch { condition: local("live"), yes: if child.is_some() { "child".into() } else { "inspect".into() }, no: "done".into() }),
    ]);
    if child.is_some() {
        // Only one owned direct child is eligible, so its flag follows the
        // shell. Neither a surrendered shell nor a surrendered field is read.
        code.append(m::Code::from(vec![
            m::Event::Label("child".into()),
            inst(Some("child.flag"), field(flag + 1)),
            inst(Some("child.live"), m::Op::Load { ty: m::Ty::Int(1), ptr: local("child.flag"), align: 1 }),
            inst(None, m::Op::CondBranch { condition: local("child.live"), yes: "inspect".into(), no: "vacant".into() }),
            m::Event::Label("vacant".into()),
            clear("flag"),
            inst(None, m::Op::Branch("done".into())),
        ]));
    }
    let mut indices = vec![m::Arg::new(m::Ty::Int(32), m::Val::Int(0)), m::Arg::new(m::Ty::Int(32), m::val(capture))];
    if let Some(field) = child { indices.push(m::Arg::new(m::Ty::Int(32), m::val(field))); }
    code.append(m::Code::from(vec![
        m::Event::Label("inspect".into()),
        inst(Some("payload"), m::Op::Gep { ty: environment_type(definition.id), ptr: local("env"), indices }),
        inst(Some("data.field"), m::Op::Gep {
            ty: m::Ty::Struct(vec![m::Ty::Ptr, m::Ty::Int(64), m::Ty::Int(64)]), ptr: local("payload"),
            indices: vec![m::Arg::new(m::Ty::Int(32), m::Val::Int(0)), m::Arg::new(m::Ty::Int(32), m::Val::Int(0))],
        }),
        inst(Some("data"), m::Op::Load { ty: m::Ty::Ptr, ptr: local("data.field"), align: 8 }),
        inst(Some("is.empty"), m::Op::Compare { predicate: "eq".into(), ty: m::Ty::Ptr, left: local("data"), right: m::Val::Null }),
        inst(None, m::Op::CondBranch { condition: local("is.empty"), yes: "empty".into(), no: "allocated".into() }),
        m::Event::Label("empty".into()),
        clear("flag"),
    ]));
    if child.is_some() { code.push(clear("child.flag")); }
    code.append(m::Code::from(vec![
        // Preserve even the null allocator call and both flag stores before it.
        // Optimization follows LLVM's ordinary libcall rules, not an assumption.
        inst(None, m::Op::Call { result: m::Ty::Void, result_attrs: vec![], callee: m::global("zil.bytes.drop".into()), args: vec![arg(m::Ty::Ptr, "payload")], vararg: None }),
        inst(None, m::Op::Branch("done".into())),
        m::Event::Label("allocated".into()),
        // Neither flag has changed on this path. The original full worker
        // repeats the checks and performs the original cleanup instruction order.
        inst(None, m::Op::Call { result: m::Ty::Void, result_attrs: vec![], callee: m::global(worker_name(definition.id)), args: vec![arg(m::Ty::Ptr, "env")], vararg: None }),
        inst(None, m::Op::Branch("done".into())),
        m::Event::Label("done".into()), inst(None, m::Op::Return(None)), m::Event::End,
    ]));
    code
}

#[cfg(test)]
mod tests {
    use super::*;
    fn definition(id: u32, count: usize, package: bool) -> ClosureDefinition {
        let byte = Type::Managed(Box::new(Type::ByteBuffer));
        let ty = if package { Type::Package { name: "State".into(), fields: vec![("sum".into(),Type::I64),("data".into(),byte)] } } else { byte };
        ClosureDefinition {
            storage_life: None, id: ClosureId(id), function: FunctionId(id), captures: vec![ty; count],
            parameters: vec![], result: Type::Unit, kind: CallableKind::FnMut, lending_captures: vec![],
        }
    }
    fn functions(definitions: &[ClosureDefinition]) -> Vec<m::Function> {
        drop_functions(definitions, &[ResourceDefinition { name: "bytes".into(), payload: Type::ByteBuffer, teardown: None }])
            .unwrap().into_items().unwrap().into_iter().map(|item| {
                let m::Item::Function(function) = item else { panic!("expected function") }; function
            }).collect()
    }
    #[test]
    fn allocated_boundary_preserves_the_entire_original_worker() {
        for package in [false,true] {
            for count in [1,8,15,16,32,64] {
                let ds=(0..count).map(|id|definition(id,1,package)).collect::<Vec<_>>(); let fs=functions(&ds);
                assert_eq!(fs.len(),count as usize*if count>=16{2}else{1});
                for d in &ds {
                    let original=functions(std::slice::from_ref(d)).pop().unwrap();
                    if count<16 { assert_eq!(*fs.iter().find(|f|f.name==original.name).unwrap(),original); continue; }
                    let mut worker=fs.iter().find(|f|f.name==worker_name(d.id)).unwrap().clone();
                    assert_eq!(worker.attrs,vec![m::Attr::NoInline]); worker.name=original.name.clone();worker.attrs.clear();assert_eq!(worker,original);
                    let front=fs.iter().find(|f|f.name==original.name).unwrap();assert!(front.attrs.is_empty());assert_eq!(front.signature,original.signature);
                    let blocks=front.blocks.as_ref().unwrap();
                    assert!(matches!(&blocks[0].instructions.last().unwrap().op,m::Op::CondBranch{yes,no,..} if yes==if package{"child"}else{"inspect"} && no=="done"));
                    assert!(!blocks[0].instructions.iter().any(|i|matches!(&i.op,m::Op::Load{ty:m::Ty::Ptr,..})));
                    let empty=blocks.iter().find(|b|b.name=="empty").unwrap();let clears=if package{2}else{1};
                    assert!(empty.instructions[..clears].iter().all(|i|matches!(&i.op,m::Op::Store{..})));
                    assert!(matches!(&empty.instructions[clears].op,m::Op::Call{callee:m::Val::Global(name),..} if name=="zil.bytes.drop"));
                    if package {
                        let vacant=blocks.iter().find(|b|b.name=="vacant").unwrap();assert!(matches!(vacant.instructions[0].op,m::Op::Store{..}));
                        assert!(!vacant.instructions.iter().any(|i|matches!(i.op,m::Op::Call{..}|m::Op::Load{..})));
                    }
                }
            }
        }
    }
    #[test]
    fn multiple_and_nested_owners_do_not_enter_the_fast_path() {
        for package in [false,true] {
            assert_eq!(capture(&definition(0,1,package)),Some(0));assert_eq!(capture(&definition(0,2,package)),None);assert_eq!(capture(&definition(0,0,package)),None);
            let mut d=definition(0,1,package);d.captures.insert(0,Type::I64);assert_eq!(capture(&d),Some(1));d.captures.push(Type::Closure(ClosureId(1)));assert_eq!(capture(&d),None);
            let ds=(0..32).map(|id|definition(id,2,package)).collect::<Vec<_>>();let fs=functions(&ds);assert_eq!(fs.len(),32);assert!(fs.iter().all(|f|f.attrs.is_empty()));
        }
        let ty=Type::Package{name:"Two".into(),fields:vec![("a".into(),Type::Managed(Box::new(Type::ByteBuffer))),("b".into(),Type::Managed(Box::new(Type::ByteBuffer)))]};
        assert_eq!(byte_field(&ty),None);
    }
}
