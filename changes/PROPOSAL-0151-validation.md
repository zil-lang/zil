# 151 validation boundary

Ordinary Copy record methods in closure bodies resolve through the existing
concrete method signatures in BOTH pre-CT inference and local closure inference.
Receiver mutability marks the existing capture path; final method lowering and
independent Verify remain unchanged. Fields win over same-named methods, and
slice length retains its dedicated rule. Bound method values are not introduced.

7 new tests cover shared/move reads, owned FnMut persistence and early return,
nonmove mutation writeback, nested record fields and borrowed mutable receivers,
by-value methods, callable fields, slice len, mutable permission failures and
signature failures. 116 related tests passed in the final focused regression run.

Unknown nominal receiver inference, resource/Package receivers, cross-module
method resolution, dynamic dispatch and storing a bound method remain outside
this slice. This does not implement generic trait constraints or resource self
replacement. Independent review and formal whole-product gates remain NotRun.
