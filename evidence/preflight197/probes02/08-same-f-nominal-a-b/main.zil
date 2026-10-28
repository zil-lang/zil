struct A{n:i32}struct B{n:i32}
package State<T,F>{manages bytes:ByteBuffer;callback:F;value:T}
fn make<T:Copy,F:Fn()->i32>(callback:F,value:T)->State<T,F>{return State::<T,F>{bytes:bytes_new(),callback:callback,value:value}}
fn factory()->impl Fn()->i32{return move()=>1}
fn main()->i32{let a=make(factory(),A{n:3});let b=make(factory(),B{n:4});return a.value.n+b.value.n+a.callback()+b.callback()-9}
