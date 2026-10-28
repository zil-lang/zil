# 171：有限资源真实共享头地址交付

实现冻结 `2d5214b0a65a688f86736abdcf54625b5427ae9b`，crates tree `61fef030177ba55424d3619eb2b32ccfafe6b31f`。吸收 170 与公共 Wire 修复 `bf2280d8ef53ed234f0605e10a55d7741b914b3e`；schema/wire 20，规范摘要 `c358b8c5e0abaf11d81b1d55e23f6896cfb485eb4291f34937fe43e9e7ea319c`。本记录为作者限定结果，尚待独立审核与集成；作者未跑正式全量、未合入、未修改默认预算或产品条件。

## 实际能力

有限 Copy payload 的用户 resource 可从实际 Managed 局部建立共享 `&R`，用于普通同步 helper、inherent `self:&Self` 方法和本模块静态 trait 的共享 receiver。调用可跨模块。原 159 `&Self` 声明探针源码保持原样，现为 Success；测试又实际调用 trait 与 inherent 方法，并通过 LLVM O0/O2 执行。

该值为 `Address<shared, Managed<Record<R>>>`，LLVM 指针引用既有内联局部，字段投影使用真实 payload 布局。它不是 Desc，不增加独立资源责任或隐式堆盒；`ReadAddress` 整头、`WriteAddress` 整头、伪造可变头与同布局异名资源均被拒绝。头窗口活跃时，同源 Desc 写入、give、清理或重新初始化旧槽后的旧指针使用不能通过；最后使用后，既有 payload 写与 give 可继续。

Analyze 构造真实候选来源；Verify 从 CFG、资源名义定义、来源与 loan 独立重建。Desc-only 被调用者仍导出读写效果，跨模块和多层普通 helper 不能因其自身没有头地址而遗漏写效果。借入 Desc 回调按潜在写入处理，纯 Desc 的异源回调可在共享 helper 内连续调用，也可多次调用该 helper。无关自含 owning 闭包保持可组合。

## 最近反例推动的边界

自审曾实证两个误接受，原件收录在 `evidence/impl/171/retained-counterexamples/`：

1. 调用方持有共享头，Desc-only helper 在内部构造并调用写闭包。原效果推导只覆盖直接 Desc 写和已知 closure 参数，遗漏内部回调；现沿实际 callback 根导出并传播效果。
2. 回调同时捕获 Desc 和从共享头投影出的地址，最后一次调用经多层环境 reborrow 隐藏相交窗口。当前首片没有逐捕获读写合同，因此明确关闭这类混合组合。

后者的准确边界是：**存在实际共享资源头的整合模块内，调用同时捕获 Desc 与 Address 的闭包，返回 Verify / Unsupported / E-UNSUPPORTED**，消息为 `mixed descriptor/address callback captures with resource-head windows require a per-capture effect contract`。这是一项保守边界；不宣称一般混合捕获 reborrow 已经支持。无共享资源头的原有普通 Copy 地址加 Desc 闭包已另有 native 对照通过。

首片还保持以下 Unsupported：ByteBuffer 头、可变资源头、Package 字段头、Desc 转头、头返回/捕获、从头参数函数返回投影地址、在捕获的 Managed 内建立头、头函数指针/FFI。155 会把实际整局部重赋值的 Managed 降为私有 Package 字段槽，该类绑定的头也保持关闭。公开 trait 声明、资源与实现使用同一拥有模块；普通调用可跨模块，不宣称新增导入 trait 的通用 impl 求解。

## 可复核结果

| 层次 | 实际结果 | 证据 |
|---|---|---|
| 限定源码/原生组 | 17 passed，0 failed，0 ignored | `evidence/impl/171/limited-09/command.json` |
| 源编译观察 | 25 Success；27 个近邻拒绝，含 2 个 Verify/Unsupported 混合捕获边界 | `evidence/impl/171/focused-observations.json` |
| 测试组原生执行 | 24 次：O0 与 O2 各 12 次，含析构顺序与失败实参准备 | 同上及 `focused-cases/*/native.json` |
| 原始与序列化输入 | 9 个篡改，各经 raw 与 wire 独立拒绝，共 18 次 | `focused-cases/*/*-raw.txt`、`*-wire.txt` |
| Verify 工作预算 | N=5337 Pass；N−1=5336 ResourceLimit；默认额度未变 | `focused-cases/*/head-budget.txt` |
| CLI 交付示例 | 多模块成功及失败实参准备，各 O0/O2：4 次 Pass | `evidence/impl/171/consumer/summary.json` |

原生观察数是执行次数，CLI 复用了交付示例，不是额外独立安全证明。阶段表示保留在 `evidence/impl/171/stages/`：接口、TypedIR、LLVM；证据仅约 1.1 MB，不含可执行文件、object、Cargo target 或批量阶段输出。各次正式失败、早期探索、缓存异常均保留并在 `result.json` 分别归类；误接受不被后续 Pass 覆盖。

交付源码为 `examples/mainline/impl171/{main,model,user,prepare_failure}.zil`。使用当前 CLI 可直接构建 `main.zil` 或 `prepare_failure.zil`；前者输出 `A` 并退出 0，后者输出 `A` 并退出 1。`tools/language/171/consumer.py --repo <checkout> --cli <zil> --out <new-dir>` 运行两种优化并记录实际命令和输入身份。限定测试入口为 `cargo test --offline -p zil-driver --test impl171_resource_heads -j2`。

## Rust 四目标与产品状态

| 目标 | 本片实证与限制 |
|---|---|
| 同类安全 | 真实 head/Desc 相交、名义身份、责任和调用效果有源码/raw/wire 反例；尚无全语言、逃逸、并发或一般混合捕获证明。 |
| 运行与内存成本 | 真实栈内头地址、字段布局、单次析构与失败清理已运行；未做与 Rust 的同合同时间、峰值内存或尾延迟测量。 |
| 表达与维护 | 资源可用普通 `&R` 与共享 `&Self` 建立同步接口，多模块复用实际成立；维护工作量优势未测。 |
| 编译成本 | 新增扫描按实际有界工作收费，N/N−1 可复核；本片未做编译性能比较，不从测试耗时推导加速。 |
| 至少一项净突破 | 从原声明探针推进到真实地址、调用与机器执行是工程能力增量；尚无产品级 Rust 净胜出结论。 |

完整语言仍为 NotSatisfied，产品 NotRun，父任务 B0 的终态由集成方按既有门核定。
