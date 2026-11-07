struct Phantom<life a,T:Copy,#N:i32>{n:i32}
struct Maker<life h,X:Copy,#M:i32>{
    fn make()->Type{return Phantom::<'h,X,M>}
}
fn choose<life s>()->Type{return Maker::<'s,bool,42>.make()}
fn result<life s>(p:&'s i32)->i32{
    let value:#(choose::<'s>())=Phantom::<'s,bool,42>{n:42};
    return value.n
}
fn main()->i32{let n=0;return result(&n)}
