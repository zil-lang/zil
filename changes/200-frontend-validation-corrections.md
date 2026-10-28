# 200 首次失败与修复记录

所有原始命令、源文件、阶段数据与程序输出都保存在独立 `200-author-evidence.zip` 中。本文件说明原因，不改写原收据；最终 CLI 和消费者绑定 5c2fbc50，不能回填到较早版本。

| 原始运行 | 实际结果 | 原因与后续处理 |
|---|---|---|
| check01 / base 63ab0150，dirty | Expand / Types cargo check Pass | 当时五个生产文件未提交；保留 git_status，不冒称对干净基底的检查。随后生产提交为 73133ced |
| types01 / clean 8bd03433 | 一个测试中的三份原输入符合 Types 前门预期 | 此命令停在 Types，Verify 0；没有当成安全或机器验收 |
| source01 / clean 353407d0 | 8 通过 / 4 失败 | 下列四项原失败全部保留 |
| source01：once-first | Expand Unsupported | 测试在泛型 run 内直接写 unsafe 语句，超出既有符号语句面。改为已支持的普通 `mark()` 调用；没有为打印扩大泛型 unsafe 能力 |
| source01：direct-managed-parameter | Expand Unsupported | 真实缺口：显式 give 具体 Managed 形参仍被只允许 Package 字段 give 的旧门挡住。8247f3bf 在新增双输入构造中记录真实拥有来源，描述符、别名和重复 give 均有未调用定义反例 |
| source01：copy-once | Verify E-VERIFY-MOVE | 真实缺口：结构 Copy 的 Load 是访问视图，不能直接发布进 State。8247f3bf 让 new2 每个无 cleanup Closure 实参先各自 store 到独立临时，再分别 TakeLocal。Verify 发布门和旧单输入路径保留 |
| source01：three-fields | Expand SafeReject | 原输入重复消费泛型 a，实际先触发 generic move。保留其字节，修正预期为这个更早的真实拒绝；不能把该源当成已经触达字段数量门 |
| source02 / clean 8247f3bf | 10 通过 / 2 失败 | 两个生产缺口和 once 测试已通过；剩余项是下列预期差异 |
| source02：noncopy-duplicate | Analyze E-UNINITIALIZED | 第二次 TakeLocal 实际先被未初始化位置拒绝；测试原先只匹配含 mov 的诊断。源字节不变，预期改为 Analyze / not initialized，不改生产 |
| source02：unused-copy-operation | Expand Unsupported | Copy 不授予加法能力；原测试错写 SafeReject。源字节不变，改为实际 Unsupported / declared bound；仍不是 Verify 安全反例 |
| source03 / clean a33fb506 | 12 / 0；58 次 source native | 这版实际通过，不追改为后续 hash |
| 独审预算发现 / 修复前生产 | 原单 `State<F>` 的展开扫描增加 | 新功能不应给旧单 F 类型族增加重复扫描。8bbd05f0 恢复原 `arguments.len()!=2` 的即时返回；混合 T/F 的既有收费保留。独审自行测量旧基底及固定源，作者不冒领其结果 |
| 新增 unused 三字段 single-F 形状控制 | 后续改为旧 Types admission | 未实例化输入形状原本可描述；作者不能借本片关闭旧形状。该控制停在 Types / Verify 0，不宣称真实三字段 State 构造可用；原重复 a 的完整构造反例仍拒绝 |
| consumer-linux01 / a33fb506 | 100 / 0，66 次真实故障 | 保留旧 hash；最终同冻结另跑 linux02 |
| consumer-win6401 / a33fb506 | 首个 count-0 / O0 普通产物退出 −6 | 父环境设了 TMPDIR，Wine 在进入程序前报 `free(): invalid pointer`。同 PE 的 invalid-input 和 probe-zero 也如此；不是 State 构造或清理的安全结果 |
| wine-controls02 / 同一初版 PE | invalid 退出 9；zero 输出 ab；probe-zero allocated=frees=1、live=0、bad_operations=0 | 唯一环境修正为移除 TMPDIR；保留原 PE SHA、命令、完整相关环境与前后输出，没有改程序掩盖失败 |
| combined01 / clean 5c2fbc50 | 43 / 0 | 197 source 14、199 source 9、200 Analyze source 8、200 作者 source 12；200 的真实 native 58 次单列 |
| cli-build02、clippy02 / clean 5c2fbc50 | Pass | 最终 CLI SHA 为 4eaf5291…7a2fc9；原三份 CLI 复验与最终消费者均核同一二进制 |
| consumer-linux02 / clean 5c2fbc50 | 100 / 0，66 次真实故障 | 14 普通执行与 86 探针分列；所有观测 frees=allocated、live=0 |
| consumer-win6402 / clean 5c2fbc50 | 100 / 0，66 次真实故障 | 采用已核的 TMPDIR 缺失环境；所有观测另满足 bad_operations=0。原生 Windows 仍 NotRun |

首轮失败、前门 Unsupported、资格 SafeReject、Types-only 控制、真实 Analyze 拒绝、Verify 拒绝、正常原生产物和分配探针属于不同证据层级。最终通过不把它们合并成一个笼统安全结论，也不改变完整语言与产品门状态。
