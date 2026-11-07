struct Phantom<life a,T:Copy,#N:i32>{n:i32}
fn make<T:Copy,#N:i32,life a>(marker:Type)->Type{return Phantom::<'a,T,N>}
fn middle<life b,#M:i32,U:Copy>(marker:Type)->Type{return make::<U,M,'b>(marker)}
fn forward<#N:i32,life a,T:Copy>(marker:Type)->Type{return middle::<'a,N,T>(marker)}
fn result<life s>(p:&'s i32)->i32{
    let value:#(forward::<42,'s,i32>(i32))=Phantom::<'s,i32,42>{n:42};
    return value.n
}
fn main()->i32{let n=0;return result(&n)}
