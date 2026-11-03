fn first<T:Copy,life a>(value:&'a T)->&'a T{return value}
fn second<life a,T:Copy>(value:&'a T)->&'a T{return value}
fn third<life a,T:Copy,life b>(value:&'a T,other:&'b T)->&'a T{return value}
fn main()->i32{
    let small:i32=7;
    let wide:i64=11;
    let other:i64=13;
    if *first(&small)!=7{return 1}
    if *first::<i64>(&wide)!=11{return 2}
    if *second::<i32>(&small)!=7{return 3}
    if *second(&wide)!=11{return 4}
    if *third::<i64>(&wide,&other)!=11{return 5}
    return 0
}
