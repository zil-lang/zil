import model;
import dispatch;
fn main()->void!i32{
    let item=model.item()?;
    var count=0;
    var handler=move(item:&model.Item)->void!model.Reply=>{
        count+=1;return model.reply(item,count)
    };
    let first=dispatch.run(&mut handler,&item)?;
    let second=dispatch.run(&mut handler,&item)?;
    if first.tag!=8||second.tag!=9||item.tag!=7{return 2}
    model.emit(88);return 0
}
