
resource Ticket{code:i32}
package Item<F>{manages ticket:Ticket;callback:F;value:i32}
package Packet<F>{manages marker:Ticket;callback:F}
fn make()->impl Fn()->i32{let count=8;return move()=>count}
fn pack<F:Fn()->i32>(callback:F)->Packet<F>{return Packet::<F>{marker:Ticket{code:90},callback:callback}}
fn run<F:Fn()->i32>(packet:Packet<F>)->i32{
 let items:Store<Item<F>>=store_new();store_reserve(items,1);
 store_push(items,Item::<F>{ticket:Ticket{code:65},callback:packet.callback,value:3});
 let element=store_at_mut(items,0);return element.callback()-8
}
fn main()->i32{return run(pack(make()))}
