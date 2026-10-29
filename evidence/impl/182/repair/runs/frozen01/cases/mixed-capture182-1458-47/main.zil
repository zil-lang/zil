resource R{n:i32}fn hidden(d:R)->i32{let f=()=>d.n;return f()}fn relay(d:R)->i32{return hidden(d)}fn main()->i32{let r=R{n:42};let p=&r;return relay(r)+p.n-84}
