extern "C" fn putchar(n:i32)->i32;
resource R{code:i32;n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
fn invoke<F:FnMut()->i32>(f:&mut F)->i32=>f();
fn hidden(edit:R){let f=()=>{edit.n+=1};f()}fn second(edit:R){hidden(edit)}fn main()->i32{let a=R{code:65,n:20};let b=R{code:66,n:21};let h=&a;second(b);return h.n+b.n-42}