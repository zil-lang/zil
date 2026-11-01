import portable
import model
import collections

pub package NodeSet { manages items: Store<model.Node>; pub ready: bool }
pub package SubscriptionSet { manages items: Store<model.Subscription>; pub ready: bool }
// Both children own their complete Store. Field order is the shutdown order.
pub package Session {
    pub subscriptions: SubscriptionSet
    pub nodes: NodeSet
    pub ready: bool
}

fn nodes(count: i32) -> NodeSet {
    let items: Store<model.Node> = store_new()
    if count > 0 && !collections.reserve(items, i64(count)) {
        portable.emit(70)
        return NodeSet { items: give items, ready: false }
    }
    var index = 0
    while index < count {
        let value = model.node(index)
        if !value.ready { return NodeSet { items: give items, ready: false } }
        collections.push(items, value)
        index += 1
    }
    return NodeSet { items: give items, ready: true }
}
fn subscriptions(count: i32, start: i32) -> SubscriptionSet {
    let items: Store<model.Subscription> = store_new()
    if count > 0 && !collections.reserve(items, i64(count)) {
        portable.emit(70)
        return SubscriptionSet { items: give items, ready: false }
    }
    var index = 0
    while index < count {
        let value = model.subscription(index, start)
        if !value.ready { return SubscriptionSet { items: give items, ready: false } }
        collections.push(items, value)
        index += 1
    }
    return SubscriptionSet { items: give items, ready: true }
}
pub fn create(count: i32) -> Session {
    let prepared_nodes = nodes(count)
    if !prepared_nodes.ready {
        return Session {
            subscriptions: subscriptions(0, 97), nodes: prepared_nodes, ready: false
        }
    }
    let prepared_subscriptions = subscriptions(count, 97)
    let ready = prepared_subscriptions.ready
    return Session {
        subscriptions: prepared_subscriptions, nodes: prepared_nodes, ready: ready
    }
}

// Deep children first become ordinary complete locals. No deep Place extension.
pub fn checked(owned: Session, count: i32, start: i32) -> Session {
    var current = owned
    let node_set = current.nodes
    let subscription_set = current.subscriptions
    if store_len(node_set.items) != i64(count) || store_len(subscription_set.items) != i64(count) {
        portable.emit(33)
        current.ready = false
    }
    var index: i64 = 0
    while index < store_len(node_set.items) {
        let view = store_at(node_set.items, index)
        if view.id != i32(index) || !view.ready {
            portable.emit(33)
            current.ready = false
        }
        index += 1
    }
    index = 0
    while index < store_len(subscription_set.items) {
        let view = store_at(subscription_set.items, index)
        if view.id != i32(index) || !view.ready || view.start != start {
            portable.emit(33)
            current.ready = false
        }
        index += 1
    }
    current.nodes = node_set
    current.subscriptions = subscription_set
    return current
}

pub fn grow(owned: Session, capacity: i64) -> Session {
    var current = owned
    let node_set = current.nodes
    let ready = collections.reserve(node_set.items, capacity)
    current.nodes = node_set
    current.ready = ready
    if !ready { portable.emit(70) }
    return current
}

pub fn replace(owned: Session, count: i32) -> Session {
    var current = owned
    let replacement = subscriptions(count, 109)
    if !replacement.ready {
        current.ready = false
        // Failure drops only the prepared prefix. The old child remains owned.
        return checked(current, count, 97)
    }
    // Preparation finishes before the old child is closed and the new published.
    current.subscriptions = replacement
    portable.emit(82)
    return checked(current, count, 109)
}

fn held_then_move(owned: Session) -> Session {
var current = owned
let child = current.nodes
let held = store_at(child.items, 0)
current.nodes = child
portable.emit(held.id)
return current
}
