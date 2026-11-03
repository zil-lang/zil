extern "C" fn putchar(code:i32)->i32;
enum Choice<T>{None,Some(T)}
fn cleanup(){unsafe{putchar(69)}}
fn mark()->i32{unsafe{putchar(66)}return 0}
fn run<F:Fn()->i32>(f:F)->Choice<i32>!i32{f();let value:Choice<i32>!i32=(7);f();return value}
fn main()->Choice<i32>!i32{unsafe{putchar(65)}defer{cleanup()}let f=()->i32=>mark();let value=run(f)?;unsafe{putchar(68);putchar(48+value)}return value-7}
