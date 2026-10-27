// Fixed-width Copy byte records over an explicitly owned ByteBuffer.
// This is an ordinary library, NOT Vec<T>, a resource-owning element container,
// or a permission to reinterpret bytes as native pointers/records.
// Every public operation checks the representation, so malformed external byte
// edits cannot create an out-of-bounds record. Returned windows borrow storage.

pub fn count(storage:ByteBuffer, width:i64)->i64 {
    let length=bytes_len(storage)
    if width<=0 || length%width!=0 {return -1}
    return length/width
}

// byte_limit bounds logical length and new allocation requests; an already
// larger capacity is retained. Failure leaves bytes/length/capacity unchanged.
fn prepare(storage:ByteBuffer, width:i64, extra:i64, byte_limit:i64)->bool {
    let n=count(storage,width)
    if n<0 || extra<0 || byte_limit<0 {return false}
    let length=bytes_len(storage)
    if length>byte_limit || extra>(byte_limit-length)/width {return false}
    let required=length+extra*width
    let capacity=bytes_capacity(storage)
    if required<=capacity {return true}
    var next=capacity
    if next==0 {next=width}
    if next<=byte_limit/2 {next*=2}else{next=byte_limit}
    if next<required {next=required}
    return bytes_reserve(storage,next)
}

pub fn reserve(storage:ByteBuffer,width:i64,additional:i64,byte_limit:i64)->bool {
    return prepare(storage,width,additional,byte_limit)
}

// Empty means invalid index/representation; valid records always have width>0.
pub fn at(storage:ByteBuffer,width:i64,index:i64)->&[u8] {
    let n=count(storage,width)
    let data=bytes_view(storage)
    if n<0 || index<0 || index>=n {return &data[0..0]}
    let first=index*width
    return &data[first..first+width]
}

fn copy_to(data:&mut[u8],offset:i64,item:&[u8]) {
    var at:i64=0
    while at<item.len() {data[offset+at]=item[at];at+=1}
}

pub fn set(storage:ByteBuffer,width:i64,index:i64,item:&[u8])->bool {
    let n=count(storage,width)
    if n<0 || index<0 || index>=n || item.len()!=width {return false}
    copy_to(bytes_view_mut(storage),index*width,item)
    return true
}

pub fn push(storage:ByteBuffer,width:i64,item:&[u8],byte_limit:i64)->bool {
    if width<=0 || item.len()!=width {return false}
    if !prepare(storage,width,1,byte_limit) {return false}
    // The checked reservation established enough capacity. Each primitive
    // initializes one byte before publishing its length; no user callback runs.
    var at:i64=0
    while at<width {bytes_push(storage,item[at]);at+=1}
    return true
}

fn shift_right(data:&mut[u8],start:i64,old_length:i64,width:i64,item:&[u8]) {
    var at=old_length
    while at>start {at-=1;data[at+width]=data[at]}
    copy_to(data,start,item)
}

pub fn insert(storage:ByteBuffer,width:i64,index:i64,item:&[u8],byte_limit:i64)->bool {
    let n=count(storage,width)
    if n<0 || index<0 || index>n || item.len()!=width {return false}
    let old_length=bytes_len(storage)
    if !prepare(storage,width,1,byte_limit) {return false}
    var at:i64=0
    while at<width {bytes_push(storage,0u8);at+=1}
    shift_right(bytes_view_mut(storage),index*width,old_length,width,item)
    return true
}

fn shift_left(data:&mut[u8],first:i64,width:i64) {
    var at=first+width
    while at<data.len() {data[at-width]=data[at];at+=1}
}

pub fn remove(storage:ByteBuffer,width:i64,index:i64)->bool {
    let n=count(storage,width)
    if n<0 || index<0 || index>=n {return false}
    let new_length=bytes_len(storage)-width
    shift_left(bytes_view_mut(storage),index*width,width)
    // The mutable window has ended before changing the owning header.
    return bytes_truncate(storage,new_length)
}

fn move_last(data:&mut[u8],first:i64,width:i64) {
    let last=data.len()-width
    var at:i64=0
    while at<width {data[first+at]=data[last+at];at+=1}
}

pub fn swap_remove(storage:ByteBuffer,width:i64,index:i64)->bool {
    let n=count(storage,width)
    if n<0 || index<0 || index>=n {return false}
    let new_length=bytes_len(storage)-width
    if index<n-1 {move_last(bytes_view_mut(storage),index*width,width)}
    return bytes_truncate(storage,new_length)
}

pub fn pop(storage:ByteBuffer,width:i64)->bool {
    let n=count(storage,width)
    if n<=0 {return false}
    return bytes_truncate(storage,bytes_len(storage)-width)
}

pub fn clear(storage:ByteBuffer,width:i64)->bool {
    if count(storage,width)<0 {return false}
    return bytes_truncate(storage,0)
}
