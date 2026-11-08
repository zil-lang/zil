struct Batch<life a,life b>{left:[&'a i32;2];right:[&'b i32;2]}
fn make<life a,life b>(x:&'a i32,y:&'a i32,z:&'b i32,w:&'b i32)->Batch<'b,'a>{
    return Batch::<'b,'a>{left:[z,w],right:[x,y]}
}
fn relay<life a,life b>(batch:Batch<'b,'a>)->Batch<'b,'a>{return batch}
fn left<life a,life b>(batch:&Batch<'b,'a>)->&'b i32{return batch.left[0]}
fn right<life a,life b>(batch:&Batch<'b,'a>)->&'a i32{return batch.right[1]}
fn main()->i32{
    var x=1
    var y=2
    var z=3
    var w=4
    let original=make(&x,&y,&z,&w)
    let copied=original
    let batch=relay(copied)
    let a=left(&batch)
    let b=right(&batch)
    x=8
    w=9
    if *a*10+*b==32{return 0}
    return 1
}
