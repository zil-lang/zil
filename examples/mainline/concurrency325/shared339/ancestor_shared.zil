fn worker(input:&i32)->i32 { return *input+2 }
fn call(input:&mut i32)->i32 {
    let shared=&*input
    let body=()->i32=>*shared
    let(created,child,parent)=thread_scope(worker,shared,body)
    if !created { return -100 }
    *input=13
    return child+parent
}
fn main()->i32 { var value=9;let result=call(&mut value);return result+value-33 }
