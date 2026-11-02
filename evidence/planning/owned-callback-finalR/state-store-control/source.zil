package State<F>{manages guard:ByteBuffer;callback:F;tag:i32}
fn pack<F:Fn(i32)->i32>(callback:F)->State<F>{return State::<F>{guard:bytes_new(),callback:callback,tag:7}}
fn keep_state<F:Fn(i32)->i32>(state:State<F>)->State<F>{return state}
fn enqueue<F:Fn(i32)->i32>(state:State<F>)->i32{let items:Store<State<F>>=store_new();if !store_reserve(items,1){return 1}store_push(items,state);return 0}
fn main()->i32{let state=keep_state(pack(move(value:i32)->i32=>{return value+3}));let value=state.callback(4);return enqueue(state)+value-7}
