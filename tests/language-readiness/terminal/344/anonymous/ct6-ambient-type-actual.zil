struct Phantom<life a,T:Copy,#N:i32>{n:i32}
struct Wrap<T:Copy>{n:i32}
fn wrap<T:Copy,#N:i32,life a>(marker:Type)->Type{return Wrap::<T>}
fn result<life s>(p:&'s i32)->i32{
    let value:#(wrap::<Phantom<'s,i32,42>,42,'s>(i32))=Wrap::<Phantom<'s,i32,42>>{n:42};
    return value.n
}
fn main()->i32{let n=0;return result(&n)}
