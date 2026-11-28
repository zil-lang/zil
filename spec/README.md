# ZIL 规范附件权威分类索引（`spec/`）

> 规范版本：`ZIL-LANG-1.5` · 语言语义总权威：[`_zil_design.md`](../_zil_design.md)

本目录承载与 [`_zil_design.md`](../_zil_design.md) 配套的机器可读附件、语言规范细则与编译器阶段合同。所有文件按**权威层次**明确分为四类，避免语言规则与编译器内部实现合同混同。

---

## 权威层次总则

1. **语言语义权威（Language Specification）**：[`_zil_design.md`](../_zil_design.md) 是 ZIL 语言语义、类型、所有权、借用、协议与形式语义的唯一文字权威源；本目录第一类文件为其规范性机器可读附件，第三类文件为其具名语言细则附件。若任一细则附件与 `_zil_design.md` 正文冲突，以 `_zil_design.md` 为准并同步修订附件。
2. **编译器架构与阶段合同（Compiler Contracts）**：[`docs/architecture/compiler.md`](../docs/architecture/compiler.md)（`ZIL-ARCH` / `ZIL-LOWERING`）、本目录第二类文件与 [`policy/dependencies.json`](../policy/dependencies.json) 定义编译器内部数据契约、中段语义核、降低契约与阶段可信边界，不单独创造新的源语言特性。
3. **产品验收与符合性（Validation & Conformance）**：[`validation/product-gates.json`](../validation/product-gates.json) 与 [`conformance/`](../conformance/index.json) 管理量化验收门槛与源码符合性用例，不替代语言规范正文。
4. **说明性与指南文档（Decisions & Guides）**：[`docs/decisions/`](../docs/decisions/README.md) 与 [`docs/guide/`](../docs/guide/quickstart.md) 为非规范性说明与使用文档，不具有覆盖规范的效力。

[`authority-manifest.json`](authority-manifest.json) 是机器可读的唯一权威文件清单（区分 `language_spec`、`compiler_contracts`、`validation_contracts` 三类）；`python3 tools/update-indexes.py` 从该清单确定性生成 [`data/normative-inputs.json`](../data/normative-inputs.json)（含分类哈希与总 `normative_hash`），并强制校验其与 [`interfaces.json`](interfaces.json) 的 `normative_inputs` 及本页索引严格一致。

---

## 一、语言规范机器可读附件（Language Normative Attachments）

归属 `language_spec` 并纳入 `normative_hash`，直接定义源语言文法、属性注册表、权威清单与稳定诊断码。

| 文件 | 权威范围 |
| --- | --- |
| [`authority-manifest.json`](authority-manifest.json) | 唯一机器可读权威文件清单（语言规范 / 编译器合同 / 验证合同三类划分） |
| [`grammar.ebnf`](grammar.ebnf) | `ZIL-LANG-1.5` 规范性源语言文法产生式与词法终结符索引 |
| [`attributes.json`](attributes.json) | 内建 `#[...]` 属性注册表、合法目标、消费阶段与重复规则 |
| [`diagnostics.json`](diagnostics.json) | 稳定诊断码注册表与阶段/规范规则 ID 映射 |

---

## 二、编译器阶段与中段架构合同（Compiler Stage Contracts）

归属 `compiler_contracts` 并全部纳入 `normative_hash`，约束编译器内部架构，不改变源语言语义。

| 文件 | 合同范围 |
| --- | --- |
| [`interfaces.json`](interfaces.json) | 编译器阶段间数据接口与可信构造边界注册表 |
| [`semantic-kernel.md`](semantic-kernel.md) | `zil-semantics` 唯一规则内核与 `zil-check` 归纳证书核验架构合同 |
| [`source-lowering.md`](source-lowering.md) | 源 AST 到 Body IR（`zil-typeck`）降低与候选 `SourceMap` 合同 |
| [`analysis-inventory-sharing.md`](analysis-inventory-sharing.md) | Analyze / Verify 请求内不可变快照共享与独立核验边界 |

---

## 三、主规范具名引用的语言细则附件（Referenced Language Sub-Specs）

归属 `language_spec` 并全部纳入 `normative_hash`，由 [`_zil_design.md`](../_zil_design.md) 正文显式引用，补充特定语言特性在细粒度边界下的行为与拒绝条件：

| 文件 | 所属主规范规则 | 主题 |
| --- | --- | --- |
| [`ordinary-values-and-cleanup.md`](ordinary-values-and-cleanup.md) | `ZIL-PACKAGE` / `ZIL-CLEANUP` | 普通拥有值组合与同步迭代清理细则 |
| [`ordinary-store-ownership.md`](ordinary-store-ownership.md) | `ZIL-PACKAGE` | `OwnedStore<T>` 元素拥有与前缀不变量 |
| [`store-growth.md`](store-growth.md) | `ZIL-PACKAGE` / `ZIL-ACCESS` | `OwnedStore<T>` 按需增长追加边界 |
| [`pod-store-elements.md`](pod-store-elements.md) | `ZIL-PACKAGE` | POD `StoreElement` 结构元素规则 |
| [`error-union-construction.md`](error-union-construction.md) | `ZIL-SYNTAX` / `ZIL-PACKAGE` | `E!T` 显式构造（`error_ok` / `error_err`）与模式匹配 |
| [`closure-receiver-borrows.md`](closure-receiver-borrows.md) | `ZIL-CAPTURE` | 闭包接收者与混合借用捕获 |
| [`returned-capture-provenance.md`](returned-capture-provenance.md) | `ZIL-CAPTURE` | 返回闭包环境的来源与拥有动态清理 |
| [`shared-dyn-dispatch.md`](shared-dyn-dispatch.md) | `ZIL-CAPTURE` / `ZIL-TRAIT` | 借入动态派发（`&dyn Trait` / `&mut dyn Trait`） |
| [`dyn-implementation-witnesses.md`](dyn-implementation-witnesses.md) | `ZIL-CAPTURE` / `ZIL-TRAIT` | 动态派发声明与实现见证保真 |
| [`owned-dyn-storage.md`](owned-dyn-storage.md) | `ZIL-CAPTURE` / `ZIL-PACKAGE` | 显式拥有动态存储（`OwnedDyn<dyn Trait>`） |
| [`affine-mutex.md`](affine-mutex.md) | `ZIL-CONCURRENCY` | `Mutex<T>` 所有权与仿射 `MutexGuard` 借用 |
| [`task-owned-aggregates.md`](task-owned-aggregates.md) | `ZIL-ASYNC` | `Task<T>` 拥有聚合、取消与关闭协议 |
| [`threaded-foreign-registration.md`](threaded-foreign-registration.md) | `ZIL-ASYNC` / `ZIL-CONCURRENCY` | 跨线程外部回调登记（`AsyncRegistration<F>`）状态机 |
| [`reserved-module-trait-names.md`](reserved-module-trait-names.md) | `ZIL-TRAIT` | 模块重写后的保留 trait 排他名称检查 |
| [`trait-method-lending.md`](trait-method-lending.md) | `ZIL-TRAIT` | Trait 方法逐调用 lending 与高阶生命周期 |
| [`mutable-stored-holder.md`](mutable-stored-holder.md) | `ZIL-TRAIT` / `ZIL-ACCESS` | 可变存储持有者与 `StorageLifeContract` |
| [`semantic-scopes.md`](semantic-scopes.md) | `ZIL-TRAIT` / `ZIL-ORIGIN` | 原始语义作用域与完整程序借用/生命周期组合 |

---

## 四、特性切片补充细则（Supplementary Feature-Profile Specs）

以下文件记录特定语言切片与标准库容器的精细组合剖面（均从属于 `_zil_design.md` 及对应 `ZIL-*` 规则，不得与主规范冲突）：

- **借用、投影与存储窗口**：[`borrowed-byte-field-views.md`](borrowed-byte-field-views.md)、[`borrowed-byte-metadata-queries.md`](borrowed-byte-metadata-queries.md)、[`borrowed-product-head-captures.md`](borrowed-product-head-captures.md)、[`dynamic-shared-address-array.md`](dynamic-shared-address-array.md)、[`fixed-shared-address-array.md`](fixed-shared-address-array.md)、[`qualified-package-shared-array.md`](qualified-package-shared-array.md)、[`selective-product-paths.md`](selective-product-paths.md)、[`storage-source-atoms.md`](storage-source-atoms.md)、[`store-element-field-borrow.md`](store-element-field-borrow.md)、[`stored-owned-observations.md`](stored-owned-observations.md)
- **回调、高阶组合与动态合同**：[`borrowed-input-owned-callbacks.md`](borrowed-input-owned-callbacks.md)、[`callable-return-lifetimes.md`](callable-return-lifetimes.md)、[`callable-source-scopes.md`](callable-source-scopes.md)、[`dyn-context-provenance.md`](dyn-context-provenance.md)、[`dyn-retained-contracts.md`](dyn-retained-contracts.md)、[`ordinary-collected-pipeline.md`](ordinary-collected-pipeline.md)、[`qualified-owning-environments.md`](qualified-owning-environments.md)、[`stored-owned-callback-pipeline.md`](stored-owned-callback-pipeline.md)、[`trait-owned-results.md`](trait-owned-results.md)
- **编译期与标准库专项剖面**：[`checked-resource-graph.md`](checked-resource-graph.md)、[`ct-dynamic-catalogue.md`](ct-dynamic-catalogue.md)、[`numeric-text-and-buffered-io.md`](numeric-text-and-buffered-io.md)、[`owning-hash-collections.md`](owning-hash-collections.md)、[`pod-hash-collections.md`](pod-hash-collections.md)
