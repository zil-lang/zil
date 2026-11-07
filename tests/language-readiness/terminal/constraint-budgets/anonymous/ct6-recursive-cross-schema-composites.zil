struct Leaf<life a>{n:i32;wide:u64}
enum Choice<life a>{Empty,Value(Leaf<'a>)}
struct Nested<life a,T:Copy,#N:i32>{tuple:(Leaf<'a>,bool);array:[Leaf<'a>;2];record:Leaf<'a>;variant:Choice<'a>}
fn make<T:Copy,#N:i32,life a>(marker:Type)->Nested<'a,T,N>{
    return Nested::<'a,T,N>{
        tuple:(Leaf::<'a>{n:N,wide:18446744073709551557u64},true),
        array:[Leaf::<'a>{n:N+1,wide:18446744073709551557u64},Leaf::<'a>{n:N+2,wide:18446744073709551557u64}],
        record:Leaf::<'a>{n:N+3,wide:18446744073709551557u64},
        variant:Choice.Value(Leaf::<'a>{n:N+4,wide:18446744073709551557u64})
    }
}
fn middle<life b,#M:i32,U:Copy>(marker:Type)->Nested<'b,U,M>{return make::<U,M,'b>(marker)}
fn forward<#N:i32,life a,T:Copy>(marker:Type)->Nested<'a,T,N>{return middle::<'a,N,T>(marker)}
fn result<life s>(p:&'s i32)->u64{
    let value:Nested<'s,i32,37>=#(forward::<37,'s,i32>(i32));
    if !value.tuple.1{return 0u64}
    if value.tuple.0.n!=37{return 0u64}
    if value.array[0].n!=38{return 0u64}
    if value.array[1].n!=39{return 0u64}
    if value.record.n!=40{return 0u64}
    if value.record.wide!=18446744073709551557u64{return 0u64}
    let choice:Choice<'s>=value.variant;
    match choice{
        Choice.Value(leaf)=>{if leaf.n!=41{return 0u64};return leaf.wide},
        Choice.Empty()=>{return 0u64}
    }
}
fn main()->u64{let n=0;return result(&n)}
