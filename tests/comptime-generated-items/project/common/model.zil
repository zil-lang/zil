extern "C" fn putchar(value:i32)->i32;
pub struct Point { pub x:i32; pub y:i32 }
pub struct OtherPoint { pub x:i32; pub y:i32 }
pub struct Hidden { x:i32 }
pub resource Ticket { pub code:i32 }
impl Drop for Ticket {
    fn drop(self:Self) { unsafe { putchar(.code) } }
}
