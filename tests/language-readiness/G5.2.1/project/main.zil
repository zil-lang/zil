import a;
import b;
fn main()->i32 {
    let left=a.Meter{n:20};
    let right=b.Meter{n:22};
    return a.read_it(&left)+b.read_it(&right)
}
