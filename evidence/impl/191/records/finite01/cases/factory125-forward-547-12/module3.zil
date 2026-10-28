import helpers
pub fn make(seed:i64)->impl Fn()->i64{let init=(n:i64)=>n+2;let state=helpers.apply(&init,seed);return move ()=>state}