# 与 Rust 对照的产品证据

每轮交付用短表分别回答：安全边界、运行时间及内存、完整任务的表达与演进、冷/增量编译，以及至少一项经同语义可重复证据支持的突破。明确标注已实现子集、设计义务、受限探针、未测和未达成。量化阈值只引用 `validation/product-gates.json`，不在计划或文档中另立宽松数字。

Rust 对照可使用惯用安全封装、Cell/GhostCell、索引和 Arena；缩小 ZIL 语义、弱化对手或只数语法字符/测试数量不能证明产品优势。报告还需指出结构性退步和下一步最便宜的证伪实验，不因此每轮重跑全部产品基准。完整门与当前语言支持范围分别见 `docs/RUST-GOALS.md`、`docs/LANGUAGE-DRAFT.md`。

历史来源：`.agent/rust-goals.md`、`.agent/design-and-quality.md`。
