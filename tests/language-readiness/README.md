# 语言完整性来源样本

基线与实际观察见 `docs/validation/language-readiness-baseline.json`，核查日期2026-11-03。
`controls` 已通过当前源码到Verify；`pending` 保存待完善入口，不注册为“必须拒绝”测试。
未来修复须保留原意并晋升为实际正控。没有从未支持诊断推断安全，也没有将此检查冒充机器运行。

每份源可用 `zil check <path> --json` 重跑；判定以当前规范及其具名附件为准。
这些源覆盖典型缺口，不构成完整语言测试集；接口设计仍未固定的浮点/匿名union没有加入必修清单。
