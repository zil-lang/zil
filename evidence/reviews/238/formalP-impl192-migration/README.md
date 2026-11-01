# P integration boundary triage and exact192 patch

The original192 no-loan source now legitimately succeeds under238. Its paired active-field-loan source still reaches independent Verify and rejects with E-BORROW-CONFLICT. The tree-outside patch splits these expectations and adds O0/O2 execution of the unchanged successful source (exit20).

The exact source probe passed1/0; the complete patched test file compiled in an isolated Cargo adapter with its original include layout, and its two migrated tests passed with19 other tests filtered. The first include-layout-only harness failure is retained. Core and production were never modified, and no whole-workspace test was run.

The observed P full-run failures are classified in RESULT.json: only192 is the newly opened source boundary. Three private-field and five captured-Package positive failures are real production regressions; they must not be converted to expected failures. The remaining two failures are explicit old public-protocol fixture assumptions and need a preserved version boundary plus meaningful current semantic replay.
