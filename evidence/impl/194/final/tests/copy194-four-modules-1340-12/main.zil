import model
import callbacks
import api
fn main()->i32{let f=callbacks.make();let a=api.keep(&f,model.A{n:3});let b=api.keep(&f,model.B{n:4});return a.n+b.n-7}