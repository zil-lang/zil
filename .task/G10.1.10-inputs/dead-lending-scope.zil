struct Tag<life s>{p:&'s i64}
fn peek<life s,life env,F:Fn(&Tag<'s>)->&i32>(f:&'env F,x:&Tag<'s>)->&'env i32{return f(x)}
fn apply<life s>(left:&Tag<'s>)->i32{
    let zero=0;
    var saved=&zero;
    if true {
        let value=42;
        let first=move(p:&Tag<'s>)=>{let observed=*p.p+*left.p;observed;return &value};
        saved=peek(&first,left);
    }
    return *saved;
}
fn setup<life s>(x:&'s i64)->i32{let tag:Tag<'s>=Tag::<'s>{p:x};return apply(&tag)}
fn main()->i32{let x:i64=21;return setup(&x)-42}
