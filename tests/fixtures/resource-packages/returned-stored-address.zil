pub struct Holder<life a> { pub pointer:&'a mut i64 }
fn make<life a>(pointer:&'a mut i64)->Holder<'a>{
    return Holder::<'a>{pointer:pointer}
}
fn identity<life a,life b>(holder:&'b mut Holder<'a>)->&'b mut Holder<'a>{
    return holder
}
fn step<life a,life z>(left:&mut Holder<'a>,right:&mut Holder<'z>){
    let first=identity(left).pointer
    *first=*first+1
    let second=identity(right).pointer
    *second=*second+2
}
fn main()->i32{
    var left:i64=40
    var right:i64=12
    if true {
        var a=make(&mut left)
        var b=make(&mut right)
        step(&mut a,&mut b)
    }
    if left==41 {if right==14{return 0}}
    return 1
}
