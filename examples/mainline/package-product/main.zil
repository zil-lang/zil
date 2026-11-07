import portable
import leaf
import product
import transitions

fn main() -> void!i32 {
    let input = portable.read()
    if input < 48 || input > 55 { return 9 }
    let mode = input - 48
    var root = product.wrap(product.middle(leaf.make(65, 8)?, leaf.make(66, 16)?, 73))
    var left_data = 65
    var left_mark = 65
    var left_capacity = i64(8)
    var right_data = 66
    var right_capacity = i64(16)
    var base = 73

    if mode == 1 {
        root = transitions.take_refill(root)
        left_mark = 88
    } else if mode == 2 {
        root.child = product.middle(leaf.make(67, 24)?, leaf.make(68, 32)?, 76)
        left_data = 67
        left_mark = 67
        left_capacity = 24
        right_data = 68
        right_capacity = 32
        base = 76
    } else if mode == 3 {
        root.child = product.middle(leaf.make(67, 24)?, leaf.make(68, 32)?, portable.fail_tag()?)
        return 8
    } else if mode == 4 {
        let middle = root.child
        portable.emit(81)
        return 0
    } else if mode == 5 {
        var middle = root.child
        let left = middle.left
        portable.emit(82)
        return 0
    } else if mode == 6 {
        (root) = transitions.relay(root)
    } else if mode == 7 {
        var middle = root.child
        middle.left = leaf.make(67, 24)?
        root.child = middle
        left_data = 67
        left_mark = 67
        left_capacity = 24
    }

    root = transitions.checked(root, left_data, left_mark, left_capacity,
        right_data, right_capacity, base)
    portable.emit(80)
    return 0
}
