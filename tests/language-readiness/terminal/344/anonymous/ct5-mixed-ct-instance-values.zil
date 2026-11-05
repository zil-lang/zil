struct Phantom<life a,T:Copy,#N:i32>{n:i32}
fn make<T:Copy,#N:i32,life a>(marker:Type)->Phantom<'a,T,N>{
    return Phantom::<'a,T,N>{n:N}
}
fn selected<T:Copy,#N:i32,life a>(marker:Type)->Type{return T}
fn result<life s,life t>(p:&'s i32,q:&'t i32)->i32{
    let left:Phantom<'s,i32,7>=
        #((make::<i32,7,'s>(i32),make::<bool,35,'t>(bool)).0);
    let right:Phantom<'t,bool,35>=
        #((make::<i32,7,'s>(i32),make::<bool,35,'t>(bool)).1);
    let scalar:#(selected::<i32,7,'s>(i32))=42;
    let flag:#(selected::<bool,35,'t>(bool))=true;
    if left.n!=7{return 1}
    if right.n!=35{return 2}
    if !flag{return 3}
    return scalar
}
fn main()->i32{let n=0;let m=0;return result(&n,&m)}
