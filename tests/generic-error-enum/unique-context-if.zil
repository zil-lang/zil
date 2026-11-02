enum Choice<T>{None,Some(T)}
fn run<F:Fn()->i32>(f:F)->Choice<i32>!i32{return if f()!=0{Choice.None}else{Choice.None}}
fn main()->i32{let f=()->i32=>1;run(f);return 0}
