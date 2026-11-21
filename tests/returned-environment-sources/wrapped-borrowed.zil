
fn wrap<F:Fn()->i32>(input:F)->impl Fn()->i32{return move()=>input()}
fn main()->i32{let value=41;let pointer=&value;let inner=move()=>*pointer;let outer=wrap(inner);return outer()-41}
