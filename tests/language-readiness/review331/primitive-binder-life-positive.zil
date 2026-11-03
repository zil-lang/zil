fn concrete()->i64{return 7}
fn identity<i64:Copy,life i64>(value:&'i64 i64)->&'i64 i64{
    let number=concrete();
    if number!=7{return value}
    return value
}
fn main()->i32{
    let value=true;
    if !*identity::<bool>(&value){return 1}
    return 0
}
