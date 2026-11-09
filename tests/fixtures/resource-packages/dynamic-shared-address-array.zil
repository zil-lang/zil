fn choose<life a>(items:&[&'a i32;2],index:i32)->&'a i32{return items[index]}
fn main()->i32{
    var first=7
    var second=9
    let items=[&first,&second]
    let pointer=choose(&items,1)
    return *pointer-9
}
