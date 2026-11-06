extern "C" fn putchar(value:i32)->i32;
struct Cell{value:i32;stamp:u64}
fn main()->i32{
    let left:Store<Cell>=store_new()
    let right:Store<Cell>=store_new()
    if !store_reserve(left,3) || !store_reserve(right,1){return 1}
    store_push(left,Cell{value:41,stamp:18446744073709551615u64})
    let view=store_at(left,0)
    if view.value!=41 || view.stamp!=18446744073709551615u64{return 2}
    // The actual view's last use precedes all three operations on its root.
    if !store_reserve(left,4){return 3}
    store_push(left,Cell{value:42,stamp:9u64})
    match store_pop(left){.ok(value)=>{if value.value!=42 || value.stamp!=9u64{return 4}},.err(())=>{return 5}}
    let live=store_at(left,0)
    // Keep this window live while all three operations target another root.
    if !store_reserve(right,3){return 6}
    store_push(right,Cell{value:43,stamp:10u64})
    match store_pop(right){.ok(value)=>{if value.value!=43 || value.stamp!=10u64{return 7}},.err(())=>{return 8}}
    if live.value!=41 || live.stamp!=18446744073709551615u64{return 9}
    var ordinary=Cell{value:1,stamp:2u64}
    let mutable=&mut ordinary
    mutable.value=3
    if ordinary.value!=3{return 10}
    unsafe{putchar(86);putchar(73);putchar(69);putchar(87);putchar(10)}
    return 0
}
