struct Phantom<T:Copy>{value:i32}
type Integer=Phantom<i32>;
type Boolean=Phantom<bool>;
type First=Integer|bool;
type Second=Boolean|bool;
type Both=Boolean|Integer;
type ReverseBoth=Integer|Boolean;
fn select(t:Type)->Type {if First==Second{return bool}if t==i32{return Both}return ReverseBoth}
fn pick(flag:bool)->Integer|Boolean {
    if flag{return Phantom::<i32>{value:20}}
    return Phantom::<bool>{value:22}
}
fn score(value:Boolean|Integer)->i32 {
    match value{Integer(x)=>{return x.value},Boolean(x)=>{return x.value}}
}
fn result()->i32 {
    let first:#(select(i32))=Phantom::<i32>{value:20};
    let second:#(select(bool))=Phantom::<bool>{value:22};
    return score(first)+score(second)
}
