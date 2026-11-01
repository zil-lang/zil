import portable
import model
import state
fn switch(count:i32)->model.Fault!i32{
    var phase=state.ready_phase(count,97,65)?
    defer{portable.emit(81)}
    let late=model.Mark{code:76}
    phase=state.ready_phase(count,109,77)?
    portable.emit(78)
    return 0
}
fn main()->model.Fault!i32{
    let count=portable.read()-48
    let mode=portable.read()-48
    if count<0||count>3||mode<0||mode>7{return 9}
    portable.emit(83)
    if mode==7{return switch(count)}
    var app=if mode==0{state.empty()}else{
        if mode==2{state.pending(count)}else{
            if mode==5{state.nested(count)}else{state.ready(count,false,97,65)?}
        }
    }
    defer{portable.emit(81)}
    let late=model.Mark{code:76}
    if mode==3{app=state.ready(count,true,109,77)?}
    if mode==4{app=state.relay(app)}
    if mode==6{portable.emit(78);return 0}
    let phase=app.phase
    match phase{
        state.Phase.Empty=>{portable.emit(69)}
        state.Phase.Ready(session)=>{portable.emit(82)}
        state.Phase.Loaded(result)=>{let session=result?;portable.emit(82)}
        state.Phase.Nested(a,state.Right.Pair(c,d),1)=>{portable.emit(66)}
        state.Phase.Nested(a,state.Right.Pair(c,d),_)=>{portable.emit(67)}
        state.Phase.Nested(a,state.Right.Empty,_)=>{portable.emit(69)}
    }
    portable.emit(78)
    return 0
}
