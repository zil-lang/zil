struct Tag<life s>{p:&'s i64}
fn all<life s,F:Fn(&Tag<'s>)->i32>(a:&F,b:&F,c:&F,x:&Tag<'s>)->i32{return a(x)+b(x)+c(x)}
fn all_mut<life s,F:FnMut(&Tag<'s>)->i32>(a:&mut F,b:&mut F,c:&mut F,x:&Tag<'s>)->i32{return a(x)+b(x)+c(x)}
fn apply<life s>(left:&Tag<'s>)->i32{
    let a=move(p:&Tag<'s>)->i32=>i32(*p.p+*left.p);
    let b=a;
    let c=a;
    var count=0;
    var d=move(p:&Tag<'s>)->i32=>{let seen=*p.p+*left.p;seen;count=count+1;return count};
    var e=d;
    var f=d;
    return all(&a,&b,&c,left)+all_mut(&mut d,&mut e,&mut f,left)+all_mut(&mut d,&mut e,&mut f,left);
}
fn setup<life s>(x:&'s i64)->i32{let tag:Tag<'s>=Tag::<'s>{p:x};return apply(&tag)}
fn main()->i32{let x:i64=21;return setup(&x)-135}
