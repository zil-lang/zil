extern "C" fn worker(value:i32)->i32;
fn main()->i32 { let body=()->i32=>0;let result=thread_scope(worker,1,body);return result.1 }
