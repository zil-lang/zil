struct Pair{left:i32,right:i32}
fn ready<life a>(pointer:&'a Pair)->Poll<&'a Pair>{return Poll::<&'a Pair>.Ready(pointer)}
fn main()->i32{
 let pair=Pair{left:17,right:25}
 match ready(&pair){
  Poll.Pending()=>{return 1},
  Poll.Ready(pointer)=>{return pointer.left+pointer.right-42}
 }
}
