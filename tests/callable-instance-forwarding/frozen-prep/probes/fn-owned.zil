resource R{n:i32}package Event{manages bytes:ByteBuffer;n:i32}package Reply{manages bytes:ByteBuffer;n:i32}
fn keep<F:Fn(Event)->Reply>(f:F)->F{return f}
fn main()->i32{let handler=move(event:Event)->Reply=>{return Reply{bytes:give event.bytes,n:event.n+1}};let current=keep(handler);let reply=current(Event{bytes:bytes_new(),n:1});return reply.n-2}
