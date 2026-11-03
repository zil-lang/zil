fn select<T:Copy,life a,life b>(later:&'b T,earlier:&'a T)->&'a T{return earlier}
fn forward<life b,U:Copy,life a>(first:&'b U,second:&'a U)->&'b U{return select(second,first)}
fn main()->i32{
    var first:i32=40;
    var second:i32=2;
    let selected=forward::<i32>(&first,&second);
    second=9;
    return *selected+second-49
}
