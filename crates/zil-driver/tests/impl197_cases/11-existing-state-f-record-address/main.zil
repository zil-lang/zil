struct Data{wide:i64;n:i32}
package State<F>{manages bytes:ByteBuffer;callback:F;value:Data}
fn make<F:FnMut(&mut Data)->i32>(callback:F)->State<F>{return State::<F>{bytes:bytes_new(),callback:callback,value:Data{wide:7,n:41}}}
fn run<F:FnMut(&mut Data)->i32>(state:State<F>)->i32{var current=state;let callback=&mut current.callback;let value=&mut current.value;return callback(value)}
fn factory()->impl FnMut(&mut Data)->i32{var count=0;return move(value:&mut Data)=>{count+=1;value.n+=1;return value.n}}
fn main()->i32{return run(make(factory()))-42}
