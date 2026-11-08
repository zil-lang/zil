struct Batch<life a>{items:[&'a i32;2]}
fn make<life a>(first:&'a i32,second:&'a i32)->Batch<'a>{
    return Batch::<'a>{items:[first,second]}
}
fn relay<life a>(batch:Batch<'a>)->Batch<'a>{return batch}
fn select<life a>(batch:&Batch<'a>)->&'a i32{return batch.items[1]}
fn main()->i32{
    var first=7
    var second=9
    let original=make(&first,&second)
    let copied=original
    let batch=relay(copied)
    let pointer=select(&batch)
    first=11
    if *pointer==9{return 0}
    return 1
}
