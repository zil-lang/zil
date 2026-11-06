import api;pub fn make<F:api.Factory<i32>>(value:i32)->F{return F.make(value)}
