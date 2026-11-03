enum E{A(i32),B}
fn choose(e:E)->i32{match e{E.A(n)=>{return n},E.B()=>{return 0}}}
fn main()->i32{return #(choose(E.A(3)))-3}
