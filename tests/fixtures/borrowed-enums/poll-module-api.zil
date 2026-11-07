pub fn ready<life a>(pointer:&'a i32)->TaskPollState!Poll<&'a i32>{return Poll::<&'a i32>.Ready(pointer)}
pub fn pending<life a>(pointer:&'a i32)->TaskPollState!Poll<&'a i32>{return Poll::<&'a i32>.Pending()}
pub fn failed<life a>(pointer:&'a i32)->TaskPollState!Poll<&'a i32>{return TaskPollState.Invalid()}
pub fn relay<life a>(value:TaskPollState!Poll<&'a i32>)->TaskPollState!Poll<&'a i32>{return value}
pub fn read<life a>(value:TaskPollState!Poll<&'a i32>)->TaskPollState!i32{
 let local=relay(value)?
 match local{
  Poll.Pending()=>{return 3},
  Poll.Ready(pointer)=>{return *pointer}
 }
}
