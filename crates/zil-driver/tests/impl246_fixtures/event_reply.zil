package Event{manages payload:ByteBuffer;tag:i32}package Reply{manages payload:ByteBuffer;tag:i32}
fn dispatch<F:FnOnce(Event)->Reply>(f:F,event:Event)->Reply{return f(event)}
fn main()->i32{let bytes=bytes_new();if !bytes_reserve(bytes,3){return 1}let handler=(event:Event)->Reply=>{return Reply{payload:give event.payload,tag:event.tag+1}};let reply=dispatch(handler,Event{payload:give bytes,tag:65});return reply.tag-66}
