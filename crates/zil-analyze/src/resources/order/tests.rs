use super::*;
fn budget() -> Budget { Budget::new(&Limits::default()) }
fn oracle(order: &Order, expected: &BTreeSet<Edge>) {
    assert_eq!(order.len(), expected.len());
    assert_eq!(order.all().collect::<BTreeSet<_>>(), *expected);
    if let Order::Packed { words, .. } = order { assert!(words.values().all(|&v|v!=0)); }
}
#[test]
fn every_four_vertex_relation_preserves_union_forget_and_successors() {
    for mask in 0..65536u32 {
        let reference = (0..16).filter(|bit|mask & (1<<bit)!=0).map(|bit|(LocalId(bit/4),LocalId(bit%4))).collect::<BTreeSet<_>>();
        let mut order = Order::Small(reference.clone());
        order.pack(&mut budget()).unwrap();
        oracle(&order,&reference);
        assert_eq!(order,Order::Small(reference.clone()));
        for local in 0..4 {
            let mut removed = order.clone();
            removed.forget(LocalId(local),&mut budget()).unwrap();
            let want = reference.iter().copied().filter(|&(a,b)|a!=LocalId(local) && b!=LocalId(local)).collect();
            oracle(&removed,&want);
            let actual = order.cleanup_edges(LocalId(local),&mut budget()).unwrap().collect::<Vec<_>>();
            assert_eq!(actual,reference.iter().copied().filter(|&(a,_)|a==LocalId(local)).collect::<Vec<_>>());
        }
        let complement = (0..16).filter(|bit|mask & (1<<bit)==0).map(|bit|(LocalId(bit/4),LocalId(bit%4))).collect::<BTreeSet<_>>();
        order.join(&Order::Small(complement),&mut budget()).unwrap();
        assert_eq!(order.len(),16);
        assert!(!order.join(&order.clone(),&mut budget()).unwrap());
    }
}
#[test]
fn packing_threshold_and_sparse_maximum_ids_do_not_allocate_by_id() {
    let mut order = Order::default(); let mut expected = BTreeSet::new(); let mut b = budget();
    for n in 0..300u32 {
        let edge = (LocalId(n),LocalId(u32::MAX-n));
        order.insert(edge,&mut b,true,"control").unwrap(); expected.insert(edge);
        if n<256 { assert!(matches!(order,Order::Small(_))); }
    }
    assert!(matches!(order,Order::Packed { .. })); oracle(&order,&expected);
    for n in 0..300u32 {
        let local=LocalId(u32::MAX-n);
        order.forget(local,&mut b).unwrap(); expected.retain(|&(a,c)|a!=local && c!=local);
        oracle(&order,&expected);
    }
    assert_eq!(order.len(),0);
}
#[test]
fn repeated_births_cross_word_boundaries_and_mixed_representation_joins_match_sets() {
    let mut a=Order::default(); let mut reference=BTreeSet::new(); let mut b=budget(); let mut seed=0x997124u64;
    for step in 0..5000 {
        seed=seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let x=LocalId(((seed>>32)%140) as u32);let y=LocalId(((seed>>16)%140) as u32);
        if step%7==0 { a.forget(x,&mut b).unwrap(); reference.retain(|&(p,q)|p!=x && q!=x); }
        else { a.insert((x,y),&mut b,true,"control").unwrap();reference.insert((x,y)); }
        if step%29==0 {
            let from=BTreeSet::from([(x,y),(y,x),(LocalId(63),LocalId(64)),(LocalId(127),LocalId(128))]);
            let old=reference.len(); reference.extend(&from);
            assert_eq!(a.join(&Order::Small(from),&mut b).unwrap(),old!=reference.len());
        }
        oracle(&a,&reference);
    }
}
#[test]
fn packed_work_is_bounded_and_exhaustion_precedes_mutating_word_operations() {
    let relation=(0..64).flat_map(|a|(a+1..64).map(move |b|(LocalId(a),LocalId(b)))).collect::<BTreeSet<_>>();
    let mut order=Order::Small(relation.clone()); order.pack(&mut budget()).unwrap();
    assert!(order.storage_cost()<relation.len()/8);
    let before=order.clone();
    let mut zero=Budget::new(&Limits{max_analysis_steps:0,..Limits::default()});
    assert!(order.forget(LocalId(3),&mut zero).is_err());assert_eq!(order,before);
    let mut zero=Budget::new(&Limits{max_analysis_steps:0,..Limits::default()});
    assert!(order.insert((LocalId(70),LocalId(71)),&mut zero,true,"control").is_err());assert_eq!(order,before);
    let mut exact=Budget::new(&Limits{max_analysis_steps:order.storage_cost(),..Limits::default()});
    order.forget(LocalId(3),&mut exact).unwrap();
    let expected=relation.into_iter().filter(|&(a,b)|a!=LocalId(3) && b!=LocalId(3)).collect();oracle(&order,&expected);
}
