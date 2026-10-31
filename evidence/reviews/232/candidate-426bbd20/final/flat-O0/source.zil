
extern "C" fn putchar(code:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Leaf{manages ticket:Ticket;value:i32}
package Item{child:Leaf;tag:i32}
 fn main()->i32{let owner=Leaf{ticket:Ticket{code:65},value:7};let callback=()=>owner.value+owner.ticket.code;let n=callback();let moved=owner;return n-72}