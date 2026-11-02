extern "C" fn reg(cb:extern "C" fn(c_handle,i32)->i64,data:c_handle)->c_handle contract{provides(static_callbacks(),scoped_callback_register());effects(ffi);};
extern "C" fn close(token:c_handle) contract{provides(static_callbacks(),scoped_callback_close());effects(ffi);};
resource Mark{n:i32}
package Event{manages mark:Mark;manages bytes:ByteBuffer}
enum Fault{NoMemory}
fn no_memory()->Fault!i32{return Fault.NoMemory}
fn consume(event:Event)->i32{return event.mark.n+i32(bytes_len(event.bytes))}
package Reply{manages mark:Mark;manages bytes:ByteBuffer}
fn reply(event:Event)->Reply{return Reply{mark:give event.mark,bytes:give event.bytes}}
fn render(reply:Reply)->i32{return reply.mark.n+i32(bytes_len(reply.bytes))}
fn dispatch<H:FnOnce(Event)->Reply>(handler:H,event:Event)->Reply{return handler(event)}
fn prepare()->Fault!Event{
    let mark=Mark{n:3}
    let bytes=bytes_new()
    if !bytes_reserve(bytes,1){return Fault.NoMemory}
    bytes_push(bytes,86u8)
    return Event{mark:give mark,bytes:give bytes}
}
fn run()->Fault!i32{
    var f=(x:i32)->i32=>x+1
    let event=prepare()?
    let body=move ()->i32=>{
        let handler=(event:Event)->Reply=>reply(event)
        let result=dispatch(handler,event)
        return render(result)
    }
    unsafe{let result=c_with_callback(reg,close,&mut f,body);if !result.0{return no_memory()};return result.1}
}
fn main()->Fault!i32{
    var f=(x:i32)->i32=>x+1
    let value=run()?
    if f(4)!=5{return 99}
    return value-4
}
