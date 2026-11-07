resource R{n:i32}fn main()->i32{let r=R{n:42};let d=r;let f=()=>d.n;let p=&r;return f()+p.n-84}
