fn try_increment(cell:&AtomicI32,expected:i32)->(i32,bool){return atomic_compare_exchange(cell,expected,expected+1)}
fn worker(cell:&AtomicI32)->i32{
    var index=0;var expected=atomic_load(cell);
    while index<25000 {
        let (observed,won)=try_increment(cell,expected);
        if won {index+=1;expected=observed+1} else {expected=observed}
    }
    return 0;
}
fn main()->i32{
    let(ok,value)=atomic_scope(0,(cell:&AtomicI32)->i32=>{
        let(created,child,parent)=thread_scope(worker,cell,()->i32=>worker(cell));
        if !created{return 2} return atomic_load(cell)-50000+child+parent;
    });
    if !ok{return 1} return value;
}
