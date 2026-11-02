import common.io as io
pub resource Tracked { pub code:i32 }
impl Drop for Tracked {
    fn drop(self:Self) { io.emit(.code) }
}
