package P { manages data:ByteBuffer; n:i32 }
enum E { Pair(i32,i32), None }
enum Single { Data(i32) }
fn inspect(value:&Option<P>)->i32 {
  match *value {
    Option.Some(ref payload)=>{return payload.n},
    Option.None()=>{return 0}
  }
}
fn mutate(value:&mut E)->i32 {
  match *value {
    E.Pair(ref mut left,ref mut right)=>{*left+=*right;return *left},
    E.None()=>{return 0}
  }
}
fn selected(value:&Single)->&i32 {
  match *value { Single.Data(ref payload)=>{return payload} }
}
fn result()->i32 {
  let (ref mut first,second)=(20,22);
  *first+=second;
  if *first!=42 {return 1}
  var pair=E.Pair(20,22);
  if mutate(&mut pair)!=42 {return 2}
  let owner:P?=Option.Some(P{data:bytes_new(),n:42});
  if inspect(&owner)!=42 {return 3}
  var single=Single.Data(42);
  let payload=selected(&single);
  let observed=*payload;
  single=Single.Data(7);
  return observed
}
