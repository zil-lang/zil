fn make<life a>(first:&'a i32,second:&'a i32)->[&'a i32;2]{return [first,second]}
fn relay<life a>(items:[&'a i32;2])->[&'a i32;2]{return items}
fn select<life a>(items:[&'a i32;2])->&'a i32{return items[1]}
fn main()->i32{
    var first=7
    var second=9
    let original=make(&first,&second)
    let copied=original
    let items=relay(copied)
    let pointer=select(items)
    first=11
    if *pointer==9{return 0}
    return 1
}
