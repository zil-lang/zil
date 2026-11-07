import holder
fn main()->i32{
 var old=1
 var next=41
 if true {
  var session=holder.make(&mut old)
  holder.replace(&mut session,&mut next)
  old=7
  if holder.read(&mut session)!=41{return 1}
 }
 if old==7 && next==41{return 0}
 return 2
}
