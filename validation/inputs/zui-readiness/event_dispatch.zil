struct Props { width:i32=10; height:i32=20 }
enum Event { Click(i32), Reset }
fn dispatch<F:FnMut(Event)->i32>(handler:&mut F,event:Event)->i32{return handler(event)}
fn main()->i32 {
 let base=Props{};let props=Props{...base,width:30}
 var count=0
 var handler=(event:Event)=>{match event{Event.Click(delta)=>{count+=delta},Event.Reset()=>{count=0}};return count}
 let first=dispatch(&mut handler,Event.Click(props.width))
 let second=dispatch(&mut handler,Event.Click(12))
 if first!=30 || second!=42{return 1};return 0
}
