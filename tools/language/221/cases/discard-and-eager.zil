extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){unsafe{putchar(code)}}
resource Marker{code:i32}
impl Drop for Marker{fn drop(self:Self){emit(.code)}}
fn make(give marker:Marker)->impl Fn()->i32{return move()=>marker.code}
fn discard<F:Fn()->i32>(f:F)->impl Fn()->i32{return move()=>7}
fn eager<F:FnOnce()->i32>(f:F)->impl Fn()->i32{
    let n=f()
    return move()=>n
}
fn main()->i32{
    let discarded=discard(make(Marker{code:65}))
    let called=eager(make(Marker{code:66}))
    if discarded()+called()!=73{return 9}
    emit(88)
    return 0
}
