
fn leaf<T:Copy,F:Fn(T)->T>(f:&F,x:T)->T=>f(x)
fn run<T:Copy,F:Fn(T)->T>(f:&F,x:T,n:i32)->T{var out=x;var i=n;while i>0{out=leaf(f,out);i-=1}return out}
fn main()->i32{let f=move(n:i32)=>n+1;return run(&f,1,0)+run(&f,2,1)+run(&f,3,7)-14}
