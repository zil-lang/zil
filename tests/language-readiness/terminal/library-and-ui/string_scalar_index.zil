import std.string
fn main()->i32 {
    match string.from_utf8("A猫🙂",32) {
        .err(_)=>{return 1},
        .ok(text)=>{
            match string.scalar_at_index(&text,0) {.err(_)=>{return 2},.ok(value)=>{if value!=65{return 3}}}
            match string.scalar_at_index(&text,1) {.err(_)=>{return 4},.ok(value)=>{if value!=29483{return 5}}}
            match string.scalar_at_index(&text,2) {.err(_)=>{return 6},.ok(value)=>{if value!=128578{return 7}}}
            match string.scalar_at_index(&text,3) {.ok(_)=>{return 8},.err(_)=>{}}
            match string.scalar_at_index(&text,-1) {.ok(_)=>{return 9},.err(_)=>{}}
            return 0
        }
    }
}
