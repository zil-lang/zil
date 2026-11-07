resource R{n:i32}fn main()->i32{let r=R{n:1};let d=r;let n=42;let q=&n;let f=()=>{d.n=43;return *q};return f()-42}
