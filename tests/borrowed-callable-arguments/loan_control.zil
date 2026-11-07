package Event{manages payload:ByteBuffer;tag:i32}
fn main()->i32{let b=bytes_new();let e=Event{payload:give b,tag:1};let view=bytes_view(e.payload);let h=(e:Event)->Event=>{return e};let other=Event{payload:bytes_new(),tag:2};let a=h(other);let n=i32(view[0]);return n+a.tag}
