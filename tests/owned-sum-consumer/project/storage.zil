import model
pub fn set(count:i32,base:i32)->model.Fault!model.Items{
    let values:Store<model.Leaf>=store_new()
    if !store_reserve(values,i64(count)){
        return model.failure(model.items(give values))
    }
    for i in 0..count{
        let mark=model.Mark{code:base+i}
        let bytes=bytes_new()
        if !bytes_reserve(bytes,1){
            return model.failure(model.items(give values))
        }
        bytes_push(bytes,7u8)
        store_push(values,model.leaf(give mark,give bytes))
    }
    return model.items(give values)
}
pub fn session(count:i32,fail:bool,left:i32,right:i32)->model.Fault!model.Session{
    let subscriptions=set(count,left)?
    if fail{return model.failure(subscriptions)}
    let nodes=set(count,right)?
    return model.complete(subscriptions,nodes)
}
