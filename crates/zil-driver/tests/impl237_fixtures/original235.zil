
extern "C" fn putchar(code:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item<F>{manages ticket:Ticket;callback:F;value:i32}
package Packet<F>{manages marker:Ticket;callback:F}
package Bag<T:StoreElement>{manages items:Store<T>}
package Root<T:StoreElement>{child:Bag<T>}
fn wrap<T:StoreElement>(give items:Store<T>)->Root<T>{return Root::<T>{child:Bag::<T>{items:give items}}}
fn make()->impl Fn()->i32{let count=8;return move()=>count}
fn pack<F:Fn()->i32>(callback:F)->Packet<F>{return Packet::<F>{marker:Ticket{code:90},callback:callback}}
fn run<F:Fn()->i32>(packet:Packet<F>)->i32{
 let items:Store<Item<F>>=store_new();
 if !store_reserve(items,1){return 1}
 store_push(items,Item::<F>{ticket:Ticket{code:65},callback:packet.callback,value:3});
 let root=wrap(give items);let child=root.child;
 let element=store_at_mut(child.items,0);
 if element.callback()!=8{return 2}
 if element.callback()!=8{return 3}
 return 0
}
fn main()->i32{return run(pack(make()))}
