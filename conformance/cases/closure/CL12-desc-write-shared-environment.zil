struct Node {
    value: i64
}
fn twice<F: Fn() -> void>(callback: F) {
    callback()
    callback()
}
fn check(node: Ref<Node>) -> i64 {
    let update = () => {
        node.value += 1
    }
    twice(update)
    return node.value
}
