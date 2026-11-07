extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages first:R;manages second:R;n:i32}
enum E{Left(P),Right(P)}
fn reset(target:&mut E,left:bool){*target=if left{E.Left(P{first:R{code:67},second:R{code:68},n:42})}else{E.Right(P{first:R{code:69},second:R{code:70},n:43})}}
fn work(left:bool)->i32{var value=E.Left(P{first:R{code:65},second:R{code:66},n:0});reset(&mut value,left);match value{E.Left(packet)=>{return packet.n},E.Right(packet)=>{return packet.n}}}
fn main()->i32{let a=work(true);let b=work(false);unsafe{putchar(48+a/10);putchar(48+a%10);putchar(44);putchar(48+b/10);putchar(48+b%10);putchar(10)}return 0}
