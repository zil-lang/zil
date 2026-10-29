extern "C" fn putchar(value: i32) -> i32;
pub resource Ticket { pub value: i32 }
impl Drop for Ticket { fn drop(self: Self) { unsafe { putchar(46) } } }
impl Ticket {
    pub fn read(self: Self) -> i32 => .value
    pub fn consume(give self: Self, delta: i32) -> i32 => .value + delta
}
pub fn make(value: i32) -> Ticket contract { result(create) } => Ticket { value: value }
