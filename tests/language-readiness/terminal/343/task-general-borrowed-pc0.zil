// Even a non-yielding worker retains this input from create until first poll.
fn borrowed(input:&i32)->i32 { return *input; }
fn escape()->TaskCreateError!Task<i32> contract { result(create) } {
    let local=42;
    return task_create(borrowed,&local);
}
fn main() { let task=escape(); }
