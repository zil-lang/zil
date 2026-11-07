package State<F>{manages bytes:ByteBuffer;callback:F}
fn make<F:Fn()->i32>(callback:F)->State<F>{return State::<F>{bytes:bytes_new(),callback:callback}}
fn run<T:Copy,F:Fn()->i32>(state:State<F>,value:T)->T{state.callback();return value}
fn main()->i32{let callback=move()=>1;return run(make(callback),41)-41}
