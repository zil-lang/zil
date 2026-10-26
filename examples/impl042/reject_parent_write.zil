fn answer() -> i32 {
    var value = 1;
    let parent = &mut value;
    let child = &mut *parent;
    *parent = 2;
    return *child;
}
