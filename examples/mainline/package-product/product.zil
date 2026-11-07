import portable
import leaf

pub package Middle {
    manages before: portable.Marker
    pub left: leaf.Leaf
    manages between: portable.Marker
    pub right: leaf.Leaf
    manages after: portable.Marker
    pub stamp: i64
}
// No direct manages field: ownership comes entirely from this complete child.
pub package Root { pub child: Middle }

pub fn middle(left: leaf.Leaf, right: leaf.Leaf, base: i32) -> Middle {
    return Middle {
        before: portable.Marker { code: base }, left: left,
        between: portable.Marker { code: base + 1 }, right: right,
        after: portable.Marker { code: base + 2 }, stamp: i64(base) * i64(1000000000) + 29
    }
}
pub fn wrap(child: Middle) -> Root { return Root { child: child } }
