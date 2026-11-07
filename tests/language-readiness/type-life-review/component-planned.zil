struct View<T:Copy>{value:T}
fn borrow<T:Copy,life a>(view:&'a View<T>)->&'a T{return &view.value}
trait Read{
    fn read(self:&Self)->i32;
    fn adjusted(self:&Self)->i32{return self.read()+1}
}
struct Meter{value:i32}
impl Read for Meter{fn read()->i32{return .value}}
enum Message{Value{n:i32},Empty}
fn worker(value:i32)->i32{return value+1}
fn main()->i32{
    let view=View::<i32>{value:40};
    let projected=borrow(&view);
    let meter=Meter{value:*projected};
    let message=Message.Value{n:meter.adjusted()};
    let outer=()->i32=>{
        var payload:i32=0;
        match message{
            Message.Value{n:value}=>{payload=value},
            Message.Empty()=>{return 3}
        }
        let body=()->i32=>*projected;
        let(created,child,parent)=thread_scope(worker,payload,body);
        if !created{return 4}
        return child+parent-82
    };
    return outer()
}
