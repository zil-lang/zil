extern "C" fn putchar(value:i32)->i32;
struct Cell { value:i32; other:i32 }
fn bump(value:&mut Cell, amount:i32)->i32 { value.value+=amount; return value.value }
fn relay(value:&mut Cell)->i32 { return bump(value,3) }
fn main()->i32 {
    let left:Store<Cell>=store_new()
    let right:Store<Cell>=store_new()
    if !store_reserve(left,3) || !store_reserve(right,2) { return 1 }
    store_push(left,Cell{value:40,other:7})
    store_push(right,Cell{value:70,other:9})
    let view=store_at_mut(left,0)
    view.value=41
    if relay(view)!=44 || bump(view,2)!=46 || view.other!=7 { return 2 }
    let separate=store_at_mut(right,0)
    separate.value=71
    if separate.value!=71 || view.value!=46 { return 3 }
    if !store_reserve(left,4) { return 4 }
    let reread=store_at(left,0)
    if reread.value!=46 || reread.other!=7 { return 5 }
    unsafe { putchar(77);putchar(85);putchar(84);putchar(10) }
    return 0
}
