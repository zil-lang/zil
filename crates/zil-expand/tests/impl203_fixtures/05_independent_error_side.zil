
fn identity<T:StoreElement>(value:T)->T{return value}
fn empty<T:StoreElement>()->Store<T> contract{result(create)}{return store_new()}
fn reserve<T:StoreElement>(items:Store<T>,count:i64)->bool{return store_reserve(items,count)}
fn push<T:StoreElement>(items:Store<T>,value:T)->T!void{return store_push(items,value)}
fn pop<T:StoreElement>(items:Store<T>)->void!T{return store_pop(items)}
extern "C" fn putchar(value: i32) -> i32;
resource Ticket { code: i32 }
impl Drop for Ticket { fn drop(self: Self) { unsafe { putchar(.code) } } }
package State<T,F,G> { manages marker: Ticket; first: F; second: G; tag: T }
fn make(label: i32, value: i32) -> impl Fn() -> i32 {
    let ticket = Ticket { code: label }
    return move () => { ticket.code; return value }
}
fn pack<T: Copy, F: Fn() -> i32, G: FnOnce() -> i32>(first: F, second: G, tag: T) -> State<T,F,G> {
    return State::<T,F,G> { marker: Ticket { code: 90 }, first: first, second: second, tag: tag }
}

fn error_route<T:StoreElement>(value:T!void)->T!void{return value}
fn run<T:Copy,F:Fn()->i32,G:FnOnce()->i32>(state:State<T,F,G>)->State<T,F,G>!i32{
    let items:Store<State<T,F,G>>=empty()
    let full:State<T,F,G>!void=push(items,state)
    let routed:State<T,F,G>!void=error_route::<State<T,F,G>>(full)
    routed?
    return 0
}
fn main()->i32{let result=run(pack(make(65,10),make(66,20),17));return 0}
