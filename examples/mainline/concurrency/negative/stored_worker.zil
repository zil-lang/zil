fn worker(value:i32)->i32 { return value }
fn main()->i32 { let stored=worker;let body=()->i32=>0;let result=thread_scope(stored,1,body);return result.1 }
