struct Pair<life s,life t>{n:i32}
fn scalar(t:Type,n:i32)->i32{return n}
fn result<life a,life b>(pa:&'a i32,pb:&'b i32)->i32{
    let first:Pair<'a,'b>=#((Pair::<'a,'b>{n:scalar(i32,20)},Pair::<'b,'a>{n:scalar(i32,0)}).0);
    let second:Pair<'b,'a>=#((Pair::<'a,'b>{n:scalar(i32,0)},Pair::<'b,'a>{n:scalar(i32,22)}).1);
    return first.n+second.n
}
fn main()->i32{let left=0;let right=0;return result(&left,&right)}
