import std.syntax as syntax
import common.io as io
pub fn drop_impl(t:Type)->Syntax<ItemList> {
    return quote<ItemList>{
        impl Drop for $(syntax.type_ref(t)) {
            fn drop(self:Self) { io.emit(.code) }
        }
    }
}
