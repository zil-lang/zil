extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){unsafe{putchar(code)}}
resource Marker{code:i32}
impl Drop for Marker{fn drop(self:Self){emit(.code)}}
package State<F>{manages bytes:ByteBuffer;callback:F}
fn make(give marker:Marker)->impl Fn()->i32{return move()=>marker.code}
fn wrap<F:Fn()->i32>(f:F)->impl Fn()->i32{return move()=>f()}
fn pack<F:Fn()->i32>(callback:F)->State<F>{
    return State::<F>{bytes:bytes_new(),callback:callback}
}
fn seed<T:StoreElement>(first:T,second:T)->Store<T> contract{result(create)}{
    let items:Store<T>=store_new()
    if !store_reserve(items,2){return give items}
    store_push(items,first)
    store_push(items,second)
    return give items
}
fn main()->i32{
    let items=seed(pack(wrap(make(Marker{code:65}))),pack(wrap(make(Marker{code:66}))))
    if store_len(items)!=2{return 1}
    if store_at(items,0).callback()!=65 || store_at(items,1).callback()!=66{return 8}
    if !store_reserve(items,4){return 2}
    if store_at(items,1).callback()!=66 || store_at(items,0).callback()!=65{return 8}
    return 0
}
