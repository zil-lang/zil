# ZIL 文档导航

ZIL 文档按四层清晰分离，语言规则与编译器内部实现不再混写：

1. **语言规范与参考（权威）**：[`_zil_design.md`](../_zil_design.md)（`ZIL-LANG-1.5` 唯一语言语义权威源，含附录 A 形式语义模型）与 [`spec/README.md`](../spec/README.md)（文法、属性、诊断与细则附件分类索引）。
2. **编译器架构与设计决定**：[`architecture/compiler.md`](architecture/compiler.md)（编译器架构、分析/核验算法、增量缓存与 LLVM 后端合同）、[`decisions/README.md`](decisions/README.md)（设计目标、信任边界与长期决策）以及 [`spec/semantic-kernel.md`](../spec/semantic-kernel.md) / [`spec/source-lowering.md`](../spec/source-lowering.md)。
3. **开发者指南**：[语言导览](guide/language-tour.md)（可运行 ZIL 实践教程）、[标准库参考](guide/stdlib-reference.md)（从 `lib/std/` 自动抽取）、[快速开始](guide/quickstart.md)、[支持范围](guide/supported-language.md)、[构建说明](guide/build.md)（Rust / LLVM 23 / Clang / CLI）。
4. **产品验证与符合性**：[`validation/README.md`](../validation/README.md)（产品门、计量原则、证据等级与真实输入）与 [`conformance/`](../conformance/index.json)。

任务排期、工作流与交付协调位于独立 [`goal` 分支](https://github.com/zil-lang/zil/blob/goal/goal.md)。历史施工与研究快照见主线提交 `28b3e37bb4d8a1fe8b146c72cf891fc99de32440`。
