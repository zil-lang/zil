# Main function-cache slice: four-block diagnostic

Candidate `aff75567688d493cbd3d3f006f6952d6923bb6c0` versus main `3ec7e8a960ca69368cd9a90f1a15ac063fec5a02`. No Rust arm. Both use original LLVM23.1.4, requested CGU16/O2/jobs1, cacheall/cap256, no research/profile flags. Check and native build are separate.

Ratios are candidate/main; below1 favors candidate. Four pairs per cell do not establish statistical nonregression or speedup. Both signs and all blocks remain in the JSON.

## Check

| Request | Main median ms | Candidate median ms | Paired GM | B0 | B1 | B2 | B3 |
|---|---:|---:|---:|---:|---:|---:|---:|
| cold | 1105.878 | 1047.062 | 0.928192 | 1.067 | 1.167 | 0.473 | 1.261 |
| unchanged | 81.436 | 89.457 | 1.116581 | 1.218 | 0.999 | 1.140 | 1.121 |
| private | 684.675 | 782.582 | 0.958164 | 1.273 | 1.333 | 0.405 | 1.225 |
| multi | 603.693 | 709.248 | 1.092103 | 1.028 | 1.638 | 0.569 | 1.484 |
| broad | 605.744 | 739.885 | 1.070740 | 0.931 | 1.223 | 0.946 | 1.220 |
| positions | 811.943 | 1009.857 | 1.143793 | 1.171 | 0.966 | 1.546 | 0.979 |
| error | 591.280 | 642.470 | 1.083466 | 1.094 | 1.476 | 0.770 | 1.108 |
| recovery | 652.300 | 619.161 | 0.958900 | 0.912 | 1.030 | 0.688 | 1.308 |
| restore | 92.887 | 83.436 | 0.896186 | 1.026 | 0.921 | 0.859 | 0.794 |

## Native O2

| Request | Main median ms | Candidate median ms | Paired GM | B0 | B1 | B2 | B3 |
|---|---:|---:|---:|---:|---:|---:|---:|
| cold | 5987.858 | 5490.395 | 0.922695 | 0.840 | 0.954 | 0.832 | 1.087 |
| unchanged | 493.769 | 486.430 | 1.071556 | 0.626 | 0.981 | 0.992 | 2.164 |
| private | 1495.288 | 1750.367 | 1.091976 | 1.152 | 1.339 | 0.836 | 1.104 |
| multi | 1923.423 | 1810.749 | 0.976148 | 0.952 | 0.941 | 0.898 | 1.129 |
| broad | 5161.916 | 5185.230 | 0.996649 | 0.955 | 1.116 | 0.978 | 0.947 |
| positions | 1591.385 | 1504.424 | 0.937865 | 0.895 | 0.961 | 1.109 | 0.811 |
| error | 324.846 | 309.842 | 0.974718 | 0.945 | 0.997 | 1.041 | 0.921 |
| recovery | 1488.402 | 1473.359 | 0.965647 | 0.899 | 0.988 | 1.014 | 0.966 |
| restore | 476.814 | 470.204 | 0.973048 | 0.978 | 0.895 | 1.010 | 1.014 |

All272rows qualify within the stated evidence limits: each mode136rows/72measured/64parents; native720 actual outcomes match the fixed integer oracle. Each successful request ran current independent Verify once. The actual16CGU definition partition is matched by decoded layout indices and physical object symbols; all960 corresponding object payload pairs match. CGU16 alone is not used as that proof.

All24 full LLVM blobs were physically rehashed. Recorded ELF hashes match across60 success pairs; only8 final restore executables remain physically, representing1 of5 ELF contents and80 of120 recorded successful rows. Earlier4 ELF contents cannot now be rehashed from original bytes. Native outcomes and recorded hashes are retained; no historical ELF reconstruction was performed.

Mixed positive/negative observations, substantial variation and four pairs per request. Private edit is slower in3/4 check blocks and3/4 O2 blocks; check private favorable GM is driven by one0.405 block despite adverse median/majority. No statistical nonregression, speedup, causal performance attribution or Rust superiority established. First single-block negative V24 remains separate and is not pooled or discarded. Main merge/hold decision is root/Astra responsibility.

Detailed per-block main/candidate ms, savedms and ratios, actual hit/miss controls and fixed receipts are in the adjacent JSON. First single-block V24 evidence stays separate.
