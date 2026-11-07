extern "C" fn putchar(value: i32) -> i32;

pub resource Ticket { pub code: i32 }
impl Drop for Ticket { fn drop(self: Self) { unsafe { putchar(.code) } } }
pub package Item { manages ticket: Ticket; pub code: i32 }

pub fn item(code: i32) -> Item => Item { ticket: Ticket { code: code }, code: code }
pub fn empty() -> Store<Item> contract { result(create) } { return store_new() }
