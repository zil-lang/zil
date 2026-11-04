fn worker(seed:i32)->i32 {
    var total=seed
    var index=0
    while index<4 {
        if index%2==0 {
            total=total+index
            task_yield()
            total=total+10
        } else {
            total=total+20
            task_yield()
            total=total+index
        }
        index=index+1
    }
    return total
}
fn main()->i32 {
    var task=task_start(worker,7)
    if !task_valid(task) {return 10}
    var count=0
    while count<4 {
        let (status,value)=task_poll(&mut task)
        if status!=0 || value!=0 { return 11 }
        count=count+1
    }
    let (status,value)=task_poll(&mut task)
    if status!=1 || value!=73 { return 12 }
    if !task_poll_close(&mut task) {return 13}
    task_finish_destroy(give task)
    return 0
}
