extern "C" fn reg(cb:extern "C" fn(c_handle,i32)->i64,data:c_handle)->c_handle contract{provides(static_callbacks(),scoped_callback_register());effects(ffi);};
extern "C" fn close(token:c_handle) contract{provides(static_callbacks(),scoped_callback_close());effects(ffi);};
extern "C" fn preparation_drop(value:i32);
extern "C" fn preparation_body(value:i32);
extern "C" fn preparation_after(value:i32);
resource Mark{n:i32}
impl Drop for Mark{fn drop(self:Self){unsafe{preparation_drop(.n)}}}
package Event{manages mark:Mark;manages bytes:ByteBuffer}
enum Fault{NoMemory}
fn no_memory()->Fault!i32{return Fault.NoMemory}
fn consume(event:Event)->i32{
    let value=event.mark.n+i32(bytes_len(event.bytes))
    unsafe{preparation_body(value)}
    return value
}
fn run<F:FnMut(i32)->i32>(f:&mut F)->Fault!i32{
    let mark=Mark{n:3}
    let bytes=bytes_new()
    if !bytes_reserve(bytes,1){return no_memory()}
    bytes_push(bytes,86u8)
    let event=Event{mark:give mark,bytes:give bytes}
    let body=move ()->i32=>consume(event)
    unsafe{let result=c_with_callback(reg,close,f,body);return result.1}
}
fn main()->i32{
    var f=(x:i32)->i32=>x+1
    run(&mut f)
    let after=f(4)
    if after!=5{return 101}
    unsafe{preparation_after(after)}
    return 0
}
