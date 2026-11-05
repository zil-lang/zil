struct Tag<life s>{p:&'s i64}
fn peek<life s,F:FnMut(&Tag<'s>)->&i32>(f:&mut F,x:&Tag<'s>)->&i32{return f(x)}
fn counter<life s>(left:&Tag<'s>)->i32{
    var value=3;
    var f=move(p:&Tag<'s>)=>{let observed=*p.p+*left.p;observed;value+=1;return &value};
    let a=peek(&mut f,left);
    let old=*a;
    let b=peek(&mut f,left);
    return old+*b;
}
fn setup<life s>(x:&'s i64)->i32{let tag:Tag<'s>=Tag::<'s>{p:x};return counter(&tag)}
fn main()->i32{let x:i64=21;return setup(&x)-9}
