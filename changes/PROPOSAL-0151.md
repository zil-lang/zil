# 151 ordinary receiver calls inside closures

Extend finite local nominal record method syntax into closure inference, including
147 trait methods lowered to ordinary functions. No trait solving or new dispatch.
Only known Copy record receivers and their address captures; function fields and
slice len retain precedence. Inferred mutable calls flow through the existing
capture-address write and writeback logic. Existing Builder emits the ordinary
Call plus receiver borrow; independent Verify remains unchanged.

Require receiver permission/arity/type negatives, repeated FnMut calls, normal/
early return writeback, nested record fields, by-value receivers, borrowed captures,
and O0/O2 source-to-native checks. Resource-owning receiver consumption, unknown
receiver inference, cross-module method lookup and dyn remain outside this slice.
Default budgets/schema unchanged; independent review and formal acceptance NotRun.
