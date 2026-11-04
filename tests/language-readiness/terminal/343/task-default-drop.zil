fn worker(seed:i32)->i32 {task_yield(); return seed+1}
fn main()->i32 {
    var task=task_start(worker,7)
    if !task_valid(task) {return 10}
    let (status,value)=task_poll(&mut task)
    if status!=0 || value!=0 {return 11}
    return 0
}
