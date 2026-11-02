package Event{manages payload:ByteBuffer;tag:i32}
fn main()->i32{let bytes=bytes_new();if !bytes_reserve(bytes,3){return 1}let handler=(event:Event)->i32=>{return event.tag};let result=handler(Event{payload:give bytes,tag:7});return result-7}
