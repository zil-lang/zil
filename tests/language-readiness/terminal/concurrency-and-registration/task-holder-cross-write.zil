struct Holder<life a> { pointer:&'a i32 }
fn hold<life a>(pointer:&'a i32)->Holder<'a> {
    return Holder::<'a>{pointer:pointer};
}
fn bump(first:&mut i32)->i32 {
    let holder = hold(&*first);
    let value = *holder.pointer;
    *first = 64;
    return value+1;
}
fn worker(x:i32)->i32 { task_yield(); return x; }
fn main()->TaskCreateError!i32 {
    var base = 31;
    let computed = bump(&mut base);
    var task = task_create(worker, computed)?;
    task_request_cancel(&mut task);
    match task_poll_close(&mut task) {
        Poll.Pending() => {return 99;},
        Poll.Ready(_) => {}
    }
    task_finish_destroy(give task);
    if computed==32 {return 0;}
    return 95;
}
