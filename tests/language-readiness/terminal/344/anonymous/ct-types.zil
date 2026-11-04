type Forward=i32|bool;
type Reverse=bool|i32;
type Nested=(i32|bool)|f64;
type Flat=f64|bool|i32;
fn select(t:Type)->Type {
    if Forward!=Reverse {return bool}
    if Nested!=Flat {return bool}
    if t==i32 {return Reverse}
    return Forward
}
fn choose(flag:bool)->bool|i32 {if flag{return 40}return false}
fn score(value:i32|bool)->i32 {
    match value {i32(n)=>{return n},bool(b)=>{if b{return 1}return 2}}
}
fn result()->i32 {
    let value:#(select(i32))=40;
    let other=#(choose(false));
    return score(value)+score(other)
}
