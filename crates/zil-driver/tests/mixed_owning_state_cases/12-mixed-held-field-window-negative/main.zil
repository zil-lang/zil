package State<T,F>{manages bytes:ByteBuffer;callback:F;value:T}
fn run<T:Copy,F:FnMut(&mut T)->T>(state:State<T,F>)->T{var current=state;let held=&current.value;let callback=&mut current.callback;let value=&mut current.value;callback(value);return *held}
fn main(){}
