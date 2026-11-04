package P { manages data:ByteBuffer; n:i32 }
fn build()->P? { return Option.Some(P{data:bytes_new(),n:42}) }
fn result()->i32 {
  let first:i32?=Option.Some(20);
  let second:Option<i32>=Option.Some(22);
  let sum=match_value(first)+match_value(second);
  let owner=build();
  match owner {
    Option.Some(payload)=>{ return payload.n+sum-42 },
    Option.None()=>{ return 0 }
  }
}
fn match_value(value:i32?)->i32 {
  match value { Option.Some(n)=>{return n}, Option.None()=>{return 0} }
}
