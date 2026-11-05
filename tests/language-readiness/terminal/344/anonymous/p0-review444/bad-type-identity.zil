struct Phantom<life a,T:Copy,#N:i32>{n:i32}
fn make<T:Copy,#N:i32,life a>(marker:Type)->Phantom<'a,T,N>{return Phantom::<'a,T,N>{n:N}}
fn forward<T:Copy,U:Copy,#N:i32,life a>(marker:Type)->Phantom<'a,T,N>{return make::<U,N,'a>(marker)}
fn result<life s>(p:&'s i32)->i32{let value:Phantom<'s,i32,42>=#(forward::<i32,i32,42,'s>(i32));return value.n}
fn main()->i32{let n=0;return result(&n)}
