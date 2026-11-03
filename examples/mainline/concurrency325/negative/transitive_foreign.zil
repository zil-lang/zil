extern "C" fn hidden()->i32;
fn helper(value:i32)->i32 { unsafe { return value+hidden() } }
fn worker(value:i32)->i32 { return helper(value) }
fn main()->i32 { let body=()->i32=>0;let result=thread_scope(worker,1,body);return result.1 }
