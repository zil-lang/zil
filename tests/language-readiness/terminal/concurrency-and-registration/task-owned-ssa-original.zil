resource Token { code:i32 }
fn child(x:i32)->i32 { task_yield(); return x; }
fn worker(give token:Token)->i32 contract { effects(suspend); resume(local) } {
    let prepared=(give token,task_await(child,42));
    return prepared.1;
}
fn main()->std.task.TaskCreateError!i32 {
    let input=Token{code:65};
    var task=task_create(worker,give input)?;
    task_request_cancel(&mut task);
    task_poll_close(&mut task);
    task_finish_destroy(give task);
    return 0;
}
