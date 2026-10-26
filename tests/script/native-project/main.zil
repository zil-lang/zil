import app
import io
import lifetime
fn main() -> app.Fault!i32 {
    let root = lifetime.token(90)
    io.emit(83)
    let result = app.work(io.read())?
    io.emit(69)
    return result
}
fn wide() -> i64 { return 1099511628800 }
