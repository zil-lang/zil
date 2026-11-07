fn result<T:Copy,#N:i32>(marker:T)->i32{return #(N+1)}
fn main()->i32{return result::<bool,7>(true)}
