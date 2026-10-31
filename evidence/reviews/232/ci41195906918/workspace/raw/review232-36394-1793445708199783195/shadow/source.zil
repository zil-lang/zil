
extern "C" fn putchar(code:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Leaf{manages ticket:Ticket;value:i32}
package Item{child:Leaf;tag:i32}
 fn main()->i32{let owner=Item{child:Leaf{ticket:Ticket{code:65},value:7},tag:3};let callback=()=>owner.child.value;if true{let owner=Item{child:Leaf{ticket:Ticket{code:66},value:8},tag:4};return callback()+owner.child.value-15}else{return 1}}