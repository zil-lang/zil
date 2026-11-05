struct Pair{left:i32;right:u64}
enum Choice{Empty,Pair(i32,u64)}
struct Phantom<life a,T:Copy,#N:i32>{tuple:(i32,bool);array:[i32;2];record:Pair;variant:Choice}
fn make<T:Copy,#N:i32,life a>(marker:Type)->Phantom<'a,T,N>{
    return Phantom::<'a,T,N>{tuple:(N,true),array:[N,N+1],record:Pair{left:N+2,right:18446744073709551557u64},variant:Choice.Pair(N+3,18446744073709551557u64)}
}
fn middle<life b,#M:i32,U:Copy>(marker:Type)->Phantom<'b,U,M>{return make::<U,M,'b>(marker)}
fn forward<#N:i32,life a,T:Copy>(marker:Type)->Phantom<'a,T,N>{return middle::<'a,N,T>(marker)}
fn result<life s>(p:&'s i32)->u64{
    let value:Phantom<'s,i32,37>=#(forward::<37,'s,i32>(i32));
    if !value.tuple.1{return 0u64}
    if value.tuple.0!=37{return 0u64}
    if value.array[0]!=37{return 0u64}
    if value.array[1]!=38{return 0u64}
    if value.record.left!=39{return 0u64}
    if value.record.right!=18446744073709551557u64{return 0u64}
    match value.variant{
        Choice.Pair(x,y)=>{if x!=40{return 0u64};return y},
        Choice.Empty()=>{return 0u64}
    }
}
fn main()->u64{let n=0;return result(&n)}
