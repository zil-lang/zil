# 十方面缺口复核与有限语言组合

共同编译器源码为 `14832d5a35630b7002c79e4df47b0cbb007570b6`（B），最终完整受测提交为 `65c25910616196eae76911afb13818dc79b73012`（C）。B→C只有补充回归、审核和重放工具，编译器源码、Cargo与原生适配输入逐字节相同。精确门、身份、原失败和范围见[验收记录](validation/language-gaps-integration-20261104.json)。

本轮补齐有限 Copy 记录式枚举构造与具名模式、静态 trait 默认方法、一个 Copy 类型参数与 life 参数的组合，以及实际 Fn/FnMut/FnOnce 父闭包中的结构化线程调用。大型 Copy Array/Tuple/Record 的内部按值参数、返回与函数指针改用 copy-in/out 内存 ABI；`.lc1` 递归隔离受影响函数指针载体，普通数据借入和无关小签名保持原 ABI。公共 Typed/Wire、默认预算及产品阈值未变。

独审关闭了四类遗漏：CT 字段简写把生成局部7误读成调用方100；未用枚举变体属性被忽略；原生类型同名的泛型参数误获数值/布尔权限；线程闭包推断新增工作未累计到请求预算。Astra 用来源明确的 release CLI 重放固定43源，修补后的29反例全部在Expand拒绝、14正控全部通过，包含实际callee原生类型与独立life命名空间。原失败留在审核历史，不把未用模板的误接受称作已证实源码UB。

| 检查 | 实际结果 |
| --- | --- |
| 默认完整工作区 | 4166/0，422组，零忽略/过滤 |
| reference完整工作区 | 4057/0，422组，零忽略/过滤 |
| 双配置Clippy、fmt | 通过 |
| LLVM文本、迁移 | 65/0、9/0 |
| handoff工具测试、计划、架构 | 25/0；计划通过；15 crate/38边通过 |
| 跨特性组件、两份原CT源 | Clang/API×O0/O2，4+8次机器运行通过 |
| 既有清理消费者 | 16构建、128运行，其中56次分配失败注入，输出/释放符合原oracle |

最后一轮完整门在C上通过。此前B全量因独立`/tmp`分区满而失败、日志不完整，不能作为完整通过；旧临时目录整体迁入workspace并保留原路径符号链接后重跑，未reset/clean或删除源码。一次Clippy测试模块位置错误及夹具/重放工具设置错误也分别保留。作者、独审和全量的重复执行不累加覆盖数。

[十方面矩阵](LANGUAGE-REMAINING.md)以原52源重新观察：27 Success、13 Unsupported、12 SafeReject，原正控及保护反例无回退。这不是语言完成率。旧334历史收据保持原字节；[当前差异与机器组合](validation/language-gaps-334-final.json)单独绑定B。

Option、usize/u64原生链、const/关联项/泛型impl及方法、一般life/lending、dyn/异构存储、通用状态协议、安装返回后保留捕获环境的外部登记、Suspend/取消/晚通知、共享线程载荷、Mutex/Atomic、完整库/ZUI和原生Windows/Rust同语义成本仍未闭合。另保留2MiB Rust API Verify栈溢出、一般大型Enum/ErrorUnion按值ABI、默认体直接返回全局函数值和既有通用Graph累计收费债务。

旧恢复卡按本轮实际B源码与非作者复核重新绑定C；旧implementation、审核、失败及stale规范registry保存在历史字段，未重写旧对象。当前handoff完整性、历史Unavailable和legacy Fail分账。父B0 Doing、完整语言/ZUI NotSatisfied及产品门NotRun保持。

源码已按阶段备份到GitHub进度/任务分支。PR30的实际状态以[GitHub](https://github.com/zil-lang/zil/pull/30)为准；已观察的两条CI任务因账号支付/额度未启动、没有步骤，不能声称远端CI通过。最终远端头、bundle验证和实际恢复源码比对另记录于备份收据。

2026-11-04接续补记：335修复fresh refs-bundle checkout把旧游离对象可用性变化误作Current错误的问题，Node29/0与独立13组真实Git检查通过；原M的325个Current错误及两种环境legacy Fail保留。当前交付对象/规范/review仍严格，manifest未改。最终新main及实际恢复以备份收据为准。
