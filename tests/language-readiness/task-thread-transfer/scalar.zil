fn compute(seed:i32)->i32 contract{resume(transfer)} {
    let saved=seed+1;
    task_yield();
    return saved+4;
}
fn drive(give input:Task<i32>)->i32 {
    var task=give input;
    var answer=0;
    loop {
        match task_poll(&mut task) {
            .ok(poll)=>{match poll{
                Poll.Pending()=>{continue;},
                Poll.Ready(value)=>{answer=value;break;}
            }},
            .err(_)=>{break;}
        }
    }
    task_request_cancel(&mut task);
    loop {match task_poll_close(&mut task){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}
    task_finish_destroy(give task);
    return answer;
}
fn main()->TaskCreateError!i32 {
    var task=task_create(compute,37)?;
    task_poll(&mut task);
    match thread_scope(drive,give task,()=>9) {
        TaskThreadScopeResult.Failed(input)=>{
            var remaining=give input;
            task_request_cancel(&mut remaining);
            loop {match task_poll_close(&mut remaining){Poll.Pending()=>{continue;},Poll.Ready(_)=>{break;}}}
            task_finish_destroy(give remaining);
            return 2;
        },
        TaskThreadScopeResult.Completed(answer,body)=>{return answer+body-51;}
    }
}
