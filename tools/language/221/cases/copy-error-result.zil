extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){unsafe{putchar(code)}}
resource Marker{code:i32}
impl Drop for Marker{fn drop(self:Self){emit(.code)}}
fn make(give marker:Marker)->impl Fn()->i64!i32{
    return move()=>{if marker.code==65{return i64(1)};return marker.code}
}
fn once<F:FnOnce()->i64!i32>(f:F,give tail:Marker)->impl FnOnce()->i64!i32{
    return move()=>{let n=f()?;return n+tail.code}
}
fn failure()->i64!i32{
    let f=once(make(Marker{code:65}),Marker{code:66})
    let n=f()?
    emit(90)
    return n
}
fn success()->i64!i32{
    let f=once(make(Marker{code:67}),Marker{code:68})
    let n=f()?
    if n!=135{return i64(9)}
    emit(89)
    return 0
}
fn main()->i32{let failed=failure();emit(88);let succeeded=success();return 0}
