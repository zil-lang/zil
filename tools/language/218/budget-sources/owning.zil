resource Token{n:i32}
fn make(give token:Token)->impl Fn()->i32{return move()=>token.n}
pub fn wrap<F:FnOnce()->i32>(f:F,bias:i32)->impl FnOnce()->i32{return move()=>f()+bias}
fn main()->i32{let output=wrap(make(Token{n:40}),2);return output()-42}
