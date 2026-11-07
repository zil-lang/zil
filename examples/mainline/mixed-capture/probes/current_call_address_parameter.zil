resource R{n:i32}fn main()->i32{let r=R{n:42};let d=r;let f=(p:&R)=>{d.n=43;return p.n};return f(&r)-43}
