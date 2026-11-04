fn expected(cell:&AtomicI32)->i32 {
    atomic_store(cell,5);
    return 5;
}
fn desired(cell:&AtomicI32)->i32 {
    return atomic_fetch_add(cell,1)+10;
}
fn main()->i32 {
    let(ok,value)=atomic_scope(0,(cell:&AtomicI32)->i32=>{
        let(observed,changed)=atomic_compare_exchange(cell,expected(cell),desired(cell));
        if observed!=6 || changed || atomic_load(cell)!=6 {return 2}
        return 0;
    });
    if !ok {return 1} return value;
}
