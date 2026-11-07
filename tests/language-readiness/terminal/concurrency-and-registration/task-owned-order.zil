extern "C" fn putchar(code:i32)->i32;
resource Token { code:i32 }
impl Drop for Token { fn drop(self:Self) { unsafe { putchar(.code); } } }
fn dispose(give token:Token) { }
fn work(flip:bool)->i32 contract { effects(suspend); resume(local) } {
    var a=Token{code:65};
    var b=Token{code:66};
    if flip {
        a=Token{code:67};
        b=Token{code:68};
    } else {
        b=Token{code:68};
        a=Token{code:67};
    }
    task_yield();
    return 0;
}
fn main()->std.task.TaskCreateError!i32 {
    var first=task_create(work,false)?;
    task_poll(&mut first);
    task_request_cancel(&mut first);
    task_poll_close(&mut first);
    task_finish_destroy(give first);
    var second=task_create(work,true)?;
    task_poll(&mut second);
    task_request_cancel(&mut second);
    task_poll_close(&mut second);
    task_finish_destroy(give second);
    return 0;
}
