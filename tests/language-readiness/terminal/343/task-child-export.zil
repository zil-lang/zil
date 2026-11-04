fn leaf(seed:i32)->i32 {
    var value=seed
    task_yield()
    value=value+1
    task_yield()
    return value+2
}
fn child(seed:i32)->i32 {
    let value=task_await(leaf,seed+1)
    task_yield()
    return value+4
}
pub fn worker(seed:i32)->i32 {
    var total=seed
    var index=0
    while index<3 {
        if index%2==0 {
            total=task_await(child,total)
        } else {
            total=task_await(leaf,total)
        }
        index=index+1
    }
    return total
}

extern "C" fn task_child_external()->i32;
fn main()->i32 {unsafe {return task_child_external()}}
