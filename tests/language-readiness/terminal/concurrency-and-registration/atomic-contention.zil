fn worker(cell:&AtomicI32)->i32 {
    var index=0
    while index<25000 {
        atomic_fetch_add(cell,1)
        index+=1
    }
    return 0
}
fn main()->i32 {
    let(initialized, result)=atomic_scope(0,(cell:&AtomicI32)->i32=>{
        let(created, child, parent)=thread_scope(worker,cell,()->i32=>{
            var index=0
            while index<25000 {
                atomic_fetch_add(cell,1)
                index+=1
            }
            return 0
        })
        if !created { return 2 }
        return atomic_load(cell)-50000+child+parent
    })
    if !initialized { return 1 }
    return result
}
