struct Phantom<life a,T:Copy,#N:i32>{n:i32}
struct Left{fn make<T:Copy,#N:i32,life a>(marker:Type)->Phantom<'a,T,N>{return Phantom::<'a,T,N>{n:N}}}
struct Right{fn make<T:Copy,#N:i32,life a>(marker:Type)->Phantom<'a,T,N>{return Phantom::<'a,T,N>{n:N*5}}}
fn result<life s>(p:&'s i32)->i32{
    let left:Phantom<'s,i32,7>=#(Left.make::<i32,7,'s>(i32));
    let right:Phantom<'s,i32,7>=#(Right.make::<i32,7,'s>(i32));
    if left.n!=7{return 1}
    if right.n!=35{return 2}
    return left.n+right.n
}
fn main()->i32{let n=0;return result(&n)}
