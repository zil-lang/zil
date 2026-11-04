fn main()->i32 {
    let(initialized, result)=mutex_scope(7,(cell:&MutexI32)->i32=>{
        let old=mutex_fetch_add(cell,5)
        mutex_store(cell,mutex_load(cell)+1)
        return old+mutex_load(cell)-20
    })
    if !initialized { return 1 }
    return result
}
