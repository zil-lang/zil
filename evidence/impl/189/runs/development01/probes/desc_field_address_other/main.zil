resource R{n:i32}fn main()->i32{let r=R{n:42};let d=r;let n=42;let f=(p:&i32)->i32=>{d.n=43;return *p};return f(&n)-42}
