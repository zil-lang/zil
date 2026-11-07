import api
fn bridge<life a>(value:TaskPollState!Poll<&'a i32>)->TaskPollState!Poll<&'a i32>{return api.relay(value)}
fn main()->TaskPollState!i32{
 let number=39
 var spare=1
 let failed=api.failed(&spare)
 spare=9
 let pending=api.pending(&spare)
 spare=11
 let local=bridge(api.ready(&number))
 return api.read(local)?+api.read(pending)?+spare-53
}
