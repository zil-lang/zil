# Generic owning Store results

Four ordinary source modules use the same `T:StoreElement` implementation with
two different nominal Package layouts. `collection.checked` returns
`Error!Store<T>` with explicit `result(create)`. The adapters receive a `give`
success payload and forward the already formed union as a normal non-Copy Value.
The outer union is never given another `give` spelling.

The factory reserves one element, inserts it, grows to four, inserts a second
element, and can return a logical failure. The main function validates actual
padded `Item` and `Other` Copy fields through Store views. Resource Drop markers
make the successful, factory-failed, caller-failed, and discarded union paths
observable. No same-layout or user trait claim supplies Store eligibility.

Input bytes `0` through `4` select success, first factory error, second factory
error, caller error, and discard-before-extraction. Byte `9` exits before any
resource is created. `tools/language/186/consumer.py` declares exact outputs and
real allocator-failure expectations before executing either platform.

The compiler remains a bounded implementation: arbitrary owning Sum nesting,
resources in the error arm, borrowed environments, async and GUI development
are outside this consumer. Wine execution is a compatibility check, not a
native Windows certification or a Rust performance comparison.
