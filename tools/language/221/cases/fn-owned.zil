extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){unsafe{putchar(code)}}
resource Marker{code:i32}
impl Drop for Marker{fn drop(self:Self){emit(.code)}}
fn make(give marker:Marker)->impl Fn()->i32{return move()=>marker.code}
fn wrap<F:Fn()->i32>(f:F)->impl Fn()->i32{return move()=>f()}
fn main()->i32{
    let unused=wrap(make(Marker{code:66}))
    let used=wrap(make(Marker{code:65}))
    if used()+used()!=130{return 9}
    emit(88)
    return 0
}
