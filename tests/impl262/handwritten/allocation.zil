import common.model as model
pub package WorkItem {
    manages ticket:model.Ticket;
    manages data:ByteBuffer;
    pub code:i32;
}
pub fn make_generated(stop:bool)->void!WorkItem {
    let ticket=model.Ticket{code:70};
    let data=bytes_new();
    if stop { return () }
    if !bytes_reserve(data,1) || !bytes_push(data,u8(88)) { return () }
    return WorkItem{ticket:give ticket,data:give data,code:88}
}
pub fn check_generated(item:WorkItem)->i32 {
    if bytes_len(item.data)!=1 || item.code!=88 { return 9 }
    let view=bytes_view(item.data);
    if view[0]!=u8(88) { return 8 }
    return 0
}
