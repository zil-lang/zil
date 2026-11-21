fn make(pointer:&i32)->impl Fn()->i32{return move()=>*pointer}
fn keep<F:Fn()->i32>(input:F)->F{return input}
fn relay<F:Fn()->i32>(input:F)->F{return keep(input)}
fn main()->i32{let a=20;let b=22;let f=relay(make(&a));let g=relay(make(&b));return f()+g()-42}
