fn worker(input:&i32)->i32 { return *input }
fn call(input:&mut i32)->i32 {
    let shared=&*input
    let body=()->i32=>{*input=13;return *input}
    let result=thread_scope(worker,shared,body)
    return result.1
}
fn main()->i32 { var value=9;return call(&mut value) }
