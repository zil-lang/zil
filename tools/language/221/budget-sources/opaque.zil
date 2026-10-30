resource Token{n:i32}
fn make(give token:Token)->impl Fn()->i32{return move()=>token.n}
fn main()->i32{let f=make(Token{n:21});let n=f();let g=f;return n+g()-42}
