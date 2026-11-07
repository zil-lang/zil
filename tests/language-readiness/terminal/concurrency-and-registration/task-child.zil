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
fn worker(seed:i32)->i32 {
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
fn main()->i32 {
    var task=task_start(worker,7)
    if !task_valid(task) {return 10}
    var count=0
    while count<8 {
        let (status,value)=task_poll(&mut task)
        if status!=0 || value!=0 {return 11}
        count=count+1
    }
    let (status,value)=task_poll(&mut task)
    if status!=1 || value!=26 {return 12}
    task_request_cancel(&mut task)
    let (complete,empty)=task_poll(&mut task)
    if complete!=2 || empty!=0 {return 13}
    if !task_poll_close(&mut task) {return 14}
    task_finish_destroy(give task)
    return 0
}
