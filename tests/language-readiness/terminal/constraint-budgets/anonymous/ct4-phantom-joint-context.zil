struct Pair<life a,life b>{n:i32}
fn scalar(marker:Type,n:i32)->i32{return n}
fn result<life a,life b>(pa:&'a i32,pb:&'b i32)->i32{
    let value:Pair<'a,'b>=#(Pair::<'a,'b>{n:scalar(i32,42)});
    return value.n
}
fn main()->i32{let a=0;let b=1;return result(&a,&b)}
