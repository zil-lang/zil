extern "C" fn reg(cb:extern "C" fn(c_handle,i32)->i64,data:c_handle)->c_handle contract{provides(static_callbacks(),scoped_callback_register());effects(ffi);};
extern "C" fn close(token:c_handle) contract{provides(static_callbacks(),scoped_callback_close());effects(ffi);};
extern "C" fn composition_drop(value:i32);
extern "C" fn composition_body(value:i32);
extern "C" fn composition_after(value:i32);
resource Mark{n:i32}
impl Drop for Mark{fn drop(self:Self){unsafe{composition_drop(.n)}}}
package Event{manages mark:Mark;manages bytes:ByteBuffer}
enum Fault{NoMemory}
fn no_memory()->Fault!i32{return Fault.NoMemory}
fn consume(event:Event)->i32{
    let value=event.mark.n+i32(bytes_len(event.bytes))
    unsafe{composition_body(value)}
    return value
}
fn prepare()->Fault!Event{
    let mark=Mark{n:3}
    let bytes=bytes_new()
    if !bytes_reserve(bytes,1){return Fault.NoMemory}
    bytes_push(bytes,86u8)
    return Event{mark:give mark,bytes:give bytes}
}
fn make_body(event:Event)->impl FnOnce()->i32{
    let tail=Mark{n:7}
    return move ()->i32=>{
        let result=consume(event)
        let remaining=tail.n
        if remaining!=7{return 102}
        return result
    }
}
fn run<F:FnMut(i32)->i32>(f:&mut F)->Fault!i32{
    let event=prepare()?
    let body=make_body(event)
    unsafe{let result=c_with_callback(reg,close,f,body);if !result.0{return no_memory()};return result.1}
}
fn main()->Fault!i32{
    var f=(x:i32)->i32=>x+1
    let outcome=run(&mut f)
    let after=f(4)
    if after!=5{return 99}
    unsafe{composition_after(after)}
    let value=outcome?
    return value-4
}
