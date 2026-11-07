import api
extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){unsafe{putchar(code)}}
resource Marker{code:i32}
impl Drop for Marker{fn drop(self:Self){emit(.code)}}
fn make(give marker:Marker)->impl Fn(i32)->i32{return move(x:i32)=>marker.code+x}
fn main()->i32{
    let wrapped=api.wrap(2,api.wrap(1,make(Marker{code:65})))
    if wrapped(2)!=70{return 9}
    emit(88)
    return 0
}
