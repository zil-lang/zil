fn worker(value:i32)->i32 { return value }
fn main()->i32 { let result=#(thread_scope(worker,1,()->i32=>0));return 0 }
