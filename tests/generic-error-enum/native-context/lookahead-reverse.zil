extern "C" fn putchar(code:i32)->i32;
fn value()->i32{unsafe{putchar(66)}return 7}
fn witness()->bool{unsafe{putchar(67)}return true}
fn sink<T:Copy>(w:T,v:T!i32)->i32{return 0}
fn run<F:Fn()->i32>(f:F)->i32{return sink(witness(),f())}
fn main()->i32{let f=()->i32=>value();return run(f)}
