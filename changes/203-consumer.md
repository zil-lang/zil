# 203 五模块双声明 State 消费者：作者实际结果

## 冻结身份与结果

最终作者消费者使用 clean `7afbb6493cde414b042dcd0dfc67fc7d2579d63d`。CLI06 来自 clean `ee0ebb3f78d942a58c8cf3576654e937c5e35629`，二进制 SHA256 `6044e4b5e5551302f3452b50759bda54cde8f65cab7c4d8237c4e4243481a187`；runner 实际核对 clean build receipt、该二进制及完整生产输入字节一致。消费者提交额外包含已保存的 CLI05 阶段小收据，生产与 CLI06 一致。

| 目标 | 普通执行 | 独立分配 probe 执行 | 其中真实分配失败 | 合计 |
| --- | ---: | ---: | ---: | ---: |
| Linux x64 native | 16 | 112 | 88 | 128 |
| Win64 / Wine | 16 | 112 | 88 | 128 |

每个平台分别编译 O0/O2，四次五模块 ZIL 构建各实际运行一次 Verify。probe 沿实际生成的 LLVM 调用独立 allocator shim，不替代 ZIL 前端或安全核；普通机器执行与 probe 分列。所有输出和退出码符合运行前固定的独立 oracle，所有 probe 最终 `live=0` 且分配/释放平衡，Win64 同时 `bad_operations=0`。最大峰值存活**申请字节**为 301，只是这些 probe 追踪的 payload 请求量，不是进程 RSS、栈或 allocator 总内存。

原生 Windows 为 **NotRun**。本记录是作者消费者结果；203 正式非作者审核、整仓集成和父任务状态由各自收据决定。

## 实际语言组合

`main`、`factory`、`state`、`store_helpers`、`portable` 是五个真实模块。两份 `State<Tag,F,G>` 的 F 是 Ticket + ByteBuffer 的实际 FnMut 环境，G 来自另一个实际 Fn 工厂，但保存后的声明只提供 FnOnce。它们沿普通 T 的 identity/seed/push/reserve/pop/len/capacity 函数传递完整类型与实际责任。

循环通过 Store 元素的真实字段位置按 ACCA 顺序调用 F，覆盖零轮、continue、break、扩容与多个实际环境。只有 pop 后的完整拥有 State 才进入消费 G 的路径。`P`、G 的 `D`、G 立即清理的 `d`、`Q` 区分弱声明 FnOnce 下的实际消费时点；随后仍可调用剩余 F。相同 F 类型的两元素各有独立 ByteBuffer，不把类型身份当成实际环境值身份。

三个代表性实际输出为：输入 0 得 `PDdQC2c1ab`，输入 1 得 `ACCAPDdQC2c1ab`，容量满 push 控制 6 得 `2cd1ab`。所有真实申请失败点和最后申请之后的成功控制均执行，原 oracle 未根据观察回填。invalid input 与 EOF 是另行普通入口控制，不冒充 allocator 观测。

## 原失败与重放

早期 `expand01` 的 seed 源遗漏 `give`、`expand02` 暴露的完整 T 值推导缺口、跨模块 active 字段遗漏 public，均按各自实际身份保存。对应修复没有把原成功目标改成 Unsupported，也没有放宽隐私或责任规则。

CLI05 的 `5e101737` 曾完整通过同一两平台 128/88；随后未用定义的 While 条件别名近邻需要补充校验，因此该成绩保留为阶段结果，并用 CLI06 从新目录重新执行完整矩阵。CLI05 与 CLI06 的五源快照和 oracle 数据已逐项核对相同；不把前一版成绩归给后一版。

首次 Linux01 只因短暂同步临时目录被 clean-source 门拒绝，ZIL 编译和机器执行均为 0。目录自行消失，没有删文件、改 ignore 或放松检查；该原始身份门失败随证据保存。CLI06 的 Linux03 / Wine02 均实际完整通过。

## 交付与边界

小收据：`evidence/impl/203/consumer/RESULT.json`。最终文本证据：`203-consumer-cli06-evidence.zip`，1,468,620 字节，SHA256 `9dabbe988cf4bd9e9a50376a8840be005af9cf41b2ff135408bdf370e1c59b1d`，877 个 payload 加 MANIFEST，CRC 与逐成员摘要均核过。源码、阶段 IR、实际命令、输出、独立 oracle、编译器 build receipt 和受控 Wine 环境都保留；exe/object 等执行产物排除。CLI05 的原包独立保留，不覆盖。

本例证明有限 State、两个声明模式、普通完整 T、Store 窗口与错误/消费清理的组合。直接 Store 捕获、含 Store Package 捕获、一般递归拥有、异构 dyn、长期外部回调、挂起/取消/并发和完整 ZUI 仍各有前置义务。后续只读审计和原样探针见 `changes/203-next-store-capture-audit.md`，不由这次消费者通过自动获得新准入。

## Rust 四目标

| 目标 | 本片实证 | 尚不能推得 |
| --- | --- | --- |
| Rust 同类安全 | 真实来源/模式/窗口、退出清理与故障矩阵形成有限闭环，作者与独审分开 | 完整语言安全或所有组合都已证明 |
| 运行与内存同级 | 有明确内联环境与实际分配/释放、峰值申请量记录，无新增隐式 Box/RC/锁 | 同语义 Rust 的运行时间、RSS、尾延迟或内存成本胜出 |
| 表达与维护 | 普通跨模块 T helper 组合两种 F/G 行为和完整拥有 State，无调用点私有内联替代 | 完整框架维护成本优势 |
| 编译成本 | 原收费控制与新别名预费分别有精确来源，默认额度未增加 | 编译速度改善或全部旧输入零成本；编译专项仍独立 |

语言组合能力确有推进；四目标合取与产品级可重复净突破仍未建立。完整 1.4 **NotSatisfied**，产品 **NotRun**，既有 product-gates 未降低。
