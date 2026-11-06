import std.store
extern "C" fn putchar(value:i32)->i32;
struct Cell { value:i32; other:i32 }
fn add(value:&mut Cell, amount:i32)->i32 { value.value+=amount; return value.value }
fn main()->i32 {
    let items:Store<Cell>=store.new::<Cell>()
    if !store.reserve(items,2) { return 1 }
    store.push(items,Cell{value:40,other:8})
    let view=store_at_mut(items,0)
    if add(view,2)!=42 || view.other!=8 { return 2 }
    if store.len(items)!=1 || store.capacity(items)!=2 { return 3 }
    match store.pop(items) {
        .ok(value)=>{ if value.value!=42 || value.other!=8 { return 4 } },
        .err(())=>{ return 5 }
    }
    unsafe { putchar(83);putchar(84);putchar(68);putchar(77);putchar(10) }
    return 0
}
