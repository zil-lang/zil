import helpers
pub fn make()->impl Fn()->i64{let init=()=>42;let n=helpers.apply(&init);return move ()=>n}fn other()->impl Fn()->i64{return move ()=>99}