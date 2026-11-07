import portable
import leaf
import product

pub fn take_refill(owned: product.Root) -> product.Root {
    var root = owned
    var middle = root.child
    let left = middle.left
    let complete = leaf.refill_marker(left)
    middle.left = complete
    root.child = middle
    return root
}

pub fn relay(owned: product.Root) -> product.Root {
    portable.emit(72)
    return owned
}

pub fn checked(owned: product.Root, left_data: i32, left_mark: i32,
    left_capacity: i64, right_data: i32, right_capacity: i64, base: i32) -> product.Root {
    var root = owned
    var middle = root.child
    if middle.stamp != i64(base) * i64(1000000000) + 29 { portable.emit(33) }
    let left = leaf.checked(middle.left, left_data, left_mark, left_capacity)
    let right = leaf.checked(middle.right, right_data, right_data, right_capacity)
    middle.left = left
    middle.right = right
    root.child = middle
    return root
}
