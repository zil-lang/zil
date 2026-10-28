# 202 Expand 作者交付：完整 StoreElement 模板与类型应用

## 冻结身份与实际范围

- 公共骨架：`9f60fb1fb1c22ecb2fdaf81d9d287fbeca54e11e`。
- 最终生产：`0f1c761dca20687ede4fe969db9277ec762caa25`；最终测试/实际 Core 输出：`806c3d9db0a5bd89c490044ec9679a6286c51f2e`。后二者的四个 Expand 生产文件字节相同。
- 设计 SHA256：`6dcf6c58d126bc7922cee8102bb4b44ca91c9acd6e1ef476fb3bcfc80f2399a6`，已核实际 `_zil_design.md`；规范摘要：`9bb84f09971a522cb8ffc3808829d3aa05744dac1e8871c3914e7a2a0635aec6`，schema/Wire 26。
- 生产写点仅 `generics.rs`、新 `generic_store_templates.rs`、`receivers.rs`、`lib.rs`。没有修改 Types/Analyze/Verify、公共规范、默认额度、generic_check、generic_enum、modules 或旧 Store 责任检查。

这份收据只证明 Expand 工作。原 helpers01 两源原本已能通过旧 Expand、随后在 Types 失败；本次必须保留的是正确 Core 模板和完整类型约束，不能把新的 Expand 成功说成整链路缺口已经关闭。本作者尚未以这些提交执行新的 Types/Analyze/Verify/Wire/native。

## 实际变化

显式 `::<T>`、值实参推导和明确结果上下文共同到达实例化入口。在原有 `original_type` 已计费遍历内观察当前词法 callable/普通 T 的依赖；未增加另一遍全树扫描。只有 exact 单 StoreElement 模板且实际 T 仍依赖 caller scope 时保留原模板；完全具体调用保持旧 eager 分支。

保留函数发布独立 `CoreFunction.type_parameters`；全部延后调用输出静态 `Call(TypeApply(callee,[完整元素 T]), values)`。模板体内仍是自己的 T，调用者 F 不被代入模板声明。child ordinary T 保留 caller scope，返回提示采用同时替换并继续使用 caller 的名义 family，不经过会切换到普通 callee scope 的旧 `signature_result`。

新增保存名单采用现有上限下的有限 Vec，对每次实际名字比较、复制和模板入队先计费。保留模板与 eager 实例共用原 `max_functions.min(512)` 上限。既有定义时检查、封闭 StoreElement 能力、模块可见性及原字段/责任门继续生效；不创建 callable 角色、空合同或安全证明。

## 已执行结果

| 精确运行 | 实际结果 | 说明 |
|---|---:|---|
| `source03` / `ac89fd72` | 6/0 | 第一可联调提交，包含两原源、显式/child T、unused/contextless 及旧 197 预算 |
| `source06` / `1df616ed` | 13/0 | 增加完整名义、跨模块、create 转发、约束冲突与新预算 |
| `crate02` / 最终生产 `0f1c761d` | **44/0** | `cargo test -p zil-expand --locked --offline`，包含新 13 项，未跑全仓 |
| `source07` / `806c3d9d` | **13/0** | 最终测试提交；写出实际源码、Core、source-map，并现场核两个原 SHA |
| `legacy-before01` / `9bc570bc` | 2/0 | 仅在 `9f60fb1f` 生产上添加与 `1df616ed` 逐字相同的测试文件，运行两个旧预算控制 |

两原输入完整保留：push 源 1655 字节，SHA256 `101580f8d78e661011d3b73f731c4f2f2ae376a408c1cc02c8aa072d4f627bf9`；empty/reserve 源 1657 字节，SHA256 `d8fd10675872542e99533ee23957db2c3150760a2f71ea8ff28150ad6c3a7a55`。实际 Core 中 push 保留一份原模板且两次调用均携带 caller Store 的同一完整元素名义；empty/reserve 各自保留模板，create 模式没有丢失。

新的 13 项同时区分：源显式与推导、同名/改名 child T 与 create 返回、同布局但不同 phantom 普通参数、定义模块/exports 与私有模板拒绝、显式类型与真实值/结果冲突、未使用的非法定义、缺失结果上下文、混合泛型头关闭。它们是源到 Core 的检查，不替代 Types 对完整实例的独立约束检查。

| Expand 逻辑预算控制 | 旧生产 N | 最终生产 N |
|---|---:|---:|
| 原 197 单字段 State | 1731 | 1731 |
| 完全具体普通 Store helper | 2255 | 2255 |
| 新 helpers01 empty/reserve 延后路径 | 未作为旧已支持语义衡量 | 8399 |

所有列出的边界均执行 N 成功、N−1 ResourceLimit、全新 N 再成功；默认额度不变。旧普通 Store 的精确 N 来自实际旧基线运行，不是根据代码差异推定。

首轮 `source01` 为 5/1：未调用的 opaque 字段访问正确返回既有 Unsupported，作者测试错误地只期待 SafeReject。保留了真实失败输出、当时五文件源快照和 diff，随后只修测试的分类预期，没有放宽生产。`source02`、`source04`、`source05` 是 dirty 探索运行，其原 `source_commit`/`git_status` 全部保留，不改写成最终提交成绩。

## 证据与后续

Git 内小索引为 `evidence/impl/202/expand/SUMMARY.json`。外置 `202-expand-evidence.zip` 为 **177159 字节**，SHA256 `485cea7ca262f67815b5429ebbef0a0c22ed6caba6209a2abd18ea2b7758da8c`，69 个材料文件另加 manifest，已检 ZIP CRC 和每个成员 SHA256。内容只有实际命令、stdout/stderr、源码/差异、Core/source-map 和索引，没有编译执行产物。大证据不加入 Git。

下一步由完整 Types/CT/driver 候选和非作者检查完成纵向证明；另按派工准备新的多模块 Store-template 消费者与故障矩阵，不修改 201 消费者，也不复用其运行成绩。202 与整个 1.4 的最终完成状态仍由 root 冻结验收。

Rust 四目标：完整 T/名义身份与失败载荷保持对应正确性和表达能力；旧精确预算实测相同对应可预测编译成本；Core 仍下沉为候选普通静态 Call，不引入新的运行时机制；源映射和独立元数据核保留可审计性。实际突破是把普通 StoreElement 函数需要的完整 callback State 类型约束送到 Core；本收据没有提前声称后续类型实例化或机器消费已经完成。
