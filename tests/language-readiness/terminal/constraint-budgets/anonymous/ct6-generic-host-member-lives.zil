struct Pair<life a,life b>{n:i32}
struct Host<life h>{
    fn make<life m>()->Pair<'h,'m>{return Pair::<'h,'m>{n:42}}
}
fn result<life a,life b>(pa:&'a i32,pb:&'b i32)->i32{
    let value:Pair<'a,'b>=#(Host::<'a>.make::<'b>());
    return value.n
}
fn main()->i32{let a=0;let b=0;return result(&a,&b)}
