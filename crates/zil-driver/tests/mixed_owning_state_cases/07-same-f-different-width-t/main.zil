package State<T,F>{manages bytes:ByteBuffer;callback:F;value:T}
fn make<T:Copy,F:Fn()->i32>(callback:F,value:T)->State<T,F>{return State::<T,F>{bytes:bytes_new(),callback:callback,value:value}}
fn factory()->impl Fn()->i32{return move()=>1}
fn main()->i32{let a=make(factory(),i32(3));let b=make(factory(),i64(4));return a.value+i32(b.value)+a.callback()+b.callback()-9}
