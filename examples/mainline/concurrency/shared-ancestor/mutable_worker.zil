fn worker(input:&i32)->i32 {
    if false { var other=3;let writable=&mut other;*writable=4 }
    return *input
}
fn main()->i32 { let value=9;let result=thread_scope(worker,&value,()->i32=>0);return result.1 }
