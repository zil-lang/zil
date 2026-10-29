import portable
import state
import factory
import collection
fn main()->void!i32{
    let input=portable.read()
    if input<48||input>52{return 9}
    let mode=input-48
    let items=collection.seed(state.make(factory.active(1),factory.once(1),1,0))
    if store_len(items)!=1{portable.emit(70);return 1}
    collection.push(items,state.make(factory.active(2),factory.once(2),2,1))
    collection.push(items,state.make(factory.active(3),factory.once(3),3,1))
    if store_len(items)!=3{return 8}
    portable.emit(83)
    if mode==4{
        collection.push(items,state.make(factory.active(4),factory.once(4),4,1))
        if store_len(items)!=3||store_capacity(items)!=3{return 8}
        portable.emit(90)
        return 0
    }
    var round=0
    while round<mode{
        var index:i64=0
        while index<store_len(items){
            let slot=store_at_mut(items,index)
            let value=slot.active(1)
            if value<0{portable.emit(70);return 1}
            if value!=slot.id*10+round+1{return 8}
            index+=1
        }
        portable.emit(46)
        round+=1
    }
    if mode>0{
        let result=collection.erase(items,1)?
        if result!=0{portable.emit(70);return result}
        if store_len(items)!=2{return 8}
        if store_at(items,0).id!=1||store_at(items,1).id!=3{return 8}
        store_at_mut(items,1).parent=0
        if store_at(items,1).parent!=0{return 8}
        portable.emit(68)
        if mode==3{portable.emit(69);return 0}
        if mode==2{
            var selected=collection.pop(items)?
            selected.active=factory.active(9)
            collection.push(items,selected)
            portable.emit(82)
        }
        let first=store_at_mut(items,0).active(1)
        if first<0{portable.emit(70);return 1}
        if first!=10+mode+1{return 8}
        let last=store_at_mut(items,1).active(1)
        if last<0{portable.emit(70);return 1}
        let expected=if mode==2{91}else{30+mode+1}
        if last!=expected{return 8}
    }
    while store_len(items)>0{
        let value=collection.pop(items)?
        let result=state.finish(value)
        if result!=0{return result}
    }
    portable.emit(90)
    return 0
}
