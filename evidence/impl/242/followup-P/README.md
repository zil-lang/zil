# 242 P author follow-up

Author repairs are bound to 41349ed2306e159695ded1b409100039d2d16048; precise legacy diagnostic wording and three real helper source controls are 18e5fdd861fa12db39c7b7dbf9c5283c89344fcd. This is a Verify author receipt, not independent review. Existing ecd/da386 evidence is unchanged.

The four new correct-expectation attack tests failed on P-equivalent Verify production (32/4), then passed after repair (36/0). The complete library is 295/0, not 295 plus 36. Original 183 failed only because the new Unsupported explanation omitted the asserted word `type`; the diagnostic now states the precise Copy-error/type restriction and its original 5 tests pass unchanged. The separate final source/223 run is 5/0 and 2/0. An intermediate newly written source test omitted a match-arm newline; its parse failure is retained and is not a compiler defect.

Helper sources use Parse, Expand, Types and independently constructed initialization/Hold candidate rows, then direct raw and Wire Verify; Analyze is never called in impl242_source_verify. Enum, Product<Enum> and ErrorUnion<Product<Enum>> inputs pass, including real manages-owner forwarding through `let saved = give a`. No native execution or whole-workspace acceptance is claimed.

REPORT.json records commands, selections, provenance and limits. source-sha256.json binds Verify and source-test files; deterministic gzip logs and source/TypedIR/Wire helper artifacts are retained. Committed raw test generators reproduce the library attacks.
