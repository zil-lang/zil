struct Phantom<T:Copy>{n:i32}
type Borrowed=for<life a> for<life b> fn(&'a i32,&'b i32)->&'a i32;
fn make<T:Copy>(marker:Type,n:i32)->Phantom<T>{return Phantom::<T>{n:n}}
fn copy<T:Copy>(marker:Type,value:Phantom<T>)->Phantom<T>{return value}
fn selected<T:Copy>(marker:Type,value:Phantom<T>)->Type{return T}
fn identity(n:i32)->i32{return n}
fn first(p:&i32,q:&i32)->&i32{return p}
fn second(p:&i32,q:&i32)->&i32{return q}
fn result()->i32{
    let left=#(copy(i32,make::<fn(i32)->i32>(i32,20)));
    let right=#(copy(i32,make::<Borrowed>(i32,22)));
    let invoke:#(selected(i32,make::<fn(i32)->i32>(i32,20)))=identity;
    let borrowed:#(selected(i32,make::<Borrowed>(i32,22)))=first;
    return invoke(left.n)+*borrowed(&right.n,&left.n)
}
