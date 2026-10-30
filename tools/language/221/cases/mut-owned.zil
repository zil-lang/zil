extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){unsafe{putchar(code)}}
resource Marker{code:i32}
impl Drop for Marker{fn drop(self:Self){emit(.code)}}
fn make(give marker:Marker)->impl FnMut()->i32{
    return move()=>{marker.code+=1;return marker.code}
}
fn mutable<F:FnMut()->i32>(f:F)->impl FnMut()->i32{
    var inner=f
    return move()=>inner()
}
fn main()->i32{
    var f=mutable(make(Marker{code:65}))
    if f()+f()!=133{return 9}
    emit(88)
    return 0
}
