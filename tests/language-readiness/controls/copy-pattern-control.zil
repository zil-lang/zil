enum E{A(i32),B}
fn main()->i32{let e=E.A(7);match e{E.A(n)=>{return n-7},E.B()=>{return 1}}}
