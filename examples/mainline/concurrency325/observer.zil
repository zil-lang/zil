extern "C" fn observer_mode()->i32;
extern "C" fn observer_payload();
extern "C" fn observer_prepared();
extern "C" fn observer_body();
extern "C" fn observer_drop(code:i32);
extern "C" fn observer_after(created:i32,child:i32,parent:i32);
pub fn mode()->i32 { unsafe { return observer_mode() } }
pub fn payload()->i32 { unsafe { observer_payload() };return 5 }
pub fn prepared() { unsafe { observer_prepared() } }
pub fn body() { unsafe { observer_body() } }
pub fn dropped(code:i32) { unsafe { observer_drop(code) } }
pub fn after(created:i32,child:i32,parent:i32) { unsafe { observer_after(created,child,parent) } }
