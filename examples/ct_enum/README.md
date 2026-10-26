# Compile-time enum plan, runtime byte input

`model.prepare()` runs a pure compile-time loop which constructs a tagged Plan
containing a record, a fixed array and nested Token values. The result is
materialized as ordinary untrusted Core constructors, checked again, then used
by the runtime transform module with an actual input byte.

For an even byte, the output is `(byte + 3) % 256`; for an odd byte it is
`(byte + 40) % 256`. Empty input exits 1 without output; output failure exits 2.
The 105 tests build and execute this three-module program at O0/O2, retain the
phase artifacts and compare dynamic bytes to this independent integer rule.
This is functional evidence, without a Rust cost or product-acceptance claim.
