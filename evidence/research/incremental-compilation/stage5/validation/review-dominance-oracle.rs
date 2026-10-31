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
    let b=new::validate(m).map(|s|s.functions);
    assert_eq!(a,b,"complete validator mismatch: {m:?}");
    a.is_ok()
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
    let n=131072usize;
    let reachable=vec![true;n];let idom=(0..n).map(|b|b.saturating_sub(1)).collect::<Vec<_>>();
    let spans=new::review_spans(&reachable,&idom);
    for b in 1..n {
        assert!(new::review_query(&reachable,&idom,Some(&spans),b-1,b));
        assert!(!new::review_query(&reachable,&idom,Some(&spans),b,b-1));
        assert!(new::review_query(&reachable,&idom,Some(&spans),0,b));
    }
    println!("{{\"graphs\":{graphs},\"all_pairs\":{pairs},\"complete_modules\":{modules},\"accepted_modules\":{accepted},\"rejected_modules\":{},\"deep_chain\":{n},\"pass\":true}}",modules-accepted);
}
