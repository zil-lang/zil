fn main()->i32 {
    let(initialized, result)=atomic_scope(7,(cell:&AtomicI32)->i32=>{
        let old=atomic_fetch_add(cell,5)
        atomic_store(cell,atomic_load(cell)+1)
        return old+atomic_load(cell)-20
    })
    if !initialized { return 1 }
    return result
}
