fn worker(seed:i32)->i32 {
    var x=seed
    task_yield()
    x=x+2
    task_yield()
    return x+3
}
fn main()->i32 {
    var task=task_start(worker,7)
    if !task_valid(task) { return 10 }
    let (s0,v0)=task_poll(&mut task)
    if s0!=0 || v0!=0 { return 11 }
    let (s1,v1)=task_poll(&mut task)
    if s1!=0 || v1!=0 { return 12 }
    let (s2,v2)=task_poll(&mut task)
    if s2!=1 || v2!=12 { return 13 }
    let (s3,v3)=task_poll(&mut task)
    if s3!=2 || v3!=0 { return 14 }
    if !task_poll_close(&mut task) { return 15 }
    task_finish_destroy(give task)
    return 0
}
