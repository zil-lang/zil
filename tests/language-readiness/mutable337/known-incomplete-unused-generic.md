This exact source is a future regression for definition-time generic exclusivity,
outside337. Baseline B accepts the unused definition; instantiating the same
definition with i64 rejects at Verify/E-BORROW-CONFLICT. This is not a native UB
claim. Mutable life with ordinary type parameters must remain Unsupported until
the unused-definition obligation is implemented; shared331 remains supported.
