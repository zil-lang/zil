struct Phantom<life a,T:Copy,#N:i32>{n:i32}
struct Wrap<T:Copy>{n:i32}
struct Factory{
    fn lifetime<life a>(marker:Type)->Type{return Phantom::<'a,i32,42>}
    fn typed<T:Copy>(marker:Type)->Type{return Wrap::<T>}
    fn type_life<T:Copy,life a>(marker:Type)->Type{return Phantom::<'a,T,42>}
    fn const_life<#N:i32,life a>(marker:Type)->Type{return Phantom::<'a,i32,N>}
}
fn result<life s>(p:&'s i32)->i32{
    let a:#(Factory.lifetime::<'s>(i32))=Phantom::<'s,i32,42>{n:42};
    let b:#(Factory.type_life::<i32,'s>(i32))=Phantom::<'s,i32,42>{n:42};
    let c:#(Factory.const_life::<42,'s>(i32))=Phantom::<'s,i32,42>{n:42};
    let d:#(Factory.typed::<Phantom<'s,i32,42>>(i32))=Wrap::<Phantom<'s,i32,42>>{n:42};
    if a.n!=42{return 0}
    if b.n!=42{return 0}
    if c.n!=42{return 0}
    return d.n
}
fn main()->i32{let n=0;return result(&n)}
