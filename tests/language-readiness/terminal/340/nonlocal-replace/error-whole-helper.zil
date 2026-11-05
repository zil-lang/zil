extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages owner:R;n:i32}
enum Fault{Bad}
fn make(code:i32,n:i32,ok:bool)->Fault!P{if ok{return P{owner:R{code:code},n:n}}return Fault.Bad()}
fn reset(target:&mut (Fault!P),keep:bool){*target=make(66,42,keep)}
fn work(keep:bool)->i32{var packet=make(65,0,true);reset(&mut packet,keep);match packet{.ok(current)=>{return current.n},.err(_)=>{return 42}}}
fn main()->i32{let left=work(true);let right=work(false);unsafe{putchar(48+left/10);putchar(48+left%10);putchar(44);putchar(48+right/10);putchar(48+right%10);putchar(10)}return 0}
