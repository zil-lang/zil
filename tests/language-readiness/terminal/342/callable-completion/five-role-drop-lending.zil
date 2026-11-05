extern "C" fn putchar(code:i32)->i32;
struct Tag<life s>{p:&'s i64}
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){unsafe{putchar(.code)}}}
fn peek<life s,life env,F:Fn(&Tag<'s>)->&i32>(f:&'env F,x:&Tag<'s>)->&'env i32{return f(x)}
fn dispatch<life s,F:FnOnce(&Tag<'s>)->i32,G:FnMut(&Tag<'s>)->i32,H:Fn(&Tag<'s>)->i32,K:FnOnce(&Tag<'s>)->i32,L:FnOnce(&Tag<'s>)->i32>(f:F,g:&mut G,h:&H,k:K,last:L,x:&Tag<'s>,invoke:bool)->i32{
    let pending=f;
    var n=0;
    var total=0;
    while n<2{total=total+g(x)+h(x);n=n+1}
    let later=k;
    let final_owner=last;
    if invoke{return pending(x)+later(x)+final_owner(x)+total}
    return total;
}
fn apply<life s>(left:&Tag<'s>,invoke:bool)->i32{
    let first=Mark{code:65};
    let second=Mark{code:66};
    let third=Mark{code:67};
    let f=move(p:&Tag<'s>)->i32=>{let taken=give first;return i32(*p.p+*left.p)+taken.code-65};
    let k=move(p:&Tag<'s>)->i32=>{let taken=give second;return i32(*p.p+*left.p)+taken.code-66};
    let last=move(p:&Tag<'s>)->i32=>{let taken=give third;return i32(*p.p+*left.p)+taken.code-67};
    var state=0;
    var g=move(p:&Tag<'s>)->i32=>{let observed=*p.p+*left.p;observed;state=state+1;return state};
    let base=4;
    let h=move(p:&Tag<'s>)->i32=>{let observed=*p.p+*left.p;observed;return base};
    let lend_value=5;
    let lender=move(p:&Tag<'s>)=>{let observed=*p.p+*left.p;observed;return &lend_value};
    let result=peek(&lender,left);
    let observed=*result;
    return dispatch(f,&mut g,&h,k,last,left,invoke)+observed;
}
fn setup<life s>(x:&'s i64)->i32{let tag:Tag<'s>=Tag::<'s>{p:x};return apply(&tag,true)+apply(&tag,false)}
fn main()->i32{let x:i64=21;let n=setup(&x);unsafe{putchar(48+n/100);putchar(48+n/10%10);putchar(48+n%10);putchar(10)}return 0}
