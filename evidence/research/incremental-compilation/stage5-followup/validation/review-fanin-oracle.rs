#![allow(dead_code)]
#[path = "model.rs"] mod model;
#[path = "old.rs"] mod old;
#[path = "new.rs"] mod new;
use model::*;

fn next(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13; *seed ^= *seed >> 7; *seed ^= *seed << 17; *seed
}
fn predecessors(succ: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut pred=vec![vec![];succ.len()];
    for (a, edges) in succ.iter().enumerate() { for &b in edges { pred[b].push(a); } }
    pred
}
fn graph_check(succ: &[Vec<usize>]) -> usize {
    let pred=predecessors(succ);
    graph_pred_check(succ,&pred)
}
fn graph_pred_check(succ: &[Vec<usize>], pred: &[Vec<usize>]) -> usize {
    let (reachable, idom)=new::review_dom(succ,&pred);
    assert_eq!(old::review_dom(succ,&pred),(reachable.clone(),idom.clone()));
    let spans=new::review_spans(&reachable,&idom);
    let n=succ.len();
    let mut dom=vec![vec![true;n];n]; dom[0].fill(false); dom[0][0]=true;
    loop {
        let mut changed=false;
        for b in 1..n {
            if !reachable[b] { continue; }
            let mut d=vec![true;n];
            for &p in &pred[b] { if reachable[p] { for a in 0..n { d[a]&=dom[p][a]; } } }
            d[b]=true;
            if dom[b]!=d {dom[b]=d;changed=true;}
        }
        if !changed {break;}
    }
    for a in 0..n { for b in 0..n {
        let expected=old::review_walk(&reachable,&idom,a,b);
        assert_eq!(expected,!reachable[b] || (reachable[a] && dom[b][a]),"set oracle {a}->{b}");
        assert_eq!(new::review_query(&reachable,&idom,Some(&spans),a,b),expected);
        assert_eq!(new::review_query(&reachable,&idom,None,a,b),expected);
    } }
    n*n
}
fn i(name: &str, op: Op) -> Inst { Inst::new((!name.is_empty()).then(||name.into()),op) }
fn ret(v: Val) -> Inst { i("",Op::Return(Some(Arg::new(Ty::Int(32),v)))) }
fn val(k: usize) -> Val {Val::Local(format!("v{k}"))}
fn module(blocks: Vec<Block>) -> Module {
    Module {triple:"x86_64-unknown-linux-gnu".into(),fingerprint:"review-801".into(),items:vec![Item::Function(Function {
        name:"test".into(),linkage:Linkage::External, signature:Signature::new(Ty::Int(32),vec![Param::new(Ty::Int(1),"c")]),attrs:vec![],blocks:Some(blocks)
    })]}
}
fn full_check(m: &Module) -> bool {
    let a=old::validate(m).map(|s|s.functions);
    for _ in 0..16 {
        // Every call constructs fresh std::HashMap RandomState instances.
        let b=new::validate(m).map(|s|s.functions);
        assert_eq!(a,b,"complete validator mismatch: {m:?}");
    }
    a.is_ok()
}
fn shuffle<T>(v: &mut [T], seed: &mut u64) {
    for i in (1..v.len()).rev() {let j=next(seed) as usize%(i+1);v.swap(i,j);}
}
fn checked_chain(n: usize) -> Module {
    let pair=Ty::Struct(vec![Ty::Int(32),Ty::Int(1)]);
    let mut bs=vec![Block{name:"entry".into(),instructions:vec![i("",Op::Branch("b1".into()))]}];
    for k in 1..=n {
        bs.push(Block{name:format!("b{k}"),instructions:vec![
            i(&format!("pair{k}"),Op::Call{result:pair.clone(),result_attrs:vec![],vararg:None,callee:Val::Global("llvm.sadd.with.overflow.i32".into()),args:vec![Arg::new(Ty::Int(32),if k==1 {Val::Local("x".into())} else {Val::Local(format!("sum{}",k-1))}),Arg::new(Ty::Int(32),Val::Int(1))]}),
            i(&format!("sum{k}"),Op::Extract{arg:Arg::new(pair.clone(),Val::Local(format!("pair{k}"))),index:0}),
            i(&format!("overflow{k}"),Op::Extract{arg:Arg::new(pair.clone(),Val::Local(format!("pair{k}"))),index:1}),
            i("",Op::CondBranch{condition:Val::Local(format!("overflow{k}")),yes:"trap".into(),no:if k==n {"exit".into()} else {format!("b{}",k+1)}})
        ]});
    }
    bs.push(Block{name:"trap".into(),instructions:vec![i("",Op::Call{result:Ty::Void,result_attrs:vec![],vararg:None,callee:Val::Global("llvm.trap".into()),args:vec![]}),i("",Op::Unreachable)]});
    bs.push(Block{name:"exit".into(),instructions:vec![ret(Val::Local(format!("sum{n}")))]});
    let decl=|name:&str,signature|Item::Function(Function{name:name.into(),linkage:Linkage::External,signature,attrs:vec![],blocks:None});
    Module {triple:"x86_64-unknown-linux-gnu".into(),fingerprint:"independent-checked-chain".into(),items:vec![
        decl("llvm.sadd.with.overflow.i32",Signature::new(pair,vec![Param::unnamed(Ty::Int(32)),Param::unnamed(Ty::Int(32))])),
        decl("llvm.trap",Signature::new(Ty::Void,vec![])),
        Item::Function(Function{name:"test".into(),linkage:Linkage::External,signature:Signature::new(Ty::Int(32),vec![Param::new(Ty::Int(32),"x")]),attrs:vec![],blocks:Some(bs)})
    ]}
}
fn main() {
    let mut graphs=0; let mut pairs=0;
    for n in 1..=4 {
        let edges=(0..n).flat_map(|a|(1..n).map(move|b|(a,b))).collect::<Vec<_>>();
        for mask in 0..(1usize<<edges.len()) {
            let mut succ=vec![vec![];n];
            for (k,&(a,b)) in edges.iter().enumerate() {if (mask>>k)&1!=0 {succ[a].push(b);}}
            pairs+=graph_check(&succ);graphs+=1;
        }
    }
    let mut seed=0x801933a9e1234567;
    for n in [7,31,32,33,64,129] {
        for mode in 0..60 {
            let mut succ=vec![vec![];n];
            for a in 0..n {
                if mode%3==0 && a+1<n {succ[a].push(a+1);}
                for _ in 0..(mode%5) {
                    let b=1+next(&mut seed) as usize%(n-1);
                    succ[a].push(b);
                    if mode%7==0 {succ[a].push(b);}
                }
            }
            pairs+=graph_check(&succ);graphs+=1;
        }
    }
    let mut high_fanin_graphs=0;
    for count in [1,2,30,31,32,33,128,4096] {
        for order in 0..3 {for cycle in 0..3 {
            let trap=count+1;let dead=count+2;
            let mut succ=vec![vec![];count+3];succ[0].push(1);
            for k in 1..=count {succ[k].push(trap);if k<count {succ[k].push(k+1);}}
            if cycle==1 {succ[count].push((count/2).max(1));}
            if cycle==2 {succ[count].push(trap);succ[dead].extend([dead,trap]);}
            let mut pred=predecessors(&succ);
            for p in &mut pred {if order==1 {p.reverse();} else if order==2 {shuffle(p,&mut seed);}}
            let old=old::review_dom(&succ,&pred);let new=new::review_dom(&succ,&pred);assert_eq!(old,new);
            assert_eq!(new.1[trap],1);assert!(!new.0[dead]);assert_eq!(new.1[dead],usize::MAX);
            if count<=128 {pairs+=graph_pred_check(&succ,&pred);}
            graphs+=1;high_fanin_graphs+=1;
        }}
    }
    // Backedges, SCCs and unreachable predecessor populations exercise partially known idoms.
    for n in [31,32,33,65,129] {for case in 0..80 {
        let mut succ=vec![vec![];n];
        for a in 0..n {for b in 1..n {if next(&mut seed)%7==0 {succ[a].push(b);}}}
        let trap=n-1;
        for (a,s) in succ.iter_mut().enumerate() {if case%2==0 || a%3!=0 {s.push(trap);if case%5==0{s.push(trap);}}}
        let mut pred=predecessors(&succ);for p in &mut pred {shuffle(p,&mut seed);}
        pairs+=graph_pred_check(&succ,&pred);graphs+=1;high_fanin_graphs+=1;
    }}
    let mut density_boundary_graphs=0;
    for n in [124,128,129,132,256] {for count in [31,32,33,63,64,65] {for order in 0..3 {
        let trap=n-2;let dead=n-1;
        let mut succ=vec![vec![];n];succ[0].push(1);
        for a in 1..trap-1 {succ[a].push(a+1);}
        for k in 0..count {let a=1+k*(trap-2)/(count-1);succ[a].push(trap);}
        succ[dead].push(dead);
        let mut pred=predecessors(&succ);assert_eq!(pred[trap].len(),count);
        for p in &mut pred {if order==1 {p.reverse();} else if order==2 {shuffle(p,&mut seed);}}
        pairs+=graph_pred_check(&succ,&pred);graphs+=1;density_boundary_graphs+=1;
    }}}
    let mut modules=0;let mut accepted=0;
    for n in [1,7,31,32,33,80] {
        for mode in 0..240 {
            let mut blocks=vec![];
            for k in 0..n {
                let from=next(&mut seed) as usize%n;
                let rhs=if mode%4==0 {val(from)} else {Val::Int(1)};
                let mut inst=vec![i(&format!("v{k}"),Op::Binary {opcode:"add".into(),ty:Ty::Int(32),left:Val::Int(0),right:rhs})];
                if n==1 || k==n-1 || mode%5==0 {
                    inst.push(ret(if mode%3==0 {val(from)} else {val(k)}));
                } else if mode%3==1 {
                    inst.push(i("",Op::Branch(format!("b{}",k+1))));
                } else {
                    inst.push(i("",Op::CondBranch {condition:Val::Local("c".into()),yes:format!("b{}",1+next(&mut seed)as usize%(n-1)),no:format!("b{}",1+next(&mut seed)as usize%(n-1))}));
                }
                blocks.push(Block {name:format!("b{k}"),instructions:inst});
            }
            accepted+=usize::from(full_check(&module(blocks)));modules+=1;
        }
    }
    // Padded diamond crosses the exact 31/32 threshold with real phi/SSA errors.
    for n in [4,31,32,33,80] {for variant in 0..10 {for reorder in [false,true] {
        let mut blocks=vec![
            Block {name:"entry".into(),instructions:vec![i("",Op::CondBranch {condition:Val::Local("c".into()),yes:"left".into(),no:"right".into()})]},
            Block {name:"left".into(),instructions:vec![i("v",Op::Binary {opcode:"add".into(),ty:Ty::Int(32),left:Val::Int(20),right:Val::Int(22)}),i("",Op::Branch("join".into()))]},
            Block {name:"right".into(),instructions:vec![i("",Op::Branch("join".into()))]},
            Block {name:"join".into(),instructions:vec![i("p",Op::Phi {ty:Ty::Int(32),incoming:vec![(Val::Local("v".into()),"left".into()),(Val::Int(7),"right".into())]}),ret(Val::Local("p".into()))]}
        ];
        for k in 4..n {blocks.push(Block {name:format!("pad{k}"),instructions:vec![ret(Val::Int(0))]});}
        match variant {
            0=>{},
            1=>blocks[3].instructions=vec![ret(Val::Local("v".into()))],
            2=>if let Op::Phi{incoming,..}=&mut blocks[3].instructions[0].op {incoming[1].0=Val::Local("v".into());},
            3=>if let Op::Phi{incoming,..}=&mut blocks[3].instructions[0].op {incoming[1].1="entry".into();},
            4=>if let Op::Phi{ty,..}=&mut blocks[3].instructions[0].op {*ty=Ty::Int(64);},
            5=>blocks[1].instructions[0]=i("v",Op::Binary {opcode:"add".into(),ty:Ty::Int(32),left:Val::Local("v".into()),right:Val::Int(1)}),
            6=>blocks[0].instructions=vec![i("",Op::Branch("join".into()))],
            7=>if let Op::Phi{incoming,..}=&mut blocks[3].instructions[0].op {incoming.push((Val::Int(8),"right".into()));},
            8=>{blocks[2].instructions=vec![i("",Op::CondBranch{condition:Val::Local("c".into()),yes:"join".into(),no:"join".into()})]; if let Op::Phi{incoming,..}=&mut blocks[3].instructions[0].op {incoming.push((Val::Int(8),"right".into()));}},
            9=>blocks[2].instructions=vec![i("",Op::Branch("entry".into()))],
            _=>unreachable!()
        }
        if reorder {blocks.swap(1,3);}
        accepted+=usize::from(full_check(&module(blocks)));modules+=1;
    }}}
    let mut checked_modules=0;
    for n in [1,30,31,32,33,128,512] {for order in 0..3 {for bad in 0..5 {
        let mut m=checked_chain(n);
        if let Item::Function(f)=&mut m.items[2] {let bs=f.blocks.as_mut().unwrap();
            match bad {
                0=>{},
                1=>bs[n+1].instructions.insert(0,i("bad",Op::Binary{opcode:"add".into(),ty:Ty::Int(32),left:Val::Local(format!("sum{n}")),right:Val::Int(0)})),
                2=>bs[n+1].instructions.insert(0,i("bad",Op::Phi{ty:Ty::Int(32),incoming:(1..=n).map(|k|(Val::Local(format!("sum{k}")),format!("b{k}"))).collect()})),
                3=>bs[1].instructions[0].result=Some("exit".into()),
                4=>{bs[1].instructions[0].result=Some("entry".into());bs[n].instructions[0].result=Some("trap".into());},
                _=>unreachable!()
            }
            if order==1 {bs[1..].reverse();} else if order==2 {shuffle(&mut bs[1..],&mut seed);}
        }
        let ok=full_check(&m);if bad==0 || bad==2 {assert!(ok);}accepted+=usize::from(ok);modules+=1;checked_modules+=1;
    }}}
    let n=131072usize;
    let reachable=vec![true;n];let idom=(0..n).map(|b|b.saturating_sub(1)).collect::<Vec<_>>();
    let spans=new::review_spans(&reachable,&idom);
    for b in 1..n {
        assert!(new::review_query(&reachable,&idom,Some(&spans),b-1,b));
        assert!(!new::review_query(&reachable,&idom,Some(&spans),b,b-1));
        assert!(new::review_query(&reachable,&idom,Some(&spans),0,b));
    }
    println!("{{\"graphs\":{graphs},\"high_fanin_graphs\":{high_fanin_graphs},\"density_boundary_graphs\":{density_boundary_graphs},\"all_pairs\":{pairs},\"complete_modules\":{modules},\"checked_chain_modules\":{checked_modules},\"fresh_hashmap_validations\":{},\"accepted_modules\":{accepted},\"rejected_modules\":{},\"deep_chain\":{n},\"pass\":true}}",modules*16,modules-accepted);
}
