
extern "C" fn putchar(code:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Leaf{manages ticket:Ticket;value:i32}
package Item{child:Leaf;tag:i32}
 fn run(flag:bool)->i32{
        let item=Item{child:Leaf{ticket:Ticket{code:65},value:7},tag:3};let other=Item{child:Leaf{ticket:Ticket{code:66},value:8},tag:4};let items:Store<Item>=store_new();store_reserve(items,1);
        store_push(items,Item{child:Leaf{ticket:Ticket{code:67},value:9},tag:5});
        let window=store_at(items,0);let view=if flag{&item}else{&other};
        let callback=()=>view.child.value;return callback()
        } fn main()->i32{return run(true)-7}