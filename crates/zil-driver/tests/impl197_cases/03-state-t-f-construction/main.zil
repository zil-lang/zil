package State<T,F>{manages bytes:ByteBuffer;callback:F;value:T}
fn make<T:Copy,F:Fn(T)->T>(callback:F,value:T)->State<T,F>{return State::<T,F>{bytes:bytes_new(),callback:callback,value:value}}
fn main()->i32{let callback=move(value:i32)=>value+1;let current=make(callback,41);return current.callback(current.value)-42}
