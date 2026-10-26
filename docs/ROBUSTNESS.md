# 持续质量探索、独立 oracle 与修复通道

## 工具地位和名称

Roam 从实现初期作为 ZIL 的默认开发基础。公开入口缩短为 **`zil qc`**，其中 qc 表示 Quality Check；内部 Rust 包、`roam/1` case、`roam-worker/1` JSONL 与原 TypeScript 参考保留。hunt 的名称仍可讨论，当前命令与协议不改名，也不增加语言关键字、运行时权限或生产依赖。

Rust 的默认 `session` profile 直接持有 CompilationSession；`scalar-native` 用独立整数模型预测源码程序的实际原生结果；`resource-closure-native` 用独立有限生命周期模型核对真实输出和退出；`byte-buffer-native` 使用独立Rust字节模型、C参考和ZIL/C各O0/O2。四者共用持久问题队列和修复命令。生产14 crate仍只用标准库和路径依赖，工具使用独立workspace与锁文件；实际领域和互操作边界见`tools/roam-rs/README.md`。

## draft22拥有字节扩展

102新增明确的`zil-byte-buffer-native/2`和`owned-byte/1`合同，原/1仍保留。四类载体为局部、Package、保存FnMut和FnOnce；同一动作序列除字节/输出/退出外，还核实际分配身份、失败后原状态保持、唯一释放、释放顺序及退出零存活。

本轮自然24case/48唯一程序/192原生执行为0 findings；固定控制另外覆盖计划内分配失败。受控漏push经旧队列发现、冻结重放、同性质缩减、错误修复拒绝和修复后回归关闭，证明通道有效，不能算自然生产bug。103两类真实词法捕获问题来自非作者审查，也不转记Roam。完整范围、数量和边界见[DRAFT22-ROAM-AUDIT](DRAFT22-ROAM-AUDIT.md)。

## 实际收益与证据归属

本次核查区分三件事：真实编译器缺陷的首次发现、人工注错验证检测能力、测试工具本身的缺陷。不能把它们合并成“Roam 已发现大量编译器 bug”。核查既有及054收据后，**尚无足够记录将一个未经人工注入的生产编译器语义缺陷的首次发现归功于持续 Roam**；这是当前证据边界，不是对未来发现能力的否定。

| 已完成的工作 | 实际证明了什么 | 证据与归属 |
|---|---|---|
| 会话状态路径：84 路径、1311 动作 | 编辑、缓存、取消和配额的可重复回归；warm/cold 一致性 | [002 收据](../evidence/impl/002/acceptance.json)；过期缓存失败由人工注入，8 步缩成 3 步，不是自然发现的生产缓存缺陷 |
| 独立 BigInt 标量 oracle：136 个程序 | 编译并执行实际程序，与独立数值模型核对 | [004 收据](../evidence/impl/004/acceptance.json)；减法变加法是受控变异 |
| Rust scalar-native 与问题队列 | 发现、去重、旧快照重放、缩减、候选及回归验证、关闭的完整通路 | [017 收据](../evidence/impl/017/acceptance.json)、[021 收据](../evidence/impl/021/acceptance.json)；示范发现显式标注 controlled mutation，不能算真实编译器缺陷 |
| 资源闭包原生模型与队列 | 12固定、8生成程序核对，受控省略调用被记录、重放、缩减并验证关闭；干净hunt 8cases/32actions通过、0发现 | [054收据](../evidence/impl/054/acceptance.json)；受控注入不算生产缺陷，051 Copy快照误拒由手工边界发现 |
| host 超时、输入限额、暂停、锁与队列终态修复 | 质量工具自身更可靠，避免错误地卡住或关闭问题 | [006 初审](../evidence/reviews/006-initial-review.json)、[017 独审](../evidence/reviews/017/review.json)；这些由独立审查发现，归入测试基础设施缺陷 |
| 闭包约束及 CT 等编译器修复 | 编译器本身确有真实缺陷被修复 | [014 独审](../evidence/reviews/014/review.json)；首次发现来自审查和专门反例，不能转记为 Roam 的成绩 |

表中数字是对应历史提交的运行记录，不是当前提交的新一次执行结果。当前成绩仍以本轮正式收据为准。

保留一等开发工具支持，是因为状态路径、独立预期、可保存的反例和独立修复通道适合长期维护编译器，也符合项目从初始阶段重视健壮性的目标。这里的一等支持指稳定命令、协议、复现和交接，不要求增加语言语法、运行时依赖或扩张安全可信核。工具的长期价值与它目前发现真实生产缺陷的证据强度是两件事。

此前工具完善和证据流程投入偏多。后续优先完成语言主线，并为新语义保留最近反例；已有 profile 随正式门持续运行。只有出现明确的重复测试需求或真实缺陷族，才把相应领域接入长期探索，避免把每个语言功能都变成先建设一套测试平台的前置任务。

## 已有两种工作方式

**前台修复循环：** 有界 hunt 发现问题 → claim 领取 → 重放旧快照 → 按需 reduce 缩减 → 开发者/Codex 修改并重建 → verify 原例与回归 → 关闭或继续修复。

**长期独立探索：** 一个进程持续 hunt，发现问题独立入队后继续探索；另一个修复进程消费同一队列。生产者不等待消费者，不会在运行途中换测试二进制。新构建启动新 run，原队列仍保留历史。

工具提供上述机制，不自动编辑源代码。本轮验证采用有限任务和明确停止条件，没有启动无限后台服务。

```sh
cargo build --offline --locked -p zil-cli
cargo build --offline --locked --manifest-path tools/roam-rs/Cargo.toml
mkdir -p out/runs

# 每次 --out 使用尚不存在的新目录；每 case 生成有界标量程序。
./target/debug/zil qc --tool ./tools/roam-rs/target/debug/roam hunt --profile=scalar-native --queue=out/qc/issues --out=out/runs/hunt-001 --cases=100 --seed=1 --steps=4

# 在独立终端持续运行，创建 out/qc/STOP 后收口。
./target/debug/zil qc --tool ./tools/roam-rs/target/debug/roam hunt --profile=scalar-native --queue=out/qc/issues --out=out/runs/hunt-daemon-001 --cases=0 --steps=4 --stop-file=out/qc/STOP --max-issues=1000

./target/debug/zil qc --tool ./tools/roam-rs/target/debug/roam issue list --queue=out/qc/issues
./target/debug/zil qc --tool ./tools/roam-rs/target/debug/roam issue claim   --queue=out/qc/issues --owner=codex-fixer
```

claim 返回 `id` 和 `token`；以下占位必须换成真实返回值。先 replay，按需 reduce；完成代码修复与重建后，用新构建的工具运行 verify。issue 命令按保存的 case contract 选择领域，不再额外指定 profile：

```sh
./target/debug/zil qc --tool ./tools/roam-rs/target/debug/roam issue replay   --queue=out/qc/issues --id=ISSUE_ID --out=out/runs/replay-001
./target/debug/zil qc --tool ./tools/roam-rs/target/debug/roam issue reduce   --queue=out/qc/issues --id=ISSUE_ID --token=CLAIM_TOKEN --out=out/runs/reduce-001 --max-runs=64 --budget-ms=30000
./target/debug/zil qc --tool ./tools/roam-rs/target/debug/roam issue verify   --queue=out/qc/issues --id=ISSUE_ID --token=CLAIM_TOKEN

# 仅在尚未关闭、需要交接修复时使用 release。
./target/debug/zil qc --tool ./tools/roam-rs/target/debug/roam issue release   --queue=out/qc/issues --id=ISSUE_ID --token=CLAIM_TOKEN --note=handover
```

`qc` 直接传递 argv，不经 shell；工具优先级是 `--tool`、`ZIL_QC_TOOL`、zil 同目录的 roam。`issue replay` 的退出 0 表示重现同一旧失败；`issue reduce` 的退出 0 表示有限缩减流程完成并保留同一失败性质，不表示全局最小或问题关闭；`issue verify` 的退出 0 才表示关闭条件满足。省略 hunt 的 `--profile` 时仍运行 session 状态路径。

## 关闭问题的必要条件

原快照必须复现原失败身份；候选必须对原输入通过；该 profile 的固定回归集与验证开始时选定的同队列全部已关闭反例也必须通过。已关闭反例可以来自不同领域，按各自 contract 分派。原件、源码身份、候选/历史二进制摘要和认领 token 在结束时复核；全部成功才追加 closed 收据。缺结果、错误 token、篡改、Unsupported、预算耗尽、基础设施错误或不同 property 不能当作修复成功。

问题原件包含具体 case、result、journal、stderr、seed、property、Git 来源和二进制快照身份；发布完整后才对消费者可见。同输入/失败/二进制/来源去重，保留首份原件；修复验证只追加新目录，不改旧失败。旧失败与后来通过各自保留 RunId。

操作锁由内核持有，稳定锁文件不删除/重建；文件存在不表示正在持锁。业务 claim 用 token 协调，不自动超时，进程死亡不等于修复完成。队列适用于受信本机目录，不是分布式租约系统。容量包括已关闭历史；满额时保留当前未发布失败并停止，须显式归档或切换队列。

有效 closed 收据是终态提交点；若进程在发布 closed 后、删除 claim 前退出，list 显示 closed、owner 为 null，并用 cleanupPending 标明物理残留。正常完整关闭仍应删除 claim；无效 closed 收据明确报错。工具不以自动解锁、改写原件或把残留当新认领来隐藏失败。

`--cases=0` 必须提供停止文件或正时间预算；停止在当前有限 worker 的期限内响应，暂停也响应停止/总预算。零执行报告 stopped，不报告 pass。六态为 pass/fail/invalid/error/unsupported/incomplete，不能将所有非零退出混成一种失败。

## 各验证层的责任

| 层 | 当前实现 | 覆盖与限制 |
|---|---|---|
| 规范样本/协议 | conformance 与真实流水线 runner | 固定预期、阶段及明确拒绝；未实现样本保留未满足 |
| 标量持续探索 | Rust `scalar-native`，独立 i128 模型与真实源码至 native | i32/i64 算术、CT、locals、loop、short、branch；当前为 O0，有限 AST 与执行预算，支持队列重放/缩减 |
| 资源闭包持续探索 | Rust `resource-closure-native`，独立有限生命周期模型与真实native | Desc/拥有Fn/FnMut/FnOnce、未调用、转交、提前返回；每程序最多4调用，每case最多32程序；无逃逸、异步或一般协议图 |
| 拥有字节持续探索 | `byte-buffer-native`，独立Rust逻辑模型、C状态解释器及ZIL/C各O0/O2 | 当前生成`/2`合同：局部、Package、保存FnMut/FnOnce四载体；真实分配身份、故障保持和释放顺序；两槽、16动作、容量64，非通用降C后端 |
| 标量 O0/O2 oracle | JavaScript BigInt，真实源码至 native | 独立集成门，算术/CT/短路/有界控制流；不导入生产求值器计算期望 |
| C1 Copy 环境 oracle | `tests/integration/closures.py` 的 Python 模型 | 创建时快照、复制、局部选择/替换、重复调用，O0/O2 |
| 共享捕获 oracle | `tests/integration/borrowed_closures.py` 的 Python 状态模型 | 副本/最后使用/循环/遮蔽、实际地址表示、最近写冲突反例，O0/O2 |
| 混合捕获 oracle | `tests/integration/mixed_captures.py` 的独立 Python 模型及 Rust 对照 | 显式地址与快照、原根保持和最后使用；ZIL O0/O2 与 Rust 实际执行，只作行为证据 |
| 会话状态探索 | Rust Roam，当前 hunt 的默认 profile | 编辑/取消/缓存/配额与 warm/cold 一致；不替代独立语义模型 |
| 变异控制 | 标量、Copy、共享/混合捕获、会话各有控制 | 实际发现指定错误，保存/重放；不是声称发现生产缺陷 |
| 工程交接 | Git、独立审核、handoff、包检查 | 精确来源、规范、记录和恢复，不替代语言安全核 |

**当前标量和有限资源闭包 native 语义域均已进入持续 hunt。** 两者使用独立Rust模型，不启动既有Node/Python oracle充当内部模型；标量O0/O2、C1、共享捕获和混合捕获oracle继续作为独立集成门。054沿用冻结worker、clang身份、ACK journal和同property重放；零执行、预算耗尽与不支持能力分别报告，不能Pass。该模型只探索顺序有限生命周期，不能代表全部闭包或资源状态图。

```sh
./target/debug/zil qc --tool ./tools/roam-rs/target/debug/roam hunt --profile=resource-closure-native --cases=100 --steps=4 --seed=53 --out=out/runs/closure-hunt-001 --queue=out/qc/issues
./target/debug/zil qc --tool ./tools/roam-rs/target/debug/roam hunt --profile=byte-buffer-native --cases=24 --steps=4 --seed=102 --out=out/runs/owned-bytes-hunt-001 --queue=out/qc/issues
```

持续独立运行使用现有 `--cases=0 --stop-file=...`；发现进入同一issue通道，由另一修复进程认领。受控 `closure-omit-call` 仅省略发射源码中的调用，独立模型保持不变。没有自动修代码或无限后台服务的运行声明。

warm/cold 两边共享编译器，可能同时犯同一语义错误；analyze/verify 一致也不是独立语义证明。独立模型仍有有界范围，不能代替 verifier 的安全职责。合法只读捕获中地址与快照可能数值等价，因此共享域还检查 Typed 地址操作和 LLVM ptr 表示。

## 状态、缩减和执行身份

“源码相同”不代表会话历史相同。短路径 BFS 使用具体动作历史，不按当前源码错误合并状态；有限深度不能报告全系统穷尽。长路径保留种子和所有具体动作，重放以动作本身为准。

shrink 保持同 property 的有效失败，做块删除、逐项删除和动作简化；scalar-native 另有保持类型且严格减小的 AST 候选。`issue reduce` 要求现有 claim token，把缩减件、试验和报告写入新的输出目录，队列只追加事件，原件与 claim 不改。默认运行次数与时间都有界，预算耗尽报告 incomplete；不是全局最短搜索，同 property 也不保证同根因。invalid/error/unsupported/incomplete 均不替换已知失败，也不会自动关闭问题。

每轮记录实际二进制 SHA-256、Git/dirty、模型/驱动摘要、平台与 clang。scalar-native 固定 clang 的 canonical 路径、字节与版本，并在实际执行代理中再次检查身份；历史重放拒绝身份漂移。内容寻址快照减少重复 bin；操作系统、链接器、动态库和硬件未密封。正式构建先绑定源码与构建命令，不能只凭运行时 git 信息证明旧二进制来源。

当前 native profile 限 Linux x86_64。host 只监督自己启动的 worker 进程组，clang、链接器与生成程序属于该组；正常退出、错误、超时和协议失败均收口残留子进程。各层有期限与输出预算，超限或回收失败不能 Pass。成功尝试清理临时源码/二进制；性质失败保留已生成的源码、模型预期、实际观察和可获得的诊断/产物摘要；工具错误按实际完成阶段记录，不能保证全部字段。平台范围与完整命令说明见 `tools/roam-rs/README.md`。

## 交付和接续

源码 ZIP 保留代码、规范、小收据、独立审核与关键反例；大产物写唯一 `out/runs`，明确选定的 raw 随可选审计 ZIP。Git bundle 保留原提交与审核对象。`.agent` 解释规则，evidence 证明某一提交实际观察；职责互补，不需要每份主包携带全部成功二进制。

完整交接见 `docs/CODEX-START.md`；源码/历史/审计恢复见 `docs/DELIVERY.md`。正式集成门在干净提交上执行 `python3 tools/validate-draft.py --publish-summary`，只以当轮机器收据给实际计数。当前报告集中在 `evidence/REPORT.md` 与 `evidence/final-summary.json`，不把旧成绩当新提交结果。

每次交付同时按 [Rust 四目标](RUST-GOALS.md) 反思现有证据、未测项和下一项突破。持续检查与独立模型支持开发可靠性，但不证明全语言安全性、整体编译/运行性能或易用性胜过 Rust；这些结论仍需既有产品门的同语义对照与实际计量。
