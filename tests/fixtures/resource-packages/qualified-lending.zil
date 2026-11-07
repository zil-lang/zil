struct Tag<life s>{p:&'s i64}
fn peek<life s,life env,F:Fn(&Tag<'s>)->&i32>(f:&'env F,x:&Tag<'s>)->&'env i32{return f(x)}
fn apply<life s>(left:&Tag<'s>)->i32{
    let first_value=17;
    let second_value=25;
    let first=move(p:&Tag<'s>)=>{let observed=*p.p+*left.p;observed;return &first_value};
    let second=move(p:&Tag<'s>)=>{let observed=*p.p+*left.p;observed;return &second_value};
    let a=peek(&first,left);
    let b=peek(&second,left);
    return *a+*b;
}
fn setup<life s>(x:&'s i64)->i32{let tag:Tag<'s>=Tag::<'s>{p:x};return apply(&tag)}
fn main()->i32{let x:i64=21;return setup(&x)-42}
