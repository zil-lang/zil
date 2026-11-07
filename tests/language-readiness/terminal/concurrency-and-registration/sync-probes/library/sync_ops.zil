pub fn advance_atomic(cell:&AtomicI32,amount:i32)->i32 { return atomic_fetch_add(cell,amount) }
pub fn advance_mutex(cell:&MutexI32,amount:i32)->i32 { return mutex_fetch_add(cell,amount) }
