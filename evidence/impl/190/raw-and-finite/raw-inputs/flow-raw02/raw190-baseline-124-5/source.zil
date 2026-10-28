fn leaf0<F:Fn()->i32>(f:&F,n:i32)->i32{var i=0;var sum=0;while i<n{sum+=f();i+=1}return sum}fn leaf1<F:Fn()->i32>(f:&F,n:i32)->i32=>leaf0(f,n)+leaf0(f,n)
fn leaf2<F:Fn()->i32>(f:&F,n:i32)->i32=>leaf1(f,n)+leaf1(f,n)
fn leaf3<F:Fn()->i32>(f:&F,n:i32)->i32=>leaf2(f,n)+leaf2(f,n)
fn leaf4<F:Fn()->i32>(f:&F,n:i32)->i32=>leaf3(f,n)+leaf3(f,n)
fn leaf5<F:Fn()->i32>(f:&F,n:i32)->i32=>leaf4(f,n)+leaf4(f,n)
fn main()->i32{let f=move()=>1;return leaf5(&f,1)-32}