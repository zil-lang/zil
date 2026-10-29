resource R{n:i32}fn main()->i32{let r=R{n:1};let d=r;let n=42;let q=&n;let f=()=>{d.n=43;return *q};let other=R{n:9};let p=&other;return f()+p.n-51}
