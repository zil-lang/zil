package State<F>{manages bytes:ByteBuffer;callback:F;value:i32}
fn make<F:FnMut(&mut i32)->i32>(callback:F)->State<F>{return State::<F>{bytes:bytes_new(),callback:callback,value:41}}
fn run<F:FnMut(&mut i32)->i32>(state:State<F>)->i32{var current=state;let callback=&mut current.callback;let value=&mut current.value;return callback(value)}
fn factory()->impl FnMut(&mut i32)->i32{var count=0;return move(value:&mut i32)=>{count+=1;*value+=1;return *value}}
fn main()->i32{return run(make(factory()))-42}
