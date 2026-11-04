struct Phantom<T:Copy,P:Copy>{value:i32}
type Integer=Phantom<i32,bool>;
type Boolean=Phantom<bool,bool>;
type Choice=Boolean|Integer;
fn select(t:Type,flag:bool)->Choice {
    if t==i32 {
        if flag{return Phantom::<i32,bool>{value:20}}
        return Phantom::<bool,bool>{value:22}
    }
    return Phantom::<i32,bool>{value:0}
}
fn score(value:Integer|Boolean)->i32 {
    match value{Integer(left)=>{return left.value},Boolean(right)=>{return right.value}}
}
fn result()->i32 {
    let left=#(select(i32,true));
    let right=#(select(i32,false));
    return score(left)+score(right)
}
