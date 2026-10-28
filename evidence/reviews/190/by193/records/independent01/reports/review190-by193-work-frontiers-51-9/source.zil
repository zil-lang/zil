fn level0<F:Fn()->i32>(f:&F,n:i32)->i32{var i=0;var sum=0;while i<n{sum+=f();i+=1}return sum}
fn level1<F:Fn()->i32>(f:&F,n:i32)->i32=>level0(f,n)
fn level2<F:Fn()->i32>(f:&F,n:i32)->i32=>level1(f,n)
fn level3<F:Fn()->i32>(f:&F,n:i32)->i32=>level2(f,n)
fn main()->i32{let f=move()=>1;return level3(&f,20000)-20000}