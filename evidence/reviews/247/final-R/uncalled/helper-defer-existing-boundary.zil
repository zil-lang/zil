extern "C" fn putchar(n:i32)->i32;
extern "C" fn getchar()->i32;
fn emit(n:i32){unsafe{putchar(n)}}
fn read()->i32{unsafe{return getchar()}}
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){emit(.code)}}
package Event{manages mark:Mark;code:i32}
fn dispatch<F:FnOnce(Event)->Event>(f:F,e:Event,run:bool)->Event{
    defer{emit(68)}
    if run{return f(e)}
    return e
}
fn main()->i32{
    let run=read()==49
    let active=Mark{code:72}
    let spare=Mark{code:104}
    let f=move(e:Event)->Event=>{emit(67);let held=give active;let seen=spare.code;return e}
    let result=dispatch(f,Event{mark:Mark{code:65},code:0},run)
    emit(82)
    return result.code
}
