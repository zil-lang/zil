extern "C" fn putchar(n:i32)->i32;
resource R{code:i32;n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
fn invoke<F:FnMut()->i32>(f:&mut F)->i32=>f();
fn main()->i32{let a=R{code:65,n:20};let d=a;let h=&a;let q=&h.n;let f=()=>{d.n=42;return *q};return f()}