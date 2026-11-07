package State<F,T>{manages bytes:ByteBuffer;callback:F;count:i32}
fn make<F:Fn()->i32,T:Copy>(callback:F,value:T)->State<F,T>{return State::<F,T>{bytes:bytes_new(),callback:callback,count:1}}
fn factory()->impl Fn()->i32{return move()=>7}
fn main()->i32{let a=make(factory(),i32(1));let b=make(factory(),i64(2));return a.callback()+b.callback()+a.count+b.count-16}
