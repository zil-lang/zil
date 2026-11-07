fn forward<life a>(p:&'a mut i64)->&'a mut i64{return p}
fn result()->i64{
    var x:i64=40;
    let p=forward(&mut x);
    *p=*p+2;
    return x
}
