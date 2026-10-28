package State<T:Copy,F>{callback:F;value:T}
fn run<T:Copy,F:Fn(T)->T>(state:State<T,F>)->T{return state.callback(state.value)}
fn main(){}
