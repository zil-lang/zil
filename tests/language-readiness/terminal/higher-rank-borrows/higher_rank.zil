fn identity<life a>(p:&'a mut i64)->&'a mut i64{return p}
fn invoke(f:for<life a> fn(&'a mut i64)->&'a mut i64,p:&mut i64)->&mut i64{
    return f(p)
}
fn result()->i64{
    let f:for<life a> fn(&'a mut i64)->&'a mut i64=identity;
    let copy=f;
    var first:i64=40;
    let p=invoke(copy,&mut first);
    *p=*p+2;
    return first
}
