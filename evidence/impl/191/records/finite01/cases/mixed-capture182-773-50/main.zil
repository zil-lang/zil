
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32;n:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
fn write(d:R,n:i32)->i32{d.n=n;return n}
fn apply<F:Fn()->i32>(f:&F)->i32{return f()}
fn apply_mut<F:FnMut()->i32>(f:&mut F)->i32{return f()}
fn once<F:FnOnce()->i32>(f:F)->i32{return f()}
fn main()->i32{let a=R{code:65,n:20};let d=a;let h=&a;var n=0;var f=()=>{n+=1;d.n+=1;return d.n+n};let answer=apply_mut(&mut f);return answer+h.n+n-42}