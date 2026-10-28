import factory
fn value()->i64{var handler=factory.make(10);let a=handler(2);return a+handler(3)}