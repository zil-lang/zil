enum Maybe<life a>{Value(&'a i32),Vacant}
fn empty<life a>(value:&'a i32)->Maybe<'a>{return Maybe::<'a>.Vacant()}
fn main()->i32{
 var number=1
 let slot=empty(&number)
 number=9
 match slot{
  Maybe.Value(pointer)=>{return *pointer},
  Maybe.Vacant()=>{return number-9}
 }
}
