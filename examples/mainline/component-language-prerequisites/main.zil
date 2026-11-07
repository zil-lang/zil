import config
import component
import bridge

type EventCode=#(config.value_type(1))
type WideCode=#(config.value_type(i64(1)))

fn main()->void!i32 {
    let mode=bridge.mode()
    let seed:EventCode=#splice(config.expression(config.Config::<i32,bool>{value:1}))
    let wide:WideCode=#splice(config.wide_expression(config.Config::<i64,i32>{value:i64(1)}))
    if wide!=i64(seed) { return 7 }
    let handler_mark=component.Mark{code:72}
    var handler=move(value:i32)->i32=>value+handler_mark.code-71
    let body_mark=component.Mark{code:66}
    let body=move()->i32=>{
        let owned=body_mark
        var panel=component.open()
        let (first,(second,ready)):(i32,(i32,bool))=component.event(seed+1)
        if !ready || owned.code!=66 || first!=2 || second!=3 { return -9 }
        component.apply(&mut panel,first)
        component.apply(&mut panel,second)
        if panel.count!=5 { return -8 }
        if bridge.pump(panel.count)!=6 { return -7 }
        if mode==1 { bridge.emit(70);return -1 }
        bridge.emit(83)
        return panel.count
    }
    let (registered,result)=bridge.run(&mut handler,body)
    bridge.after()
    if !registered { return 2 }
    if result<0 { return () }
    if result!=5 || handler(3)!=4 { return 8 }
    return 0
}
