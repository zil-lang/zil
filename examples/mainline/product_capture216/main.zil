extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){unsafe{putchar(code)}}
resource Marker{code:i32}
impl Drop for Marker{fn drop(self:Self){emit(.code)}}
package Leaf{manages first:Marker;manages second:Marker;n:i32}
package Node{manages lead:Marker;child:Leaf;manages tail:Marker;tag:i32}
package Root{child:Node;tag:i32}
package State<F>{manages bytes:ByteBuffer;callback:F}

fn root(base:i32)->Root{
    return Root{child:Node{lead:Marker{code:base},
        child:Leaf{first:Marker{code:base+1},second:Marker{code:base+2},n:base},
        tail:Marker{code:base+3},tag:base},tag:base}
}
fn make(input:Root)->impl Fn()->i32{
    return move()=>input.tag
}
fn pack<F:Fn()->i32>(callback:F)->State<F>{
    return State::<F>{bytes:bytes_new(),callback:callback}
}
fn run<F:Fn()->i32>(first:State<F>,second:State<F>)->i32{
    let items:Store<State<F>>=store_new()
    if !store_reserve(items,2){return 1}
    store_push(items,first)
    store_push(items,second)
    if store_at(items,0).callback()!=65 || store_at(items,1).callback()!=69{return 8}
    if !store_reserve(items,4){return 2}
    if store_at(items,1).callback()!=69 || store_at(items,0).callback()!=65{return 8}
    return 0
}
fn main()->i32{
    return run(pack(make(root(65))),pack(make(root(69))))
}
