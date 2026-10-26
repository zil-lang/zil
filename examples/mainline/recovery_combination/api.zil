import model
pub type Number=model.Cell<i64>;
pub type Switch=model.Flag<bool>;
pub fn apply<F:Fn(Number,Switch)->Number>(f:&F,n:Number,s:Switch)->Number{return f(n,s)}
