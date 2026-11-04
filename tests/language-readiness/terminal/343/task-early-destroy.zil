fn worker(seed:i32)->i32 {task_yield(); return seed}
fn main()->i32 {
    var task=task_start(worker,7)
    if !task_valid(task) {return 10}
    task_poll(&mut task)
    task_finish_destroy(give task)
    return 11
}
