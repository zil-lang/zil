import model
import storage
pub enum Right{Empty,Pair(manages model.Mark,manages model.Mark)}
pub enum Phase{
    Empty,
    Ready(model.Session),
    Loaded(model.Fault!model.Session),
    Nested(manages model.Mark,Right,i32)
}
pub package App{pub phase:Phase;generation:i32}
pub fn empty()->App{return App{phase:Phase.Empty,generation:7}}
pub fn ready(count:i32,fail:bool,left:i32,right:i32)->model.Fault!App{
    let session=storage.session(count,fail,left,right)?
    return App{phase:Phase.Ready(session),generation:7}
}
pub fn pending(count:i32)->App{
    return App{phase:Phase.Loaded(storage.session(count,true,97,65)),generation:7}
}
pub fn nested(count:i32)->App{
    let first=model.Mark{code:97}
    let right=if count==0{Right.Empty}else{Right.Pair(model.Mark{code:99},model.Mark{code:100})}
    return App{phase:Phase.Nested(give first,right,count),generation:7}
}
pub fn relay(app:App)->App{return app}
pub fn ready_phase(count:i32,left:i32,right:i32)->model.Fault!Phase{
    let session=storage.session(count,false,left,right)?
    return Phase.Ready(session)
}
