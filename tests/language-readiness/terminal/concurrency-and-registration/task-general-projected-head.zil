package Holder { pub task:Task<i32> }
fn worker(x:i32)->i32 { return x; }
fn main()->TaskCreateError!i32 {
    let task=task_create(worker,42)?;
    let holder=Holder{task:give task};
    if task_valid(holder.task) {return 0;}
    return 1;
}
