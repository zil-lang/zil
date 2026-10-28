# 192 作者交接：Package Copy 字段实际借址

本轮源码冻结为 `e362dcb65dec13fe0af194144936fa6857bc8335`，生产修改为 `48454703c63995e15c9533db1f6808a820417325`；两者只差 root 的恢复派工登记。由 `/root/resume192` 接续 `/root/history178_audit` 的未提交施工，保留原作者贡献、输入、失败和来源身份。独立审核及主线合入仍由 root 完成。

## 已实现与边界

对已经初始化的 named Package 局部，可取得现有有限 Copy 字段的一层共享或独占实际地址。Types、Analyze、独立 Verify 和 LLVM 均根据真实字段核验，复用 BorrowPackageField、物理 Field 根和实际内联 GEP。Copy 字段不产生独立拥有债务，仍依赖父 shell 活动及初始化；实际 Closure 字段沿用原清理和资格。直接 ReadPackageField 也执行当前读权限检查，同字段有活跃独占地址时不能绕过贷款。

拥有 Package 的闭包中，`move` 捕获后的 `&mut state.x` 原本被误判共享捕获。授权修正只把 Package 写地址的 capture 标记交给既有 name(write=true)，仍要求 explicit move 和实际 FnMut。另一个独立 raw 反例发现：伪造 FnMut 为 Fn 时，资源视图校验漏列独占字段借址。Analyze 与 Verify 各自在自己的 Shared 捕获 view 检查补上 mutable BorrowPackageField；普通共享闭包内部新建局部 Package 的独占借址仍允许。

整体 Package 一般借址、任意非 Copy 字段、嵌套 Address 或 slice 不因此开放。内层 Copy record/array 可沿既有投影使用，但不声明同一个顶层字段内新增不相交精度。schema23 与默认预算不变。

## 干净冻结结果

所有下列收据均记录相同 clean HEAD e362dcb65；详情和完整命令位于 SUMMARY.json 的 commands。

| 检查 | 实际结果 |
| --- | --- |
| 8 个 driver 定向/邻接测试组 | 93 通过，0 失败，0 忽略 |
| Verify Package captures 定向回归 | 24 通过，0 失败，0 忽略 |
| 新源面测试 | 18 项；其中实际 O0/O2 原生执行共 28 次 |
| 新 raw/Wire/预算测试 | 5 项；9 对非法输入分别拒绝，共 18 次；1 对有效输入通过；另有 Analyze Shared 资格拒绝 |
| 四模块 State 消费者 | O0/O2 共 50 次 Linux 原生执行，其中 32 次实际分配失败 |
| 编译、格式与 Clippy | build、fmt、4 个生产 crate 和 192 测试 clippy -D warnings 均通过 |

消费者直接同时借 State 的 callback 字段与 Copy sibling，消除旧的读出局部、借给回调、再写回流程。两个 ByteBuffer 拥有者及 Ticket Drop 均参与实际执行；每个受监测路径都核对分配调用次数、失败次数、分配数等于释放数及 live0，不能只看退出码。

在实际 Copy 字段输入上，Analyze 工作额度 4542 拒绝、4543 通过，Verify 8870 拒绝、8871 通过；拒绝类别均为 ResourceLimit，默认 10000000 未改变，新的默认额度重试通过。Windows/Wine 未由 192 作者运行；189 先前的平台结果不冒记为本片验证。

## 原始失败与修正保留

- development02 为 9 通过、5 失败：包含语句终止符 fixture、正例中实际冲突的当前回调实参、循环数值预期，以及已被更早 Analyze 门拒绝的父对象覆盖。保留原输入及诊断，未改为成功日志。
- development03 为 16 通过、1 失败：实际 owned Package 捕获借址误拒。修正后相同源程序通过，并补非 move、不可变闭包、同字段直接读取等控制。
- development05 源 17 项通过，raw 3 通过、2 失败：一次伪造 Shared Fn 被独立 Verify 错误接受；一次父清理更早被 E-VERIFY-INITIALIZATION 正确拒绝而测试预期错误。第一次返回的 VerifiedModule 及原始模块、候选和 Wire 输入保留。d69b2bfe 是明确 WIP，不作为通过的源码冻结。
- development06 为源 18 项及 raw 5 项全过，但它仍是开发中的 dirty 运行；正式结论只使用随后冻结执行。
- 原 handoff 和恢复 handoff 的历史登记失败原样保留，不算语言通过。

## 证据与范围

FILES.json 记录保留的原始文件，OMITTED-FILES.json 记录每个排除项的路径、长度、SHA256 和原因。保留命令、输入、诊断、192 原始 TypedIR/Wire 数据和必要的完整阶段链；排除可执行文件、链接产物及大量重复阶段文本。完整原运行目录未删除。sources/ 保存受测 fixture、消费者与生产差异，SOURCE.json 保存源码和规范哈希。

安全证据是实际来源与别名反例及独立核验；成本证据是已有内联字段指针及分配清理观测；表达能力证据是直接字段地址可组成真实 State 回调；编译工作证据是既有有界预算。未做同语义 Rust 运行、内存或编译速度比较，不宣称全面优势。完整工作区检查、独立复核、合流和最终打包由 root 负责；完整语言1.4仍为 NotSatisfied，产品为 NotRun。
