extern "C" fn putchar(code:i32)->i32;
resource Token{code:i32}
impl Drop for Token{fn drop(self:Self){unsafe{putchar(.code);}}}
fn worker(value:i32)->i32{let token=Token{code:65};return value;}
fn main()->std.task.TaskCreateError!i32 {
    worker(0);
    var task=task_create(worker,0)?;
    task_poll(&mut task);
    task_poll_close(&mut task);
    task_finish_destroy(give task);
    return 0;
}
