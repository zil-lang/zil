fn worker(seed:i32)->i32 {task_yield(); return seed+2}
fn make()->TaskI32 contract{result(create)} {return task_start(worker,7)}
fn relay(give task:TaskI32)->TaskI32 contract{result(give)} {return give task}
fn poll(task:&mut TaskI32)->(i32,i32) {return task_poll(task)}
fn main()->i32 {
    let first=make()
    var moved=relay(give first)
    if !task_valid(moved) {return 10}
    let (s0,v0)=poll(&mut moved)
    if s0!=0 || v0!=0 {return 11}
    let (s1,v1)=poll(&mut moved)
    if s1!=1 || v1!=9 {return 12}
    if !task_poll_close(&mut moved) {return 13}
    task_finish_destroy(give moved)
    return 0
}
