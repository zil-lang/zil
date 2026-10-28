import dep
pub fn apply<F:Fn()->i64>(f:&F)->i64{let independent=other();let imported=dep.other();return f()}fn other()->impl Fn()->i64{return move ()=>7}