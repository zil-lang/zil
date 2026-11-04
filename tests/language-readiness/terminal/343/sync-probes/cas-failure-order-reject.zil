fn main()->i32 {
    let (ok,value)=atomic_scope(0,(cell:&AtomicI32)->i32=>{
        let (old,changed)=atomic_compare_exchange_ordered(cell,0,1,MemoryOrder.Release,MemoryOrder.Acquire)
        if changed {return old} return 0
    })
    if !ok {return 1} return value
}
