extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){unsafe{putchar(code)}}
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){emit(.code)}}
fn keep<F:FnOnce(i32)->i32>(f:F)->F{return f}
fn outer<F:FnMut(i32)->i32>(f:F)->F{
    var current=keep(f)
    current(1)
    return current
}
fn main()->i32{
    let mark=Mark{code:77}
    var count=0
    var handler=move(delta:i32)->i32=>{
        let label=mark.code
        count+=delta
        emit(48+count)
        return count
    }
    var current=outer(handler)
    let second=current(1)
    let third=current(1)
    return second+third-5
}
