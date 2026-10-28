import factory
fn main()->i32{let f=factory.make();if f()!=42{return 1}return 0}