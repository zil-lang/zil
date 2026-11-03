fn worker(value:i32)->i32 { return value }
fn main()->i32 { let body=(value:i32)->i32=>value;let result=thread_scope(worker,1,body);return result.1 }
