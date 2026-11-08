fn identity<life a,life b>(items:&'a [&'b i32;2])->&'a [&'b i32;2]{return items}
fn select<life a,life b>(items:&'a [&'b i32;2])->&'b i32{return items[1]}
fn main()->i32{
    var first=7
    var second=9
    let items=[&first,&second]
    let window=identity(&items)
    let copied=window
    let pointer=select(copied)
    first=11
    if *pointer==9{return 0}
    return 1
}
