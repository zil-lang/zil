
extern "C" fn putchar(code:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Leaf{manages ticket:Ticket;value:i32}
package Item{child:Leaf;tag:i32}
 fn make()->impl Fn()->i32{
        let first:Store<Item>=store_new();store_reserve(first,1);store_push(first,Item{child:Leaf{ticket:Ticket{code:65},value:7},tag:3});
        let second:Store<Item>=store_new();store_reserve(second,1);store_push(second,Item{child:Leaf{ticket:Ticket{code:66},value:8},tag:4});
        return move()=>{let a=store_at(first,0);let b=store_at(second,0);return a.child.value*10+b.child.value}
        } fn main()->i32{let callback=make();return callback()-78}