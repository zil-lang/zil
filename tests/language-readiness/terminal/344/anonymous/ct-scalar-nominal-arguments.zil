struct Stamp<T:Copy,#N:i32,#B:bool>{value:i32}
type Left=Stamp<fn(i32)->i32,7,true>;
type Right=Stamp<fn(i32)->i32,8,true>;
fn choose(t:Type,first:bool)->Left|Right{
    if first{return Stamp::<fn(i32)->i32,7,true>{value:20}}
    return Stamp::<fn(i32)->i32,8,true>{value:22}
}
fn score(value:Right|Left)->i32{match value{Left(x)=>{return x.value},Right(x)=>{return x.value}}}
struct ArrayBox<T:Copy,#N:i32>{values:[T;N]}
fn array(t:Type)->ArrayBox<i32,2>{return ArrayBox::<i32,2>{values:[0,0]}}
fn result()->i32{
    let left=#(choose(i32,true));
    let right=#(choose(i32,false));
    let empty=#(array(i32));
    return score(left)+score(right)+empty.values[0]
}
