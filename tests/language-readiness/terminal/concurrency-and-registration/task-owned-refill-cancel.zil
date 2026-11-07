extern "C" fn putchar(code:i32)->i32;
resource Token{code:i32}
impl Drop for Token{fn drop(self:Self){unsafe{putchar(.code);}}}
fn worker()->i32 contract {effects(suspend);resume(local)} {
    var older=Token{code:65};
    let newer=Token{code:66};
    older=Token{code:67};
    task_yield();
    return 0;
}
fn main()->std.task.TaskCreateError!i32 {
    var task=task_create(worker)?;
    task_poll(&mut task);
    task_request_cancel(&mut task);
    task_poll_close(&mut task);
    task_finish_destroy(give task);
    return 0;
}
