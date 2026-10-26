import model
import process

pub fn result() -> i64 {
    let row = model.Row::<i64> { value: 5000000000, weight: 40 }
    let value: model.MaybeRow = model.some(row)
    let score = process.convert(value)
    let unused: model.Option<i64> = model.empty()
    let opaque = #(model.secret(0))
    let extra = model.unwrap(unused, opaque.visible) + model.read(opaque)
    match score {
        model.Score.Ok(n) => { return n + extra }
        model.Score.Err(_) => { return 0 }
    }
}

fn main() -> i32 {
    if result() == 5000000042 { return 0 }
    return 1
}
