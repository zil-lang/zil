import worker
import component
import observer
fn main()->void!i32 {
    let mode=observer.mode()
    let payload=observer.payload()
    let body=component.prepare(mode)
    let outer=move()->i32=>{
        let(created,child,parent)=thread_scope(worker.compute,payload,body)
        if created { observer.after(1,child,parent) } else { observer.after(0,child,parent) }
        if !created { if child!=0 || parent!=0 { return 9 };return 2 }
        if child!=21 { return 8 }
        if parent==-7 { return 1 }
        if parent!=17 { return 7 }
        return 0
    }
    let result=outer()
    if result==1 { return () }
    return result
}
