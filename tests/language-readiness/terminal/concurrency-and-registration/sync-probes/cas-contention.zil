fn increment(cell:&AtomicI32)->i32 {
    var count=0
    var expected=atomic_load(cell)
    while count<128 {
        let (observed,exchanged)=atomic_compare_exchange(cell,expected,expected+%1)
        if exchanged {count+=1;expected=observed+%1} else {expected=observed}
    }
    return 0
}
fn main()->i32 {
    let (ok,value)=atomic_scope(0,(cell:&AtomicI32)->i32=>{
        let (created,child,parent)=thread_scope(increment,cell,()->i32=>increment(cell))
        if !created {return 2} return atomic_load(cell)-256+child+parent
    })
    if !ok {return 1} return value
}
