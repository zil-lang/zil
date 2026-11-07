# Primary sources and attribution

Retrieved 2026-10-23 UTC. These are primary papers. No secondary explanation is
used as authority for the mapping.

* Yu Deng, Zaher Hani, Xiao Ma, *Long time derivation of the Boltzmann equation
  from hard sphere dynamics*, [arXiv:2408.07818v3](https://arxiv.org/abs/2408.07818v3),
  revised 2025-07-18; [PDF](https://arxiv.org/pdf/2408.07818v3). Relevant anchors:
  §2.3, Definitions 8.3/8.11/8.12, Proposition 8.14 and Remark 8.15; Proposition
  9.7; Definition 11.10 and Proposition 11.12; Definition 12.1 and Proposition
  12.2; §11.7.4's forest/layer preprocessing qualifications. The newly added toy
  exposition in §11 is not a replacement for the full paper's hypotheses.
* Randal E. Bryant, *Graph-Based Algorithms for Boolean Function Manipulation*,
  IEEE Transactions on Computers 35(8), 1986,
  [author's PDF](https://www.cs.cmu.edu/~bryant/pubdir/ieeetc86.pdf).
  Reduced ordered decision diagrams and graph operations are established here;
  exponential worst cases and variable ordering matter. We claim no invention of
  this representation.
* Rina Dechter, *Bucket Elimination: A Unifying Framework for Reasoning*, Artificial
  Intelligence 113, 1999, [author's PDF](https://ics.uci.edu/~csp/r48b.pdf).
  Variable elimination, interaction/induced width and space/time tradeoffs are
  prior art. Width bounds concern the actual factor interaction graph and domains,
  not merely the CFG.
* J. R. Burch, E. M. Clarke, K. L. McMillan, D. L. Dill, L. J. Hwang,
  *Symbolic Model Checking: 10^20 States and Beyond*, LICS 1990,
  [author-hosted proceedings PDF](https://www.cs.cmu.edu/~emc/papers/Conference%20Papers/symbolic%20model%20checking%2010%20%20states%20and%20beyond.pdf).
  Symbolic reachable fixed points are established prior art. This PDF's text layer
  is poorly decoded by the retrieval tool; no detailed theorem transcription is
  relied on without visual verification.
* P. Chauhan, E. Clarke, S. Jha, J. Kukula, H. Veith, D. Wang,
  *Using Combinatorial Optimization Methods for Quantification Scheduling*,
  CHARME 2001, [author PDF](https://www.cs.cmu.edu/~pchauhan/acads/charme01.pdf).
  §§2–4 treat the same relational-image equation, partitioned transition relations,
  early quantification and scheduling metrics. Planning can be amortized, and a
  schedule proxy does not determine actual BDD cost. Our mono/cut/wire experiment
  is an internal same-representation ablation, not a comparison against optimized
  partitioned/fused relational product implementations or a new quantification method.

The user's earlier attachment title has not been independently established. This
research identifies the arXiv paper above unambiguously and does not attribute the
compiler interpretation to an unseen attachment. BRIDGE.md separates paper facts,
standard relational algebra, and our compiler-specific premises.

## Recent alias models and the boundary they rule out

* Neven Villani, Johannes Hostert, Derek Dreyer, Ralf Jung, *Tree Borrows*, PLDI
  2025, [author PDF](https://research.ralfj.de/papers/2025-pldi-tree-borrows.pdf),
  DOI 10.1145/3735592. §2 associates a reference with both location and a tree tag;
  operations change permission states. §3.1's protectors constrain invalidation
  across calls. Our inference: identity/position alone is an incomplete interface
  for an alias-permission query. This paper defines a dynamic Rust alias model and
  proves particular optimizations; its rules are not a ZIL Desc or static-checker
  specification. Our prototype does not claim to implement Tree Borrows.
* Ralf Jung, Jacques-Henri Jourdan, Robbert Krebbers, Derek Dreyer, *RustBelt*, POPL
  2018, [author PDF](https://people.mpi-sws.org/~dreyer/papers/rustbelt/paper.pdf),
  §5.1 / Figure 4, pp.66:22–24. Its lifetime logic separates use during a borrow from later
  ownership recovery. The relevant lesson is to preserve the temporal permission
  premise independently of a location. It is foundational background, not recent
  work or a proof of this compiler experiment. Footnote 9 on p.66:23 explicitly
  notes that Iris is affine: resources may be abandoned. Its safety result therefore
  does not by itself prove ZIL's mandatory normal/error-exit cleanup obligation.
* Rudy Peterson, *A Program Logic for Tree Borrows*, ETH master's thesis,
  2025-09-19, [institution PDF](https://ethz.ch/content/dam/ethz/special-interest/infk/inst-pls/plf-dam/documents/StudentProjects/MasterTheses/2025-Rudy-Thesis.pdf).
  §§5.4.1–5.4.3 introduce well-formedness/protector-agreement obligations for lateral
  separation. §6.4 explicitly distinguishes its calculus's limitations from the
  PLDI Tree Borrows mechanization. This is a thesis, not a peer-reviewed PLDI
  extension. It suggests a future proof obligation for decomposing permission
  state; it supplies no automatic permission-decomposition theorem for 059.
* Lennard Gäher, Michael Sammler, Ralf Jung, Robbert Krebbers, Derek Dreyer,
  *RefinedRust: A Type System for High-Assurance Verification of Rust Programs*,
  PLDI 2024, [author PDF](https://plv.mpi-sws.org/refinedrust/paper-refinedrust.pdf),
  DOI 10.1145/3656422. Its proof-producing translation/type-system approach is a
  useful reminder to separate frontend translation correctness, IR relation
  equivalence and adequacy for a safety property. 059 proves none of these by
  invoking RefinedRust, and does not produce proof-assistant certificates.

Accordingly, 059 retains Desc root/use constraints but keeps general address and
alias-permission proof outside its admitted fragment. Extending to that domain
would require both safety and preservation of cross-boundary permission effects,
not only exact projection of the current responsibility coordinates.
