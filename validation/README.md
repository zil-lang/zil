# 验证资产与留证规范

本目录**仅存放活合同、验证清单与真实测试输入**，不存放逐 PR 或一次性阶段施工的静态 JSON 留证：

- **发布与阶段合同**：[`product-gates.json`](product-gates.json)（终态产品门）、[`language-release-gates.json`](language-release-gates.json)（基础语言 12 域功能门）、[`language-alpha-profile.json`](language-alpha-profile.json)（Alpha 验收配置）、[`language-alpha-resource-profile.json`](language-alpha-resource-profile.json)（预算与资源档位）、[`language-foundation-profile.json`](language-foundation-profile.json)。
- **验证登记与输入溯源**：[`engineering-contracts.json`](engineering-contracts.json)、[`task-test-plans.json`](task-test-plans.json)、[`input-provenance.json`](input-provenance.json) 以及真实 ZIL 输入 [`inputs/`](inputs/)；可执行正反例位于 [`conformance/`](../conformance/) 与 [`tests/`](../tests/)。

## 留证红线

1. **日常 PR 以可执行证据为准**：日常开发 PR 通过精确 Git 提交 SHA、自动化正反例测试与绿灯 CI 证明正确性，**禁止**向主树 `validation/` 追加一次性过程 JSON 快照或手动复制的运行收据。
2. **仅在冻结里程碑生成静态收据**：静态验收收据仅在冻结发布候选（Release Candidate）、正式里程碑签发或可重复性能基线对照时由自动化工具生成一份，并随该提交签发。
3. **历史收据只读归档定位**：
   - 早期验收收据固定于 Git 提交 `28b3e37bb4d8a1fe8b146c72cf891fc99de32440:validation/results/`；
   - 阶段性限定验收 JSON 快照固定于 Git 提交 `08da36ff0384144d795d6ada072a3a51a3eb9a68:validation/`。
4. 当前实时任务进度与协调规则由独立 `goal` 分支管理，永不合入 `main`。
