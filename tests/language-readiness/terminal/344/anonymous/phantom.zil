struct Phantom<T:Copy>{value:i32}
type Integer=Phantom<i32>;
type Boolean=Phantom<bool>;
type First=Integer|bool;
type Second=Boolean|bool;
fn select(t:Type)->Type {if First==Second{return bool}if t==i32{return i32}return bool}
fn pick(flag:bool)->Integer|Boolean {
    if flag{return Phantom::<i32>{value:20}}
    return Phantom::<bool>{value:22}
}
fn score(value:Boolean|Integer)->i32 {
    match value{Integer(x)=>{return x.value},Boolean(x)=>{return x.value}}
}
fn result()->i32 {
    let zero:#(select(i32))=0;
    return score(pick(true))+score(pick(false))+zero
}
