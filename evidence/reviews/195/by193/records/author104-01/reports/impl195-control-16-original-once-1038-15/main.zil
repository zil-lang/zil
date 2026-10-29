fn run<F:FnOnce()->i32>(f:F,n:i32)->i32{var total=0;for index in 0..n{total+=f();continue}return total}fn main()->i32{let f=move()=>1;return run(f,2)}
