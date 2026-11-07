struct Host<T:Copy,#N:i32,life a>{
    fn identity<T:Copy,#N:i32,life a>(value:T)->T{return value}
    fn number<T:Copy,#N:i32,life a>()->i32{return N}
}
fn result<life a,life b>(pa:&'a i32,pb:&'b i32)->i32{
    let value:bool=#(Host::<i32,7,'a>.identity::<bool,42,'b>(true));
    let number:i32=#(Host::<i32,7,'a>.number::<bool,42,'b>());
    if value{return number}
    return 0
}
fn main()->i32{let a=0;let b=0;return result(&a,&b)}
