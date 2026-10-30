resource Token{n:i32}
package State<F>{manages token:Token;callback:F}
fn make(n:i32)->impl Fn()->i32{return move()=>n}
fn pack<F:Fn()->i32>(f:F)->State<F>{return State::<F>{token:Token{n:3},callback:f}}
fn main()->i32{let state=pack(make(9));return state.callback()-9}
