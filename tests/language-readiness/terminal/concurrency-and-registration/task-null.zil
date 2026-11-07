fn worker(seed:i32)->i32 {task_yield(); return seed}
fn main()->i32 {
    var task=task_start(worker,7)
    if task_valid(task) {return 1}
    let (status,value)=task_poll(&mut task)
    if status!=4 || value!=0 {return 2}
    task_request_cancel(&mut task)
    if !task_poll_close(&mut task) {return 3}
    task_finish_destroy(give task)
    var dropped=task_start(worker,9)
    if task_valid(dropped) {return 4}
    return 0
}
