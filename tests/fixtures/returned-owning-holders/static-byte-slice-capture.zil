fn make()->impl Fn()->u8{let text="abc";return move()=>text[0]}
fn main()->i32{let f=make();return i32(f())-97}
