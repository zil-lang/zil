struct Left {value:i32}
struct Right {value:i32}
type L=Left;
type R=Right;
type Pair=(i32,bool);
type Choice=Right|Left;
fn same<T:Copy>(value:T)->T{return value}
fn pick(flag:bool)->Choice {
    if flag {return Left{value:20}}
    return Right{value:22}
}
fn unpack(value:Choice)->i32 {
    match value {L(left)=>{return left.value},R(right)=>{return right.value}}
}
fn tuple(value:Pair|bool)->i32 {
    match value { Pair(pair)=>{return pair.0}, bool(flag)=>{if flag{return 1}return 2} }
}
fn result()->i32 {
    let pair:Pair|bool=(0,false);
    return unpack(same(pick(true)))+unpack(pick(false))+tuple(pair)
}
