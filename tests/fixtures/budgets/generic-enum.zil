enum E<T>{V(T),Nil}
fn main()->i32{let e=E.V(3);match e{E.V(n)=>{return n}
E.Nil()=>{return 0}}}
