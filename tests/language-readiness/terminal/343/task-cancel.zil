fn worker(seed:i32)->i32 {
    task_yield()
    task_yield()
    return seed+1
}
fn main()->i32 {
    var before=task_start(worker,7)
    if !task_valid(before) {return 10}
    task_request_cancel(&mut before)
    task_request_cancel(&mut before)
    let (s0,v0)=task_poll(&mut before)
    if s0!=3 || v0!=0 {return 11}
    if !task_poll_close(&mut before) {return 12}
    task_finish_destroy(give before)
    var mid=task_start(worker,7)
    if !task_valid(mid) {return 13}
    let (s1,v1)=task_poll(&mut mid)
    if s1!=0 || v1!=0 {return 14}
    task_request_cancel(&mut mid)
    let (s2,v2)=task_poll(&mut mid)
    if s2!=3 || v2!=0 {return 15}
    if !task_poll_close(&mut mid) {return 16}
    task_finish_destroy(give mid)
    var after=task_start(worker,7)
    if !task_valid(after) {return 17}
    task_poll(&mut after)
    task_poll(&mut after)
    let (s3,v3)=task_poll(&mut after)
    if s3!=1 || v3!=8 {return 18}
    task_request_cancel(&mut after)
    let (s4,v4)=task_poll(&mut after)
    if s4!=2 || v4!=0 {return 19}
    if !task_poll_close(&mut after) {return 20}
    task_finish_destroy(give after)
    return 0
}
