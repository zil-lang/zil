# Borrowing a complete owning Product into an environment

A non-`move` closure may capture an ordinary complete owning Package through an actual address. A read-only body captures a shared address. A body that writes fields captures an exclusive address and requires an exclusive environment entry. Consuming a captured owner still requires an explicit owning transfer. The captured address has no cleanup debt for the Package or its fields.

This includes finite ordinary Store fields, including fields in nested ordinary Packages. Each actual Store must satisfy its independent canonical resource and element contract. Knowing that a Package owns a Store does not make the Package a Store element, qualify an arbitrary callback environment, permit borrowed escape, or grant access to an uninitialized field.

Types classifies the complete body before choosing the capture mode. Analyze and Verify independently check the complete actual owner type, the exact MakeClosure operand, the input permission, the environment entry kind, and the actual source roots and loan parents. A call that also borrows the same owner must satisfy those actual simultaneous permissions. Same-typed distinct owners remain distinct, and an exclusive capture cannot promote a shared input address.

The existing Address, MakeClosure and closure-call instructions and their lifetime checks carry this behavior. No serialized proof bit or new schema is introduced.
