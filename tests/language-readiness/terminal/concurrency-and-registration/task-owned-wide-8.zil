resource Token{code:i32}
fn consume(give a0:Token,give a1:Token,give a2:Token,give a3:Token,give a4:Token,give a5:Token,give a6:Token,give a7:Token){}
fn worker(give a0:Token,give a1:Token,give a2:Token,give a3:Token,give a4:Token,give a5:Token,give a6:Token,give a7:Token)->i32 contract{effects(suspend);resume(local)}{
    consume(give a0,give a1,give a2,give a3,give a4,give a5,give a6,give a7);
    task_yield();
    return 0;
}
fn main()->std.task.TaskCreateError!i32{
    let a0=Token{code:0};
    let a1=Token{code:1};
    let a2=Token{code:2};
    let a3=Token{code:3};
    let a4=Token{code:4};
    let a5=Token{code:5};
    let a6=Token{code:6};
    let a7=Token{code:7};
    var task=task_create(worker,give a0,give a1,give a2,give a3,give a4,give a5,give a6,give a7)?;
    task_poll(&mut task);
    task_poll_close(&mut task);
    task_finish_destroy(give task);
    return 0;
}
