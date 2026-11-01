
extern "C" fn putchar(code:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages ticket:Ticket;value:i32}
package Bag{manages items:Store<Item>}
package Pair{left:Bag;right:Bag}
fn bag(code:i32)->Bag{
 let items:Store<Item>=store_new();
 if store_reserve(items,1){store_push(items,Item{ticket:Ticket{code:code},value:code})}
 return Bag{items:give items}
}
fn pair()->Pair{return Pair{left:bag(65),right:bag(66)}}
fn main()->i32{var value=pair();var child=value.left;let popped=store_pop(child.items);store_push(child.items,Item{ticket:Ticket{code:67},value:67});store_push(child.items,Item{ticket:Ticket{code:68},value:68});value.left=child;return 0}