struct Key{n:i32}
struct Value{n:i32}
fn inspect<K:Copy+StoreElement,V:Copy+StoreElement,E:Fn(K,K)->bool,H:Fn(K)->i64>(left:K,right:K,value:V,eq:&E,hash:&H)->bool{
    value;
    let code=hash(left)
    return code>0 && eq(left,right)
}
fn main()->i32{
    let eq=(a:Key,b:Key)=>a.n==b.n
    let hash=(a:Key)=>i64(a.n)
    if inspect(Key{n:7},Key{n:7},Value{n:0},&eq,&hash){return 0}
    return 1
}
