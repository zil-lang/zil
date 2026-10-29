
struct A{n:i32}struct B{n:i32}type Alias=A;
fn keep<T:Copy,F:Fn()->i32>(f:&F,x:T)->T{f();return x}
fn main()->i32{let f=move()=>1;let a=keep(&f,A{n:3});let b=keep(&f,B{n:5});
let again:Alias=keep(&f,A{n:7});let small=keep(&f,i32(11));let wide=keep(&f,i64(13));
return a.n+b.n+again.n+small+i32(wide)-39}
