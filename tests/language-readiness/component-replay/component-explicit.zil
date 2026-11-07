// Integration source: default fnptr -> large enum ABI -> mutable life -> shared thread.
type Big=[[[[[[[[[[[[[u8;1];2];2];2];2];2];2];2];2];2];2];2];2];
enum Message{Data{bytes:Big},Empty}
fn keep(value:Message)->Message{return value}
fn increment(value:i32)->i32{return value+1}
trait Factory{fn handler(self:&Self)->fn(i32)->i32{return increment}}
struct Component{}
impl Factory for Component{}
fn mutable_slot<life a>(value:&'a mut i32)->&'a mut i32{return value}
fn child(value:&i32)->i32{return *value+1}
fn main()->i32{
    let a0=[41u8];
    let a1=[a0,a0];
    let a2=[a1,a1];
    let a3=[a2,a2];
    let a4=[a3,a3];
    let a5=[a4,a4];
    let a6=[a5,a5];
    let a7=[a6,a6];
    let a8=[a7,a7];
    let a9=[a8,a8];
    let a10=[a9,a9];
    let a11=[a10,a10];
    let a12=[a11,a11];
    let pass:fn(Message)->Message=keep;
    let message:Message=pass(Message.Data{bytes:a12});
    var value:i32=0;
    match message{
        Message.Data{bytes:data}=>{value=i32(data[0][0][0][0][0][0][0][0][0][0][0][0][0])},
        Message.Empty()=>{return 1}
    }
    let component=Component{};
    let handler=component.handler();
    let mutable=mutable_slot(&mut value);
    *mutable=handler(*mutable);
    let shared=&value;
    let body=()->i32=>*shared;
    let(created,worker,parent)=thread_scope(child,shared,body);
    if !created{return 2}
    value=value+2;
    if worker!=43||parent!=42||value!=44{return 3}
    return 0
}
