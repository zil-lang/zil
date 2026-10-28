# 150 reproduction

Use tools/research/150/{prepare,freeze,campaign,summarize,check_native}.py with the
same CLI as 146. Baseline is compose-149; candidate is sandbox/research-150.
Each has a separate absolute CARGO_TARGET_DIR; rebuild pipeline123 from each
exact tree. Preflight functional success before timing; no builds while timing.
All timing and failure samples are retained. See PROPOSAL-0150 for proof scope.
