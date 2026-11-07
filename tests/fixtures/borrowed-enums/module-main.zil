import api
fn bridge<life a,life b>(value:api.Choice<'a,'b>)->api.Choice<'a,'b>{return api.relay(value)}
fn main()->i32{
 let left=17
 let right=25
 return api.read(bridge(api.select(&left,&right)))-17
}
