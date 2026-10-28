
package State<F>{callback:F}
fn pack<F:Fn()->i32>(callback:F)->State<F>{return State::<F>{callback:callback}}
fn read<F:Fn()->i32>(state:State<F>)->i32{let callback=&state.callback;return callback()}
fn make(n:i32)->impl Fn()->i32{return move()=>n}
fn run<F:Fn()->i32>(state:State<F>)->i32 {
    let other=make(20)
    let next=pack(other)
    return read(next)+read(state)
}
fn main()->i32{let seed=3;let first=move()=>seed;return run(pack(first))-23}
