pub package State<T,F>{manages bytes:ByteBuffer;callback:F;value:T}
pub fn make<T:Copy,F:Fn(T)->T>(callback:F,value:T)->State<T,F>{return State::<T,F>{bytes:bytes_new(),callback:callback,value:value}}
pub fn run<T:Copy,F:Fn(T)->T>(state:State<T,F>)->T{return state.callback(state.value)}
