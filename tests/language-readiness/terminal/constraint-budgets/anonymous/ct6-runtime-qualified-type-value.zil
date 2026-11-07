struct Phantom<life s,T:Copy,#N:i32>{n:i32}
fn scalar(marker:Type,n:i32)->i32{return n}
fn leaf<U:Copy>()->i32{return #(scalar(U,8))}
fn outer<T:Copy,#N:i32,life s>(marker:T,p:&'s i32)->i32{
    return leaf::<Phantom<'s,T,N>>()
}
fn main()->i32{let n=0;return outer::<bool,7>(true,&n)}
