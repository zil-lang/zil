extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages first:R;manages second:R;n:i32}
enum E{Left(P),Right(P)}
fn reset(target:&mut E,left:bool){*target=if left{E.Left(P{first:R{code:67},second:R{code:68},n:42})}else{E.Right(P{first:R{code:69},second:R{code:70},n:43})}}
fn bad(input:E)->R contract{result(create)}{var value=input;reset(&mut value,true);match value{E.Left(packet)=>{return give packet.first},E.Right(packet)=>{return give packet.first}}}
fn main()->i32{let result=bad(E.Left(P{first:R{code:65},second:R{code:66},n:0}));return result.code}
