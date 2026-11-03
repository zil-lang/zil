extern "C" fn putchar(code:i32)->i32;
enum Choice<T>{None,Some(T)}
fn mark()->i32{unsafe{putchar(66)}return 7}
fn sink(value:Choice<i32>!i32)->Choice<i32>!i32{unsafe{putchar(67)}return value}
fn run<F:Fn()->i32>(f:F)->Choice<i32>!i32{return sink(Choice::<i32>.Some(mark()))}
fn main()->Choice<i32>!i32{unsafe{putchar(65)}let f=()->i32=>mark();let value=run(f)?;unsafe{putchar(68)}return value}
