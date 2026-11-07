fn inner<T:Copy,#N:i32>(marker:T)->i32{return #(N+1)}
fn outer<T:Copy,#N:i32>(marker:T)->i32{
 let before=#(N+2)
 return before+inner::<T,N>(marker)
}
fn main()->i32{return outer::<bool,7>(true)}
