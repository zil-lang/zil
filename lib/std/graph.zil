// A checked, graph-relative ticket table. Keys are values, not references.
// A key from another Graph has no provenance brand and is interpreted in the
// receiving Graph's namespace. Do not use keys as cross-Graph borrowed handles.
// Every issued ticket keeps one history slot until Graph destruction. Deleted
// tickets are never reused, even when collect reuses their physical node slots.
// collect scans/transfers all nodes and can allocate one scratch Store.
// Store elements may move during growth/collect; no node address is stable.
import std.store

pub enum Error { Capacity, Allocation, Invalid }
pub enum AddFailure { Capacity, Allocation }
pub package AddError { manages payload:ByteBuffer; pub reason:AddFailure }
pub struct NodeKey { pub ticket:i64 }
pub struct NodeSnapshot { pub key:NodeKey; pub value:i64 }
pub struct EdgeSnapshot { pub from:NodeKey; pub to:NodeKey }

package Node { ticket:i64; value:i64; alive:bool; manages payload:ByteBuffer }
struct Edge { from:i64; to:i64 }
pub package Graph { manages nodes:Store<Node>; manages edges:Store<Edge>; manages locations:Store<i64> }

pub fn with_capacity(node_capacity:i64,edge_capacity:i64,ticket_capacity:i64)->Error!Graph {
    if node_capacity<0 || edge_capacity<0 || ticket_capacity<0 {return Error.Capacity}
    let nodes:Store<Node>=store_new()
    let edges:Store<Edge>=store_new()
    let locations:Store<i64>=store_new()
    if !store_reserve(nodes,node_capacity) {return Error.Allocation}
    if !store_reserve(edges,edge_capacity) {return Error.Allocation}
    if !store_reserve(locations,ticket_capacity) {return Error.Allocation}
    return Graph{nodes:give nodes,edges:give edges,locations:give locations}
}

fn slot(graph:&Graph,key:NodeKey)->i64 {
    if key.ticket<0 || key.ticket>=store_len(graph.locations) {return -1}
    let position=*store_at(graph.locations,key.ticket)
    if position<0 || position>=store_len(graph.nodes) {return -1}
    let node=store_at(graph.nodes,position)
    if !node.alive || node.ticket!=key.ticket {return -1}
    return position
}
pub fn contains(graph:&Graph,key:NodeKey)->bool {return slot(graph,key)>=0}
pub fn get(graph:&Graph,key:NodeKey)->Option<NodeSnapshot> {
    let position=slot(graph,key)
    if position<0 {return Option.None}
    let node=store_at(graph.nodes,position)
    return Option.Some(NodeSnapshot{key:key,value:node.value})
}
pub fn physical_slot(graph:&Graph,key:NodeKey)->Option<i64> {
    let position=slot(graph,key)
    if position<0 {return Option.None}
    return Option.Some(position)
}
pub fn node_count(graph:&Graph)->i64 {
    var count:i64=0
    var at:i64=0
    while at<store_len(graph.nodes) {
        if store_at(graph.nodes,at).alive {count+=1}
        at+=1
    }
    return count
}
pub fn issued_count(graph:&Graph)->i64 {return store_len(graph.locations)}

pub fn add_with_payload(graph:&mut Graph,value:i64,give payload:ByteBuffer)->AddError!NodeKey {
    if store_len(graph.nodes)>=store_capacity(graph.nodes) || store_len(graph.locations)>=store_capacity(graph.locations) {
        return AddError{payload:give payload,reason:AddFailure.Capacity}
    }
    let ticket=store_len(graph.locations)
    let position=store_len(graph.nodes)
    match store_push(graph.locations,position) {
        .err(_)=>{return AddError{payload:give payload,reason:AddFailure.Allocation}},
        .ok(_)=>{}
    }
    let node=Node{ticket:ticket,value:value,alive:true,payload:give payload}
    match store_push(graph.nodes,node) {
        .err(original)=>{
            let place=store_at_mut(graph.locations,ticket)
            *place=-1
            return AddError{payload:give original.payload,reason:AddFailure.Allocation}
        },
        .ok(_)=>{}
    }
    return NodeKey{ticket:ticket}
}
pub fn add(graph:&mut Graph,value:i64)->Error!NodeKey {
    let payload=bytes_new()
    match add_with_payload(graph,value,give payload) {
        .err(failed)=>{
            match failed.reason {
                AddFailure.Capacity()=>{return Error.Capacity},
                AddFailure.Allocation()=>{return Error.Allocation}
            }
        },
        .ok(key)=>{return key}
    }
}
pub fn recover_payload(failed:AddError)->ByteBuffer contract {result(give)} {
    return give failed.payload
}
pub fn payload_len(graph:&Graph,key:NodeKey)->Option<i64> {
    let position=slot(graph,key)
    if position<0 {return Option.None}
    let node=store_at(graph.nodes,position)
    return Option::<i64>.Some(bytes_len(node.payload))
}
pub fn payload_byte(graph:&Graph,key:NodeKey,index:i64)->Option<u8> {
    let position=slot(graph,key)
    if position<0 {return Option.None}
    let node=store_at(graph.nodes,position)
    let view=bytes_view(node.payload)
    if index<0 || index>=view.len() {return Option.None}
    let value=view[index]
    return Option::<u8>.Some(value)
}

pub fn remove(graph:&mut Graph,key:NodeKey)->bool {
    let position=slot(graph,key)
    if position<0 {return false}
    let place=store_at_mut(graph.locations,key.ticket)
    *place=-1
    let node=store_at_mut(graph.nodes,position)
    node.alive=false
    return true
}

// Explicit O(nodes) compaction. No address into a Store may remain live here.
pub fn collect(graph:&mut Graph)->Error!i64 {
    let length=store_len(graph.nodes)
    let scratch:Store<Node>=store_new()
    if !store_reserve(scratch,length) {return Error.Allocation}
    var removed:i64=0
    while store_len(graph.nodes)>0 {
        match store_pop(graph.nodes) {
            .err(_)=>{return Error.Invalid},
            .ok(node)=>{
                if node.alive {
                    match store_push(scratch,node) {
                        .err(_)=>{return Error.Invalid},
                        .ok(_)=>{}
                    }
                } else {removed+=1}
            }
        }
    }
    while store_len(scratch)>0 {
        match store_pop(scratch) {
            .err(_)=>{return Error.Invalid},
            .ok(node)=>{
                match store_push(graph.nodes,node) {
                    .err(_)=>{return Error.Invalid},
                    .ok(_)=>{}
                }
            }
        }
    }
    var position:i64=0
    while position<store_len(graph.nodes) {
        let ticket=ticket_at(graph,position)
        set_location(graph,ticket,position)
        position+=1
    }
    return removed
}
fn ticket_at(graph:&Graph,position:i64)->i64 {return store_at(graph.nodes,position).ticket}
fn set_location(graph:&mut Graph,ticket:i64,position:i64) {
    let place=store_at_mut(graph.locations,ticket)
    *place=position
}

pub fn link(graph:&mut Graph,from:NodeKey,to:NodeKey)->Error!i64 {
    if !contains(graph,from) || !contains(graph,to) {return Error.Invalid}
    if store_len(graph.edges)>=store_capacity(graph.edges) {return Error.Capacity}
    let index=store_len(graph.edges)
    match store_push(graph.edges,Edge{from:from.ticket,to:to.ticket}) {
        .err(_)=>{return Error.Allocation},
        .ok(_)=>{}
    }
    return index
}
pub fn edge(graph:&Graph,index:i64)->Option<EdgeSnapshot> {
    if index<0 || index>=store_len(graph.edges) {return Option.None}
    let raw=*store_at(graph.edges,index)
    let from=NodeKey{ticket:raw.from}
    let to=NodeKey{ticket:raw.to}
    if !contains(graph,from) || !contains(graph,to) {return Option.None}
    return Option.Some(EdgeSnapshot{from:from,to:to})
}
pub fn edge_count(graph:&Graph)->i64 {
    var count:i64=0
    var at:i64=0
    while at<store_len(graph.edges) {
        match edge(graph,at) {
            Option.None()=>{},
            Option.Some(_)=>{count+=1}
        }
        at+=1
    }
    return count
}
