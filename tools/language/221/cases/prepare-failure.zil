extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){unsafe{putchar(code)}}
resource Marker{code:i32}
impl Drop for Marker{fn drop(self:Self){emit(.code)}}
fn make(give marker:Marker)->impl Fn()->i32{return move()=>marker.code}
fn wrap<F:Fn()->i32>(f:F,bias:i32)->impl Fn()->i32{return move()=>f()+bias}
fn prepare()->i64!i32{
    emit(80)
    let bytes=bytes_new()
    if !bytes_reserve(bytes,32){return i64(1)}
    emit(83)
    return 0
}
fn run()->i64!i32{
    let saved=wrap(make(Marker{code:65}),prepare()?)
    return saved()-65
}
fn main()->i32{let result=run();emit(88);return 0}
