import factory
import io
fn main()->bool!i32{let handler=factory.make(false);let n=handler()?;if n!=42{return true}io.emit(83);return 0}