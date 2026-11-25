// Pure ZIL POSIX-style byte path manipulation helpers.
import std.io
import std.store

struct Segment { start:i64; end:i64 }

fn copy_bytes(src:&[u8])->io.Error!ByteBuffer contract {result(create)} {
    let out=bytes_new()
    if !bytes_reserve(out,src.len()) {return io.Error.Allocation}
    var at:i64=0
    while at<src.len() {
        if !bytes_push(out,src[at]) {return io.Error.InvalidResult}
        at+=1
    }
    return give out
}

fn seg_is_dotdot(path:&[u8],seg:Segment)->bool {
    return seg.end-seg.start==2 && path[seg.start]==46u8 && path[seg.start+1]==46u8
}

// Joins base and child with a single '/' separator.
// An absolute child (starting with '/') replaces base; empty inputs return the other side.
pub fn join(base:&[u8],child:&[u8])->io.Error!ByteBuffer contract {result(create)} {
    if child.len()>0 && child[0]==47u8 {return copy_bytes(child)}
    if base.len()==0 {return copy_bytes(child)}
    if child.len()==0 {return copy_bytes(base)}
    let need_sep=base[base.len()-1]!=47u8
    var sep_len:i64=0
    if need_sep {sep_len=1}
    if base.len()>9223372036854775807-sep_len {return io.Error.Capacity}
    let prefix_len=base.len()+sep_len
    if prefix_len>9223372036854775807-child.len() {return io.Error.Capacity}
    let total=prefix_len+child.len()
    let out=bytes_new()
    if !bytes_reserve(out,total) {return io.Error.Allocation}
    var at:i64=0
    while at<base.len() {
        if !bytes_push(out,base[at]) {return io.Error.InvalidResult}
        at+=1
    }
    if need_sep {
        if !bytes_push(out,47u8) {return io.Error.InvalidResult}
    }
    at=0
    while at<child.len() {
        if !bytes_push(out,child[at]) {return io.Error.InvalidResult}
        at+=1
    }
    return give out
}

// Returns the last non-empty path component after stripping trailing '/',
// or an empty slice for "/" or "".
pub fn file_name(path:&[u8])->&[u8] {
    var end=path.len()
    while end>0 && path[end-1]==47u8 {end-=1}
    if end==0 {return &path[0..0]}
    var start=end
    while start>0 && path[start-1]!=47u8 {start-=1}
    return &path[start..end]
}

// Returns the parent directory slice: "/" for "/foo", "" for "foo" or "/".
pub fn parent(path:&[u8])->&[u8] {
    var end=path.len()
    while end>0 && path[end-1]==47u8 {end-=1}
    if end==0 {return &path[0..0]}
    var cut=end
    while cut>0 && path[cut-1]!=47u8 {cut-=1}
    if cut==0 {return &path[0..0]}
    var parent_end=cut
    while parent_end>1 && path[parent_end-1]==47u8 {parent_end-=1}
    return &path[0..parent_end]
}

// Returns the extension after the last '.' in file_name, excluding leading dot files
// such as ".gitignore".
pub fn extension(path:&[u8])->&[u8] {
    let name=file_name(path)
    if name.len()<=1 {return &name[0..0]}
    var at=name.len()-1
    while at>0 {
        if name[at]==46u8 {
            return &name[at+1..name.len()]
        }
        at-=1
    }
    return &name[0..0]
}

// Collapses repeated '/', resolves '.' segments, and resolves safe '..' segments
// (clamping at '/' for absolute paths and preserving leading '..' for relative paths).
pub fn normalize(path:&[u8])->io.Error!ByteBuffer contract {result(create)} {
    if path.len()==0 {
        let empty=bytes_new()
        return give empty
    }
    let is_abs=path[0]==47u8
    let stack:Store<Segment>=store_new()
    var at:i64=0
    while at<path.len() {
        while at<path.len() && path[at]==47u8 {at+=1}
        if at>=path.len() {break}
        let start=at
        while at<path.len() && path[at]!=47u8 {at+=1}
        let seg=Segment{start:start,end:at}
        let seg_len=at-start
        if seg_len==1 && path[start]==46u8 {
            // Skip "." segment.
        } else if seg_len==2 && path[start]==46u8 && path[start+1]==46u8 {
            let depth=store_len(stack)
            if depth>0 {
                let top=*store_at(stack,depth-1)
                if seg_is_dotdot(path,top) {
                    match store.push_growing(stack,seg) {
                        .err(_)=>{return io.Error.Allocation},
                        .ok(_)=>{}
                    }
                } else {
                    store_pop(stack)
                }
            } else if !is_abs {
                match store.push_growing(stack,seg) {
                    .err(_)=>{return io.Error.Allocation},
                    .ok(_)=>{}
                }
            }
        } else {
            match store.push_growing(stack,seg) {
                .err(_)=>{return io.Error.Allocation},
                .ok(_)=>{}
            }
        }
    }
    let count=store_len(stack)
    if count==0 {
        let out=bytes_new()
        if !bytes_reserve(out,1) {return io.Error.Allocation}
        var ch:u8=46u8
        if is_abs {ch=47u8}
        if !bytes_push(out,ch) {return io.Error.InvalidResult}
        return give out
    }
    var total:i64=0
    if is_abs {total=1}
    var idx:i64=0
    while idx<count {
        if idx>0 {
            if total==9223372036854775807 {return io.Error.Capacity}
            total+=1
        }
        let seg=*store_at(stack,idx)
        let len=seg.end-seg.start
        if total>9223372036854775807-len {return io.Error.Capacity}
        total+=len
        idx+=1
    }
    let out=bytes_new()
    if !bytes_reserve(out,total) {return io.Error.Allocation}
    if is_abs {
        if !bytes_push(out,47u8) {return io.Error.InvalidResult}
    }
    idx=0
    while idx<count {
        if idx>0 {
            if !bytes_push(out,47u8) {return io.Error.InvalidResult}
        }
        let seg=*store_at(stack,idx)
        var pos=seg.start
        while pos<seg.end {
            if !bytes_push(out,path[pos]) {return io.Error.InvalidResult}
            pos+=1
        }
        idx+=1
    }
    return give out
}
