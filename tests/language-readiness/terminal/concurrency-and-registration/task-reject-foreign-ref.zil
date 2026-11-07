fn worker(value:i32)->i32 {task_yield(); return value+1}
fn main()->i32 {
    let callback=c_callback(worker)
    return 0
}
