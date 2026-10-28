import factory
fn value()->i64{var first=factory.make(10);var second=factory.make(20);let a=first(2);let b=second(3);return a+b+first(1)}