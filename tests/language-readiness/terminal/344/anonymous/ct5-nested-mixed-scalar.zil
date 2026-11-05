struct Phantom<life a,T:Copy,#N:i32>{n:i32}
fn make<T:Copy,#N:i32,life a>(marker:Type)->i32{return N}
fn forward<T:Copy,#N:i32,life a>(marker:Type)->i32{return make::<T,N,'a>(marker)}
fn outer<A:Copy,#M:i32,life b>(marker:Type)->i32{return forward::<A,M,'b>(marker)}
fn result<life s>(p:&'s i32)->i32{let value=#(outer::<i32,42,'s>(i32));return value}
fn main()->i32{let n=0;return result(&n)}
