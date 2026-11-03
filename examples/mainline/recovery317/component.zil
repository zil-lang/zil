import bridge

pub resource Mark { pub code:i32 }
impl Drop for Mark { fn drop(self:Self) { bridge.dropped(.code) } }
pub package Panel { manages mark:Mark; pub count:i32 }
pub fn open()->Panel { return Panel{mark:Mark{code:80},count:0} }
pub fn apply(panel:&mut Panel,event:i32) { panel.count+=event }
pub fn event(value:i32)->(i32,(i32,bool)) {
    bridge.emit(69)
    return (value,(value+1,true))
}
