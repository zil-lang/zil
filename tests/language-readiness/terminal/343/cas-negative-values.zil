fn main()->i32{
    let(ok,value)=atomic_scope(-7,(cell:&AtomicI32)->i32=>{
        let(old,changed)=atomic_compare_exchange(cell,-7,-9);
        if old != -7 || !changed{return 2}
        let(observed,wrong)=atomic_compare_exchange(cell,-7,9);
        if observed != -9 || wrong || atomic_load(cell) != -9{return 3}
        return 0;
    });
    if !ok{return 1} return value;
}
