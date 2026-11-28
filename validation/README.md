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

## 性能与演进的唯一计量原则

- [`product-gates.json`](product-gates.json) 是数值预算与场景协议的唯一权威来源。目标规则、输入规模、错误/资源合同、候选与基线版本、工具链、线程数与分配器必须在采样前固定；来源记录仅说明预算出处，不代表已经达标。
- **运行对比**：覆盖构造、访问、关系维护、删除/暂存、转换、析构、任务与取消，分别报告吞吐、P50/P95、峰值与稳态内存、分配与同步开销。Rust 基线采用预先固定的合理最快已知实现；未验证更快实现不得冒称全局最快。
- **编译对比**：分别测量冷 `check`、冷开发 `build`、单点/多点私有编辑、公开合同编辑、布局/CT 输入变化与 O2 发布优化；Rust 构建 ZIL 编译器自身单列。编译期生成器准备、证书核验、缓存哈希与序列化全部计入总耗时；主判据采用配对中位数，不得只取最小单次样本。**同进程形态（冷进程 CLI）与常驻编译会话（`--resident`）必须分开报告，不得跨进程形态混比。**
- **演进对照**：固定树到环、独立删除、指针到索引/SoA、并行化、同步到挂起、helper 到泛型/`dyn` 等完整起点；与语义无关的内部实现调整不应迫使上游业务大面积重写，但涉及真实身份、失败、地址、同步与 ABI 变化的边界必须显式修改。

## 证据等级与独立测试资产边界

- 规范定义、手工论证、模型执行、手工 lowering、生产前端贯通、目标实现与产品验收属于不同证据等级。所有有效证据均须绑定精确源码提交、规则版本、工具链、profile、输入、产物与命令输出；未执行不得记为 Pass，有限穷举仅说明给定域，SC 交错不替代弱内存与真实 driver 证明。
- **独立 Oracle 约束**：独立测试资产（手写语义反例、IR 恶意变异、等价变换、原生执行对照）、业务 oracle 与测试生成器**不得调用被测语义内核、求解器或核验器实现来自证**。可复用公开数据格式，但共享测试侧代码必须单列信任范围。对可信原语的声明必须伴随实现审计与目标义务，不得仅凭标准库模块名授信。
