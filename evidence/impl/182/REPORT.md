# 182 混合捕获：作者限定交付

冻结源码为 `22a91edab6f0cf2017d6258874db3b19ae940c85`。本片在设计 1.4 / schema 22 下完成有限资源 Desc 与 Copy 字段 Address 的逐捕获核验；已吸收 179、180、181 及独立复核的 178 callable helper 回归修复。作者限定验收通过，独立审核与主树合入由集成者负责；不代表完整语言或产品验收完成。

## 结果与实际消费者

同一具体环境可以持有资源 Desc、同源 Copy 字段共享 Address 和独立资源的另一个 Desc。实际只读访问可并存，实际写仍必须避开相交共享/独占窗口。模块中加入不相干的资源头不再改变该组合的准入。

四模块 `examples/mainline/mixed_capture182` 从运行时输入读取 0–3 轮。callback 同时读取 anchor 的 Desc 与 Copy 字段共享地址，调用内部 callback 更新另一个资源；每轮通过公开同步 helper 执行。最后调用结束后再更新原共享字段所属资源。输入 `3` 实际输出 `123BA` 并返回 0；数字是实际更新输出，B/A 是真实资源 Drop。输入 0/1/3/9 各在 O0/O2 下执行，非法 9 在创建资源前返回 9。

## 实现与安全边界

Types 已生成真实按序捕获，公开 schema、Wire、`CallableParameterContract.writes`、Lower 与 LLVM 均保持原义。Analyze 对受支持资源描述发布既有保守 Hold 投影，不再依赖全模块是否出现 head；Verify 从实际 CFG 独立重建每个捕获下标的来源及实际效果，不消费 Analyze 的求证结果。

私有捕获表按 SSA/局部位置保存 capture 下标到来源集合。MakeClosure 取实际输入来源，加载、转交、环境借用及再借用保持下标，CFG 合流逐下标并合。普通 helper 参数和 callback 捕获都在调用点替换成实际来源。Desc 的读写及原存储操作效果参加预算化单调固定点；循环、递归和无正常返回路径中的可达前缀不会被当作空效果。

公开 helper 仍以独立核验的 traces 确定是否调用环境，并使用具体 callback entry 的实际效果；其 `writes` 仍是能力上界，没有改成精确 API 承诺。该检查不读取 helper 的私有效果行，资源责任/清理链也未被替换。

当前调用的活动集合包含后续活性、本次 callee 的捕获及所有显式实参。环境位置借用与外部捕获贷款分开；再借环境不能把捕获共享地址当作 Desc 写授权。`ReborrowAddress` 对 Address<Closure> 只按真实环境位置根进行访问检查，不能把只读 Desc 当成环境写。helper 参数按每个 Address 捕获的实际 shared/mut 类型建立贷款，私有 ParameterCapture(p,c) 与环境位置 Parameter(p) 分开。不同符号捕获下标不构成实际不交证明，调用方必须替换来源核验。

正常、错误与 FnOnce 退出继续使用原责任清理；零调用不提前执行 callback 效果，未调用环境的 Cleanup 不把未使用 Desc 变成强保活。未来实际调用仍不能读取已给出或清理的来源。

## 冻结验收

`runs/frozen03/summary.json` 绑定冻结源码、实际工具链和完整限定命令；`observations.json` 只统计新 182 用例的机器与反例观察，不重复累计早期重跑。

| 项目 | 本轮实际结果 |
|---|---|
| 182 新用例组 | 21 通过，0 失败，0 忽略 |
| 171/179/180、shared/borrowed addresses、065 helpers、161 nested 邻接组 | 101 通过，0 失败，0 忽略 |
| 限定总计 | 122 通过，0 失败，0 忽略 |
| 182 原生构建与执行 | 54 次：O0/O2 各 27 次 |
| 182 源码最近反例 | 18 个：16 SafeReject、2 Unsupported；Verify 15、Analyze 1、Types 1、Expand 1 |
| 182 raw/Wire | 6 组变异分别经过两条独立 Verify 入口，共 12 次拒绝 |
| 预算 | 成功 27220、少一步 27219 为 ResourceLimit；raw/Wire 分别核对，未修改默认预算 |
| 格式与 Clippy | fmt check；修改生产 lib 与新测试 Clippy `-D warnings` 通过 |

源码测试涵盖多 Desc 捕获的分别读写、当前最后一次调用、同源/异源参数替换、两层与递归 helper 隐藏效果、分支/循环、FnMut 重借、强共享读、参数准备副作用、错误返回与迟到实参失败、零调用与真实资源清理。raw/Wire 变异覆盖增加真实写、篡改捕获来源、抹除 Hold、擦除公开写上界、伪造零调用及未准入的嵌套借入环境形状。

新增的独立审核回归保留同一源输入：闭包仅修改局部 n、读取资源 a，在共享头 h 仍活跃时借入 `&mut F`，实际 O0/O2 输出 A、退出 0；只在同一闭包加入 `d.n+=1` 后仍以 Verify / SafeReject / E-BORROW-CONFLICT 拒绝。

`machine-observations.json` 从实际生成 LLVM 记录环境布局：四模块样本外层环境为三个普通指针，内部两个环境各一个指针。源码经真实机器执行；本片没有增加运行时借用表、隐藏分配或 ABI 字段。未进行运行时间、内存或编译速度比较。

## 保留范围

- callback 显式 Address 参数及 callable helper 的 Address 附加参数仍不开放；原调查输入保留并在原阶段明确 Unsupported。
- 直接资源头/Store Package 窗口捕获、借用 ByteBuffer/Slice 捕获、嵌套 Address<Closure> 捕获、借用环境逃逸、异步 Resume 不在本片。
- 对尚未支持的嵌套外部捕获，Verify 现在独立明确拒绝，不能只依赖 Types 前置拒绝。
- 共享地址仍禁止相交写；独占地址仍要求互斥与真实重借。没有放松强读规则。
- 新 182 机器观察绑定 Linux x64；181 已有 Win64 基础不等于本片混合消费者的原生 Windows 执行证明。
- 作者不运行正式 workspace 全量；完整语言 NotSatisfied、产品 NotRun、B0 状态保持。

## 失败保留与收录

早期未提交原型的日志显式记录 dirty 工作树，不冒充冻结验收。tests01 的真实实现缺陷是环境再借用吞并捕获贷款，导致 helper 组合错误接受，以及把拥有环境的整体仿射性误当成外部捕获写权限；已通过分离环境位置与逐捕获贷款修复，对应源码/raw/Wire 反例保留。

adjacent01 的一项 180 失败是同一非法输入更早触发同一 E-BORROW-CONFLICT，而旧断言限定消息含 alias；授权后只改为精确错误码，输入、阶段与类别未改。tests04 和 consumer01 分别保留示例误用 unsafe 表达式、重复声明同一外部符号的正确拒绝；仅修正示例。frozen01 的 118/120 保留两个来自 178 的既有 callable helper `give R` Expand 回归，按原 Success 期待由 178 修复 6b2a700 并独立复核后重新验收，没有改写旧期待。

frozen02 的 121/0 仅是旧候选限定结果，之后独立审核补出的 FnMut 环境重借正例仍误拒。`returned-reborrow01` 在旧 1e611912 生产上只增加最小回归，真实 0/1 失败；`returned-reborrow02` 保存修复后同输入及最近写反例的执行。22a91ed 修复实际访问检查而非降低期待，`frozen03` 重新冻结 122/0，`quality-final02` 绑定相同源码。旧失败与旧验收都保留，不以旧通过记录覆盖退回事实。

`runs/` 收录真实命令、逐 case 输入/输出/退出及 raw/Wire；不收录可执行程序、对象文件或缓存。`phase-sample/` 是实际四模块 TypedIR/候选/接口；所有新 182 阶段产物以 `phase-artifacts.json` 记录哈希。`FILES.json` 索引完整紧凑收据。

复验：配置 Rust/Cargo/Clang 后，运行 `python3 tools/language/182/check.py --repo . --out /absolute/new-output --target /absolute/independent-target --adjacent`。runner 要求干净已提交源码，创建新输出并保存真实失败。

## Rust 四目标反思

有限安全组合已得到源码、独立 raw/Wire、预算和机器行为支持，尚非完整语言安全证明。54 次真实执行说明表达与现有机器路径可组合，未证明运行/内存优于 Rust。安全的 Desc 与 Copy 地址现在可通过普通 callback/helper 使用，实际维护成本未比较。新增核验有明确有限计费，没有编译提速结论。本片实质推进是消除了与无关 head 的耦合，并闭合环境再借用与当前捕获使用的真实安全缺口；产品级 Rust 净优势未建立。
