import worker
import component
import observer
fn main()->void!i32 {
    let mode=observer.mode()
    let(created,child,parent)=thread_scope(worker.compute,observer.payload(),component.prepare(mode))
    if created { observer.after(1,child,parent) } else { observer.after(0,child,parent) }
    if !created { if child!=0 || parent!=0 { return 9 };return 2 }
    if child!=21 { return 8 }
    if parent==-7 { return () }
    if parent!=17 { return 7 }
    return 0
}
