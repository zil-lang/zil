extern "C" fn putchar(n:i32)->i32;
resource R{code:i32;n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
fn invoke<F:FnMut()->i32>(f:&mut F)->i32=>f();
fn main()->i32{let a=R{code:65,n:20};let d=a;let h=&a;var n=0;var f=()=>{n+=1;return d.n+n};let answer=invoke(&mut f);return answer+h.n+n-42}