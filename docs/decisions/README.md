# 长期设计决定

正式语言规则在 [`_zil_design.md`](../../_zil_design.md)（`ZIL-LANG-1.5`），编译器内部架构合同在 [`docs/architecture/compiler.md`](../architecture/compiler.md)。本目录记录非规范性的长期设计动机与取舍：

- [设计目标与基线定位](design-goals.md)：四项核心目标、明确非目标与独立语言必要性门。
- [编译器边界](compiler-boundaries.md)：信任边界与阶段职责取舍。
- [编译效率](compiler-efficiency.md)：优化准入与成本模型决定。

任务排期与验收协调位于独立 `goal` 分支。
