# Avoid a heap allocation for the first type-walk node

Stage12 baseline38a7d5a08. Analyze preflight and two independent Verify type scans initialize a heap Vec for every root type, including primitive scalars that have no children. Keep each root in a local Option and allocate the pending Vec only when the unchanged match arms enqueue child nodes.

The first root and all subsequent LIFO visits, diagnostic order, target checks, charges and depth limits must remain identical. Analyze and Verify retain separate implementations and do not exchange validation facts. No type is skipped and no new acceptance shortcut is added. Existing budget/depth/admission controls suffice for this mechanical traversal change; do not add mirrored implementation tests or a broad performance matrix.
