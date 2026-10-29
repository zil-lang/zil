extern "C" fn putchar(n:i32)->i32;
resource R{code:i32;n:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
fn invoke<F:FnMut()->i32>(f:&mut F)->i32=>f();
package Pair{manages left:R;manages right:R}fn main()->i32{var p=Pair{left:R{code:65,n:21},right:R{code:66,n:20}};let d=p.left;let f=()=>d.n;p.left=R{code:67,n:1};return f()-21}