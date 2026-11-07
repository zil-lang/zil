import std.store
extern "C" fn putchar(value:i32)->i32;
struct Vertex{x:f32;y:f32;color:u64}
fn intact(value:Vertex,x:f32)->bool{return value.x==x && value.y== -2.5f32 && value.color==18446744073709551615u64}
fn main()->i32{
    let items:Store<Vertex>=store.new::<Vertex>()
    if store.len(items)!=0 || store.capacity(items)!=0{return 1}
    if !store.reserve(items,2){return 2}
    store.push(items,Vertex{x:1.25f32,y:-2.5f32,color:18446744073709551615u64})
    store.push(items,Vertex{x:3.75f32,y:-2.5f32,color:18446744073709551615u64})
    match store.push(items,Vertex{x:7.5f32,y:-2.5f32,color:18446744073709551615u64}){.ok(())=>{return 3},.err(value)=>{if !intact(value,7.5f32){return 4}}}
    if store.reserve(items,-1) || store.reserve(items,9223372036854775807){return 5}
    if store.len(items)!=2 || store.capacity(items)!=2{return 6}
    let view=store_at(items,0)
    if !intact(*view,1.25f32){return 7}
    if !store.reserve(items,3){return 8}
    match store.pop(items){.ok(value)=>{if !intact(value,3.75f32){return 9}},.err(())=>{return 10}}
    match store.pop(items){.ok(value)=>{if !intact(value,1.25f32){return 11}},.err(())=>{return 12}}
    match store.pop(items){.ok(_)=>{return 13},.err(())=>{}}
    if store.len(items)!=0 || store.capacity(items)!=3{return 14}
    unsafe{putchar(83);putchar(84);putchar(68);putchar(10)}
    return 0
}
