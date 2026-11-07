resource R{n:i32}fn apply<F:Fn()->i32>(f:&F,p:&R)->i32{return f()+p.n}fn main()->i32{let r=R{n:42};let d=r;let f=()=>{d.n=43;return 0};return apply(&f,&r)-43}
