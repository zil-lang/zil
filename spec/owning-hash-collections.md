# Owning hash collections

`std.owning_hash_map.OwningHashMap<K,V>` owns complete `Entry<K,V>` values.
`K` requires `StoreElement`, `std.owning_map.KeyEq` and
`std.owning_hash_map.KeyHash`; `V` requires `StoreElement`. These bounds retain
the ordinary concrete element checks and do not manufacture Copy or borrowing
permissions. `same(&Self,&Self)` and `hash(&Self)->i64` must remain stable while
the key is stored, and equal keys must hash equally.

Lookup uses an open-addressed bucket table with linear collision probing,
including equality checks, tombstones and signed-remainder normalization.
`i64::MIN` hashes are valid. It never falls back to scanning all entries.
Dense entries own keys and values; buckets store indices only. A zero-capacity
new map allocates nothing and has no buckets until its first insertion.

`insert(map, entry)->Entry!bool` grows geometrically for new keys. Success is
`true` when an existing entry was replaced, `false` when a new key was added.
Replacement consumes the old complete key/value pair once and retains the new
representative. Its suffix preparation may fail; the error returns the incoming
entry intact and retains the old map. Existing-key replacement does not grow.

`reserve(map, n)->Error!void` prepares and rehashes a complete independent index
before reserving the dense owning entries. The entry reserve is the final
fallible operation. Allocation failure preserves old entries, indices and logical
capacity; successful commit transfers the prepared index and updates capacity.
Keys and values are never copied to perform rehash. Hash callback effects are not
rolled back by allocation failure. Capacity is an absolute count from zero to
`2^62-1`; larger/negative requests reject before arithmetic.

`get` returns `Option<&V>` tied to the actual map. `key_at` returns `Option<&K>`
for the current dense index. These windows pin the actual backing owner until
their last use, including across helpers; they are not independently valid
handles. `take(map,key)->void!Entry<K,V>` transfers the actual owning key and value.
A miss or suffix allocation failure returns Unit with no map changes. Removal
preserves dense order and reindexes buckets, so removal is O(capacity) and may
allocate a suffix; lookup uses hashing. Dense indices are snapshots.

`clear` drops all entries once and clears tombstones, retaining backing
allocations and logical capacity for reuse. `len` and `capacity` inspect the map.

`std.owning_hash_set.OwningHashSet<K>` wraps the same map with boolean values.
`insert(set,key)->K!bool` returns true for a new key, false for replacement; failure
returns the actual incoming key. `take(set,key)->void!K` transfers the stored key.
`contains`, `key_at`, `reserve`, `clear`, `len` and `capacity` retain the map's
lookup, borrowing, allocation and cleanup rules. None of these APIs grants
thread sharing or stable element addresses.
