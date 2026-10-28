package State<T:Copy,F>{manages bytes:ByteBuffer;callback:F;value:T}
fn make<T:Copy,F:FnMut(&mut T)->T>(callback:F,value:T)->State<T,F>{return State::<T,F>{bytes:bytes_new(),callback:callback,value:value}}
fn run<T:Copy,F:FnMut(&mut T)->T>(state:State<T,F>)->T{var current=state;let callback=&mut current.callback;let value=&mut current.value;return callback(value)}
fn factory()->impl FnMut(&mut i32)->i32{var count=0;return move(value:&mut i32)=>{count+=1;*value+=1;return *value}}
fn main()->i32{return run(make(factory(),41))-42}
