struct Phantom<life a,T:Copy,#N:i32>{n:i32}
struct Maker{fn make<T:Copy,#N:i32,life a>(marker:Type)->Phantom<'a,T,N>{return Phantom::<'a,T,N>{n:N}}}
fn result<life s>(p:&'s i32)->i32{let value:Phantom<'s,i32,42>=#(Maker.make::<i32,42,'s>(i32));return value.n}
fn main()->i32{let n=0;return result(&n)}