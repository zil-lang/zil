fn worker(value:i32)->i32 { return value+2 }
fn run<F:FnOnce()->i32>(body:F)->i32 {
    let result=thread_scope(worker,5,body)
    return result.1+result.2
}
fn main()->i32 { let body=()->i32=>7;return run(body) }
