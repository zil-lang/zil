extern "C" fn putchar(n:i32)->i32;fn emit(n:i32){unsafe{putchar(n)}}
fn start<T>(value:T)->i64{emit(76);return 0}
fn end()->i64{emit(72);return 3}
fn visit<T>(value:T)->i32{for index in start(value)..end(){emit(i32(index)+65)}return 0}
fn main()->i32{return visit(42)}
