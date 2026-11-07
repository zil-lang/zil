extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
package P{manages owner:R;n:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
enum F{Bad}
fn number(ok:bool)->F!i32{if ok{return 42}else{return F.Bad()}}
fn assemble(give first:R,give second:R,n:i32)->P{return P{owner:give second,n:n}}
fn reset(target:&mut P,ok:bool)->F!i32{*target=assemble(R{code:66},R{code:67},number(ok)?);return 7}
fn work()->i32{var packet=P{owner:R{code:65},n:0};match reset(&mut packet,true){.ok(returned)=>{let next=&mut packet;let current=next.owner;return next.n+current.code-67+returned-7},.err(_)=>{let next=&mut packet;let current=next.owner;return next.n+current.code-23}}}
fn show(n:i32){unsafe{putchar(48+n/10);putchar(48+n%10);putchar(10)}}
fn main()->i32{show(work());return 0}
