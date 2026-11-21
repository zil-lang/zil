extern "C" fn putchar(n:i32)->i32;
resource Token{code:i32}
impl Drop for Token{fn drop(self:Self){unsafe{putchar(.code)}}}
fn make(value:&i32)->impl Fn()->i32{return move()=>*value}
package Packet{manages owner:Token;n:i32}
fn replace(packet:&mut Packet,give next:Token){packet.owner=give next}
fn main()->i32{
 let expected=42
 let read=make(&expected)
 var packet=Packet{owner:Token{code:65},n:42}
 replace(&mut packet,Token{code:66})
 return packet.n+read()-84
}
