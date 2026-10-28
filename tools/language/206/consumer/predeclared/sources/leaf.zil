import portable

// Existing flat owning leaf shape and existing flat Package error payload.
pub package Leaf {
    lead: u8
    manages mark: portable.Marker
    wide: i64
    manages bytes: ByteBuffer
    tail: u16
}

pub fn make(code: i32, capacity: i64) -> void!Leaf {
    let mark = portable.Marker { code: code }
    let bytes = bytes_new()
    if !bytes_reserve(bytes, capacity) { portable.emit(70); return () }
    bytes_push(bytes, 7u8)
    return Leaf {
        lead: 7u8, mark: give mark, wide: i64(code) * i64(1000000000) + 17,
        bytes: give bytes, tail: 513u16
    }
}

pub fn checked(owned: Leaf, data_code: i32, marker_code: i32, capacity: i64) -> Leaf {
    if owned.lead != 7u8 || owned.tail != 513u16
        || owned.wide != i64(data_code) * i64(1000000000) + 17
        || owned.mark.code != marker_code
        || bytes_len(owned.bytes) != 1 || bytes_capacity(owned.bytes) != capacity {
        portable.emit(33)
    }
    return owned
}

pub fn refill_marker(owned: Leaf) -> Leaf {
    var current = owned
    let prior = give current.mark
    current.mark = portable.Marker { code: 88 }
    portable.emit(116)
    // The independent old Marker A is dropped after t, before the caller
    // receives the now-complete Leaf. The ByteBuffer keeps its old owner.
    return current
}
