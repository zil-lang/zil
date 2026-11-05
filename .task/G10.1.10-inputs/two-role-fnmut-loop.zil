extern "C" fn putchar(code:i32)->i32;
struct Tag<life s>{p:&'s i64}
fn dispatch<life s,F:FnMut(&Tag<'s>)->i32,G:FnMut(&Tag<'s>)->i32>(f:&mut F,g:&mut G,x:&Tag<'s>,count:i32)->i32{
    var sum=0;
    var index=0;
    while index<count{
        sum+=f(x);
        sum+=g(x);
        index+=1;
    }
    return sum;
}
fn apply<life s>(left:&Tag<'s>)->i32{
    var first=1;
    var second=10;
    var f=move(p:&Tag<'s>)->i32=>{first+=1;return first+i32(*p.p+*left.p)-42};
    var g=move(p:&Tag<'s>)->i32=>{second+=1;return second+i32(*p.p+*left.p)-42};
    return dispatch(&mut f,&mut g,left,3);
}
fn setup<life s>(x:&'s i64)->i32{let tag:Tag<'s>=Tag::<'s>{p:x};return apply(&tag)}
fn main()->i32{let x:i64=21;let n=setup(&x);unsafe{putchar(48+n/10);putchar(48+n%10);putchar(10)}return 0}
