fn plain(value:i32)->i32 {return value+1}
fn main()->i32 {
    var task=task_start(plain,7)
    let ordinary=plain(7)
    task_request_cancel(&mut task)
    task_poll_close(&mut task)
    task_finish_destroy(give task)
    return ordinary-8
}
