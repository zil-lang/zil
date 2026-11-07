extern "C" fn putchar(code:i32)->i32;
fn worker(input:&i32)->i32 {
    let first=*input;
    task_yield();
    return first+*input;
}
fn make<life a>(input:&'a i32)->TaskCreateError!Task<i32> contract { result(create) } {
    let other=100;
    let ignored=&other;
    return task_create(worker,input);
}
fn main()->TaskPollState!i32 {
    var value=32;
    match make(&value) {
        .ok(created)=>{
            var task=give created;
            match task_poll(&mut task)? {
                Poll.Pending()=>{unsafe{putchar(80);}},
                Poll.Ready(_)=>{return 91;}
            }
            match task_poll(&mut task)? {
                Poll.Pending()=>{return 92;},
                Poll.Ready(n)=>{if n!=64{return 93;}unsafe{putchar(82);}}
            }
            match task_poll_close(&mut task) {
                Poll.Pending()=>{return 94;},
                Poll.Ready(_)=>{unsafe{putchar(67);}}
            }
            task_finish_destroy(give task);
        },
        .err(_)=>{value=65;return 95;}
    }
    value=65;
    unsafe{putchar(value);}
    return 0;
}
