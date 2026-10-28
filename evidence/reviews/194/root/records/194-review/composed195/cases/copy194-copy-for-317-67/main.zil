
fn run<T:Copy,F:Fn(T)->T>(f:&F,x:T,n:i32)->T{var out=x;for i in 0..n{out=f(out)}return out}
fn main()->i32{let f=move(n:i32)=>n+1;return run(&f,3,4)-7}
