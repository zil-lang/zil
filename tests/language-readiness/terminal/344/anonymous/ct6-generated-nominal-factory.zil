import std.syntax as syntax;
#emit quote<ItemList>{
    struct $(syntax.export("Phantom"))<life a,T:Copy,#N:i32>{n:i32}
    struct $(syntax.export("Maker")) {
        fn make<T:Copy,#N:i32,life a>(marker:Type)->Phantom<'a,T,N>{return Phantom::<'a,T,N>{n:N}}
        fn ty<T:Copy,#N:i32,life a>(marker:Type)->Type{return Phantom::<'a,T,N>}
    }
};
fn result<life s>(p:&'s i32)->i32{
    let first:Phantom<'s,i32,42>=#(Maker.make::<i32,42,'s>(i32));
    let second:#(Maker.ty::<i32,42,'s>(i32))=Phantom::<'s,i32,42>{n:42};
    return first.n+second.n-42
}
fn main()->i32{let n=0;return result(&n)}
