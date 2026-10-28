# 205 作者交付：Expand 与五模块消费者

规范为设计 §8.2.3、当前规范摘要 `9813f43e…`，schema/Wire 27。Expand 最终生产 `f62b75ac`；消费者最终源 `82c5cc1c`；实际编译器 CLI03 的干净源码为 `c0f10092`，可执行文件 SHA256 为 `d1033b19c58206ad91ae286fc697685b454e2bbde6860ec66a4cfcba36131ca2`。各阶段原身份不重标，独立审核和正式整仓结果由其执行者单列。

## 已实际闭合的范围

一个完整 State 与一个独立拥有 replacement 使用同一个 F 类型绑定，但 State 两个字段和 replacement 是三个实际位置。原参数次序、完整普通/phantom T、声明模式、take/refill 和完整返回责任保留。源定义检查补齐了实际发现的旧单 State 未用正文借址漏检，包括普通 T 被省略和非 T 保留 Core 两条路径；循环合流保留分支自己的借址版本与原 born 代次，不用清除无效旧借址来通过刷新正例。准备中的 callee 与早期参数窗口跨过后续实参求值。

五模块消费者组合 204 的真实叶 Store 拥有捕获、普通完整 T 的 Store helper、实际元素窗口、pop 后的拥有 State 替换和返回。三个同 F 环境各自拥有叶 Store、资源 Ticket 和可变 counter。输入覆盖反序/非相邻参数、正常替换、同步提前返回、工厂开始前/开始后及反序的 Copy 参数准备失败、FnOnce 消费后补入、零次/一次/continue/break 事件序列。同步取消只表示普通分支返回原 State；没有新增异步取消或 `E!State` helper 权限。

作者 Linux 和 Win64/Wine 分别通过 O0/O2 共 140 次执行：20 次普通、120 次真实分配观察/故障执行，每平台 88 次实际分配失败。全部标记、退出值、分配/释放、峰值请求字节及最终 live0 符合机器运行前已固定的 oracle；Wine 的 bad_operations 为0。实际 LLVM 与观察核对 Item8、Bag32、环境40、State104 字节，最大请求 payload 峰值264字节；这不是进程 RSS。

## 失败与验证身份

- 原未用别名漏检的 Success/Verify1 只验证了 main，坏正文未 native；没有称为已执行 UAF。
- 初始循环合流对合法刷新误拒，原 join01 与独立 stale-copy 对照保留；修改针对分支版本相关性，不降低旧失效检查。
- 原五模块的单 F `pack→identity` 在 Expand 提前拒绝。已有 203 完整值推导只记录双 binder，现补实际单 F producer；无普通 StoreElement 模板的旧 State 路径不新增记录费用，也不造 caller F 或资格证明。
- 首次跨过该入口后，root 的原消费者因 `let bag` 写 counter 被正确拒绝。`82c5cc1c` 只按原意图改为 `var bag`，没有修改可变性规则或运行 oracle。
- Wine01 的首个启动被沙盒本地 IPC bind 限制阻止，零个已验 ZIL 运行观察。Wine02 在允许本地 IPC 的环境使用同源码、同编译器、同前缀实际通过；原失败完整保留。

Expand producer03 的预执行 patch/三份源码快照与最终 f62 Git blob 逐 SHA 相同，实际30/0（既有普通模板17组、205共13组），不把这次预提交运行称为 clean f62。clean f62 的 Expand all-target Clippy 通过。旧真实借址控制预算由原1731修复后迁1938，原基线与新原断言失败保留；此后1938和普通 Store2255保持。header503、unused last-use688、新单 F 完整值组合3877均实测 N/N−1/fresh N；默认额度不变。

精确小收据见 `evidence/impl/205/expand/RESULT.json` 与 `evidence/impl/205/consumer/RESULT.json`。原始源码、阶段 IR、命令、stdout/stderr 和此前失败单独归档；执行产物仅保 SHA，不进入源码或证据载荷。

## Rust 四目标与至少一项突破

| 目标 | 本片实证 | 尚未建立的结论 |
|---|---|---|
| Rust 同类安全 | 真实位置/代次、分支和循环别名、参数准备窗口、完整替换与失败清理有源及机器证据，公开核另由独审验证 | 完整语言安全与所有拥有/借入形状未证明 |
| 运行与内存成本 | 两目标实际内联环境/Store 布局与分配平衡，真实失败后全部 live0 | 没有同语义 Rust 时间、RSS 或尾延迟对照 |
| 表达与维护 | 普通静态函数可接完整 State 和独立同 F 环境，更新/退回/消费后补入并组合普通 Store helper | 一般借入更新、嵌套 callable State 及产品维护成本优势未测 |
| 编译成本 | 新工作有真实预费和限额边界；原不相关控制保持；具名旧安全修复费用如实迁移 | 本片未做编译速度比较，编译支线独立 |
| 至少一项可重复净突破 | 当前拥有事件状态的库级替换组合已实际可执行 | 没有建立产品级四目标合取或 Rust 净胜出 |

完整1.4 `NotSatisfied`、B0 `Doing`、产品与原生 Windows `NotRun`。正式整仓、最终验收和离线 bundle 由集成者使用实际收据完成，不以本作者表代替。
