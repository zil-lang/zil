# Rust Roam：编译器开发底座

本工具独立实现 `zil-compilation-session/1` 的状态路径执行与 `zil-scalar-native/1` 的源码至原生语义检查；保留附件 Roam 的 `roam/1` case、`roam-worker/1` JSONL、失败身份、重放和缩减约束。默认 `session` profile 调用 Rust `CompilationSession`；`scalar-native` profile 用独立整数模型预测结果，再实际编译和运行 ZIL 源码。`resource-closure-native` 按独立有限生命周期模型实际编译并运行资源闭包。`byte-buffer-native` 对同一操作序列运行独立 Rust 模型、C 参考及 ZIL/C 各 O0/O2。四者共用缺陷通道，不增加语言语法、运行时权限或生产依赖。

## 为什么单独实现

原适配器每条路径经过 Node supervisor → Node worker → Rust TSV bridge。原生版本只启动同一 Rust 可执行文件的一个 worker，worker 直接持有长驻会话，并为每次检查创建独立冷会话。模型状态转换与真实驱动分别实现；热、冷语义比较及独立源码 oracle 的责任保持原样。

`tools/roam-rs` 是独立 Cargo workspace，包名 `roam`，不再增加编译器生产 crate。锁定 `serde_json = 1.0.154` 和独立 `Cargo.lock`，使用成熟 JSON 库；摘要复用 `zil-base::Fingerprint`。根 Cargo 和生产依赖不改变。Rust 1.99.1 / edition 2024 实测；Windows 尚未执行平台验收。

## 开始运行

在仓库根目录，加载仓库要求的 Rust 工具链。首次准备仅下载独立测试工具依赖，以后可离线执行：

```sh
cargo fetch --locked --manifest-path tools/roam-rs/Cargo.toml
cargo test --locked --offline --manifest-path tools/roam-rs/Cargo.toml
cargo run --locked --offline --manifest-path tools/roam-rs/Cargo.toml --bin roam -- campaign --out=out/runs/roam-001
```

`out/runs` 父目录需要存在，`--out` 必须是尚不存在的新目录。每次运行使用新编号，绝不覆盖旧结果。正式 campaign 要求 Git 源码与已跟踪文件干净；开发运行显式追加 `--allow-dirty=true`，报告保留实际 `sourceDirty`。`--commit=<hash>` 可要求准确匹配 Git HEAD，不能注入另一个来源身份。使用现成可执行文件时，报告记录实际字节摘要与运行时 Git 状态；这不单独证明旧二进制来自该提交。正式交接命令先执行上述 Cargo 构建再运行，记录命令与二进制摘要。

命令退出 0 表示声明的有限 campaign 全部满足条件；退出 1 为 campaign 未满足条件。独立重放退出 0/1/2 分别表示 pass、同一执行报告中的 property fail、invalid/error/unsupported/incomplete 或宿主错误。不能把“非零”一概当作没有复现。

## 协议与行为

- `steps` 的具体动作是重放权威，seed 只记录生成来源。Mulberry32 与原 TS 动作表顺序一致。
- worker 发 `before`；host 逐条写 journal 并 `sync_all` 后才排入 ACK；有界独立 writer 写 stdin，主监督循环继续计时，超时先终止并回收 worker，再收口 writer/stdout/stderr 三个线程；真实动作结束才发 `after`。模型检查失败仍保留已完成动作计数。错误 ACK 阻止该动作执行。
- 六态为 `pass / fail / invalid / error / unsupported / incomplete`。未知动作、过期契约、非空 schedule、配额耗尽、缺失结果、重复终态、错帧、超时都不能成为 pass。非空 schedule 无论是否有同名能力声明都拒绝。
- 每次冷检查禁用磁盘缓存并使用独立取消令牌；Verify 到达时必须实际执行，不可从缓存复用。每步结束检查活动查询归零。
- case 1 MiB、单帧和总输出 4 MiB、默认 256 步、worker 10 秒、host 10 秒；case/manifest 文件先按限额加一字节有界读取，再解析，不能先整文件分配后拒绝。API 构造的 case 同样检查编码大小；JSON 库还保留自身深度界限。可执行摘要使用独立 128 MiB 文件上限，该限额与 JSON 配额分别计算。负零、非有限数和非安全整数 seed 拒绝。Rust JSON 字符串必须是 Unicode 标量序列，不能表示 JavaScript 孤立 UTF-16 surrogate；超出该互操作子集安全拒绝。
- 显式默认 `initial` 以对象结构和值比较，JSON 属性排列不是状态语义。原 TS 会话适配器的 `JSON.stringify(initial)` 比较对键序敏感；本工具不继承该偶然限制，不宣称任意 JS 对象级行为逐字节等价。
- 正常退出尝试清理会话临时目录。清理失败会把 pass 改为 error；原 property failure 保留。强制终止的 worker 可能留下带本进程标识的临时目录，主机不能把这种路径当成已清理成功。
- `session` worker 不启动 clang；`scalar-native` worker 会启动 clang、链接器及生成程序。Linux host 只为自己启动的 worker 建立独立进程组，native 正常结束、错误、超时与协议失败均终止残留组成员并回收直接子进程；不向 case 或队列提供的 PID 发信号。这是受信开发工具的进程生命周期管理，不是任意外部程序的安全沙箱。

## 探索与缩减范围

正常集为 11 条固定路径、七动作字母表深度二的 56 条边及初态检查、16 seed 各 64 个随机动作和 7 个恢复动作：合计 **84 条正常路径、1311 动作**。BFS 用完整历史作状态键，避免错误合并缓存历史；报告 `complete: false, stopReason: depth`，不称整个系统已穷尽。

测试专用 `stale-after-edit` 注入返回过期热报告。三动作字母表 BFS 发现三步见证；八步带冗余原例重放三次，再按原策略逐块/逐步删除，要求同 `session.warm-cold` property、三步缩减件复现及关闭注入的干净对照通过。Invalid、Error、不同 property 不能替换已知失败；原 case 文件摘要在结束时复核。这个注入不是生产编译器缺陷。

它实现当前会话 profile 的状态核与宿主，不是原附件全部 API 的翻译：没有通用异步 `RoamSpec` 插件、资源依赖图缩减、多 worker FileStore 租赁/恢复、Bun freezeDriver、浏览器插件或显式调度器。闭包/资源/并发能力进入 ZIL 时，应增加对应 profile 和独立模型，不能把当前 84 路径当作覆盖了它们。

## 证据和重放

每个 campaign 的快照只有一份内容寻址可执行文件及 manifest；不为每次成功路径复制源码树。开始和结束验证该快照，历史重放在启动前验证。此校验只证明快照字节/平台一致，不冻结操作系统、CPU 或动态库。Git 提交提供源码历史；不会因为保留 native 快照而宣称可跨任意环境复现。

冻结时只打开源可执行文件一次，从该句柄读取 metadata、权限及最多 128 MiB 的字节；摘要与目标写入使用同一个缓冲，随后仍核验快照。构建若在打开后替换原路径，冻结保持已打开的文件对象，不再重新按路径复制另一份产物。该句柄不等于禁止外部原地写入的文件系统快照。工具独立 Cargo 的 dev/test 统一 `debug=0`，正常使用 `cargo build` 和 `cargo test`，无需临时追加另一套 strip 参数。

成功路径执行期间仍逐步持久化 ACK journal，通过后删除临时尝试，只在 `report.json` 保存计数、检查统计及耗时。失败保留 case、完整 result、journal、stderr、环境引用；`original.case.json` 和 `shrink.json` 不覆盖。大型可执行文件、完整开发运行放 `out/runs/`，不加入普通源码包。正式交接只晋升摘要、原例/缩减例、小型检查日志与内容摘要。

```sh
# 从一个已保存 campaign 取 snapshot 路径。
roam replay --case=out/runs/roam-001/original.case.json --snapshot=out/runs/roam-001/snapshots/<sha256> --fault=stale-after-edit --out=out/runs/replay-001
roam replay --case=out/runs/roam-001/reduced.case.json --snapshot=out/runs/roam-001/snapshots/<sha256> --fault=none --out=out/runs/clean-001
roam verify-snapshot --snapshot=out/runs/roam-001/snapshots/<sha256>
```

原例可由 TS 与 Rust worker 互相消费。原 TS vendor 保持原字节，只作为兼容参考及旧证据重放入口，不再是本 Rust campaign 的运行依赖。

## 持续探索与独立修复通道

新增 `hunt` 和 `issue`，根 CLI 提供 `zil qc` 短入口。`qc` 是 Quality Check 的开发工具入口；内部 case/worker 协议仍为 `roam/1`、`roam-worker/1`。本节流程与既有有限 `campaign` 验收并存，不改变 84/1311 正常集或其判据。

```sh
# 有界探索；out 必须为新目录。首次运行自动建立队列。
roam hunt --out=out/runs/hunt-001 --queue=out/qc/issues --cases=1000 --seed=1 --steps=64

# 长期独立探索；停止文件出现后，在当前有限 worker 执行结束时收口。
roam hunt --out=out/runs/hunt-daemon-001 --queue=out/qc/issues --cases=0 --stop-file=out/qc/STOP --max-issues=1000

# 修复者在另一个进程/终端工作。
roam issue list --queue=out/qc/issues
roam issue claim --queue=out/qc/issues --owner=codex-fixer
roam issue replay --queue=out/qc/issues --id=ISSUE_ID --out=out/runs/replay-001
roam issue verify --queue=out/qc/issues --id=ISSUE_ID --token=CLAIM_TOKEN
roam issue release --queue=out/qc/issues --id=ISSUE_ID --token=CLAIM_TOKEN --note=handover
```

`ISSUE_ID`、`CLAIM_TOKEN` 必须替换成 claim 返回值；claim 可指定 `--id`，省略时领取一个未领取问题。修复代码与构建由施工者完成，再使用**新构建的 roam** 调用 verify；工具不执行队列中携带的脚本，也不会自行修改编译器。前台“发现后立即修复”使用相同的 claim→改代码→构建→verify 流程；后台 producer 不等待修复者。

### 队列和关闭条件

队列默认为 `out/qc/issues`，支持同一受信本机目录上的多个 producer/consumer。每个问题先在 incoming 完整写入并同步，再原子发布到 entries。原始 case、result、逐步 journal、stderr、种子、性质身份、Git 来源及内容寻址二进制均保留；重复的输入/失败身份/二进制/来源记录只保留第一份原件。成功路径删除尝试目录，仅累加 `progress.json`；每次 run 的最终 `report.json` 不覆盖旧 run。

claim 使用原子记录与 token，只用于合作进程协调，不是权限认证。短期操作互斥使用 Rust 标准库 `File::try_lock` 的独占文件锁；`locks/` 中零字节文件保留原 inode，文件存在不等于锁被占用。操作结束或持有进程终止后，对应句柄关闭，系统释放锁；不能删除/重建活跃锁文件。verify 追加自己的独立目录，并依次要求：旧快照重现原失败身份；当前候选对原 case 返回 pass；当前 profile 的固定回归及验证开始时选定的全部已关闭反例通过（按各自 contract 分派）；原件、候选/旧快照摘要、claim 与源码身份结束时仍一致。session 固定集仍为 11 条；scalar-native 为 6 组、46 个边界程序。只有全部成功才新增 closed 收据并释放 claim。错误 token、篡改、工具错误、Unsupported、超时、预算耗尽或不同失败性质都不能关闭问题。修复验证不是整个编译器全量门，合入仍须独立审核和集成验收。

原件永不由这些命令修改；验证结果仅追加。closed 的问题继续作为未来修复的回归输入。`--allow-dirty=true` 允许开发运行，来源及关闭收据明确保留 development；正式默认拒绝脏源码。二进制摘要证明实际字节，运行时 Git 记录不单独证明构建来源，所以正式集成仍须先构建、记录命令再执行。

有效 closed 收据是终态提交点，随后删除 claim 属于清理。如果进程在两步之间退出，`issue list` 仍显示 `state=closed, owner=null`，并以 `cleanupPending=true` 明示残留 claim；它不读取或自动删除残留内容。正常完整关闭应删除 claim，残留不能被误读为新的活跃修复者。损坏的 closed 收据仍明确报错，不能用一个任意文件宣布关闭。

### 状态、停止与容量

六态保持：property fail、invalid 输入、error 基础设施、unsupported 能力和 incomplete 预算分别分类入队。`hunt` 返回 0 表示有界全部通过或按请求停止；两者的报告状态分别为 pass、stopped，零执行绝不报告 pass。发现非 pass 后继续，最终返回 1；通道/输入/来源等执行错误返回 2，并保留尚未发布的尝试目录。`issue replay` 的 0 表示**重现了同一失败**，不表示问题已修复；`issue verify` 的 0 才表示上述关闭检查全部通过，1 为检查未通过，2 为输入/宿主错误。

`--cases=0` 必须配停止文件或正数 `--max-ms`。默认单 worker 最多 10 秒，可设 `--timeout-ms=1..60000`；停止在当前 worker 的期限内响应。`--pause-ms` 可限制探索速率。`--max-issues` 默认 1000、最多 10000，计入已关闭原件以防历史无限增长；满额时保留新失败并停止，需显式归档/更换通道，不静默丢弃。长期运行固定本次二进制；新源码构建后启动新 run，继续使用同一通道，不在运行中换测试对象。

`--case=case.json` 可替代随机生成，不能与 seed/steps 同时使用，profile 由 case contract 确定；同时提供 `--profile` 时必须一致。测试控制 `--fault=stale-after-edit`（session）或 `--fault=scalar-sub-to-add`（scalar-native）明确记录 controlledMutation；它用于证明发现/交付/修复验证机制有效，不代表真实生产缺陷。verify 上同名选项用于拒绝坏候选的控制检查，默认关闭。

claim 不自动超时，长期修复不会被另一个进程抢走；持 token 者可显式 release。进程异常终止释放的是短期操作锁，未完成的业务 claim 和 incoming 暂存件仍保留，不自动声称修复完成。旧试验版本的目录锁须在全部旧进程停止后显式清理或换用新通道；两种锁实现不能混用活跃队列。当前锁文件不需要清理，恢复/复制队列也应先停止所有持有者，避免替换仍被锁定的 inode。平台或文件系统不支持文件锁时明确失败，不回退到无锁。本片没有跨机器租约、自动杀死修复者、自动恢复分布式服务或整个 OS 的密封快照。队列是受信工作区中的可执行复现材料，不应直接执行来历不明的外部队列。

标准库合同见 [`File::try_lock`](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock) 和 [`TryLockError`](https://doc.rust-lang.org/std/fs/enum.TryLockError.html)。文件锁 API 自 Rust 1.89 稳定；本项目按 1.99.1 实测，不引入额外锁依赖。

## 独立 scalar-native 语义探索

```sh
# 每个 case 默认 4 个独立程序；下面执行 12 个 case，每个 8 个程序。
roam hunt --profile=scalar-native --queue=out/qc/issues --out=out/runs/scalar-001 --seed=7 --steps=8 --cases=12

# 生成的 typed AST case 可保存后按相同 contract 重放。
roam generate --profile=scalar-native --seed=31 --steps=8 > scalar.case.json
roam hunt --case=scalar.case.json --queue=out/qc/issues --out=out/runs/scalar-case-001 --cases=1

# 领取后的同性质缩减：原 case 不改，不关闭问题。
roam issue reduce --queue=out/qc/issues --id=ISSUE_ID --token=CLAIM_TOKEN --out=out/runs/reduce-001 --max-runs=64 --budget-ms=30000
```

输入使用已有 `zil-scalar-native/1`：i32/i64 的字面量、取负、检查及回绕整数算术，expr/ct/locals/loop/short/branch 六种程序形状。每个程序最多 256 个 AST 节点、深度 8，循环次数 0..12；每 case 最多 32 个程序。整数字面量是十进制字符串，接受协议中的 `-0`，模型与发射器把它解释为 0，原始输入保持原样。随机生成是确定性的，重放权威仍是保存的实际 steps。Rust 的种子到样例序列属于这个生成器，不宣称与旧 TS 生成器逐个相同。

预期值由 Rust i128 模型直接计算：i32/i64 运算的精确中间值、除零、最小整数除以 -1、检查溢出、回绕和短路求值均有独立处理。模型不调用 ZIL parser、types 或 CT 求值器。真实侧通过生产 driver 生成 LLVM、clang 构建并运行，比较完整整数输出、Linux x86_64 trap 或带精确阶段/错误码的 CT 算术拒绝；不是热/冷编译对拍。当前使用 driver 默认 O0；O0/O2 对拍、闭包、资源与并发的独立 oracle 仍属于其他验收范围。

native profile 当前要求 Linux x86_64。每次 run 固定 roam 字节快照，case 另记录 clang 的 canonical 路径、SHA-256、版本及平台。动作开始时验证身份，内部 clang 代理再次校验实际执行路径和字节，历史 replay/verify 拒绝身份漂移。代理参数来自编译器生成的固定 argv，输入和输出必须位于该 worker 所拥有的步骤目录；不执行 case 或队列中的 shell 字符串。clang、链接器、生成程序留在 worker 所拥有的进程组内；独立版本探测也拥有并清理自己的进程组。

clang 版本探测、编译器外部调用与程序执行的 stdout/stderr 先写临时文件，运行中检查单流 16 KiB 上限，再以限额加一字节读取，避免先在内存收集无限输出。轮询期间磁盘写入可能短暂超过阈值；超限、超时和回收失败均不能 Pass。版本探测限 2 秒，单次 clang 5 秒，程序 3 秒，外层 worker 总期限仍优先；本工具不冻结 OS、CPU、链接器和动态库。失败结果保存有界源码、独立预期、实际结果、诊断、argv 和 LLVM/执行产物摘要，成功尝试清理临时源码与二进制。

`scalar-sub-to-add` 只把发射源码中的减法换成加法，独立模型不变，用于验证“发现→记录→重放→缩减→坏候选拒绝→回归后关闭”。这些发现明确标为 controlled mutation。`issue reduce` 先删除步骤，再对 scalar AST 做保类型、严格减小的候选替换；仅接受同一 failure property 的真实重放，Invalid/Error/不同性质都拒绝。默认最多 64 次 worker 执行和 30 秒，上限分别 256 与 60 秒，零次或预算耗尽报告 incomplete。缩减报告、case 与 trial 结果写入新输出目录，队列只追加带摘要的事件；claim 和原件不改，也不宣称全局最小。

## 跨语言验收

先构建同源码、同 debug 配置的旧桥，再运行有限差分：

```sh
cargo rustc --locked --offline -p zil-driver --example roam_session -- -C strip=debuginfo
cargo rustc --locked --offline --manifest-path tools/roam-rs/Cargo.toml --bin roam -- -C strip=debuginfo
node --experimental-transform-types --import ./tools/roam/register.mjs tests/roam-rust/conformance.mjs --out=out/runs/roam-cross-001
```

检查 16 seed 的逐动作一致、8 类原 TS supervisor → Rust worker 互操作、两侧相同 84/1311 正常集，以及原 TS reducer 驱动 Rust worker 与 Rust reducer 的同结果/同运行次数。协议测试另覆盖实际错 ACK、缺结果、错误 JSON、颠倒 checkpoint、错误动作、重复终态、伪造 pass、错误计数、输出预算、崩溃、超时和快照篡改。

报告分别给出两种宿主一次正常集耗时。它包含进程启动、JSON/ACK 与持久化成本；不等同于纯编译器时间、整体语言性能、跨平台性能或统计性能门。

## 独立审核修复记录

首轮独立审核发现 hello 后不读 stdin 的 worker 可阻塞原同步 run-frame 写入，100ms timeout 实际等到 1229ms；另发现 case/manifest 在限额校验前整文件读取。两项原 Fail 保留。修订把 pipe 写移到有界队列的独立线程，并让监督循环负责 deadline/kill/reap/join；JSON 输入读取最多 limit+1 字节，摘要另限 128MiB。新增真实 512KiB blocked-write、计数 reader、256MiB 稀疏文件/CLI 拒绝三项回归，详见本次 rework 收据。

## 资源闭包原生探索（054）

`resource-closure-native` 使用独立有限生命周期模型预测输出与退出码，覆盖 Desc 调用时读取、拥有后重复读取/修改、消费及剩余字段清理、未调用销毁、环境转交与提前返回。它实际编译ZIL并运行原生程序；不能将 Unsupported 或缺失执行视为成功。它不是一般资源协议或完整闭包状态证明。

```sh
roam generate --profile=resource-closure-native --seed=53 --steps=8
roam hunt --profile=resource-closure-native --cases=100 --steps=4 --seed=53 --out=out/runs/closure-hunt-001 --queue=out/qc/issues
```

有限模型每程序最多4次调用、每case最多32程序，输入范围有界；同一已有发现/记录/独立修复队列可以长期运行。`closure-omit-call` 是专用控制注入，只使发射源码省略调用，预期模型不改；此注入只证明探测链有效，不记作生产缺陷。源码、二进制、clang身份和ACK journal沿用既有冻结与重放合同；源码参数缩减仍要求相同 failure property，不能将无效输入或另一错误当作复现。

## 动态字节存储的三方差分探索（098，冻结 /1 合同）

当前命令默认生成下节102的 `/2`；以下记录 `/1` 的历史运行命令与能力，旧case不改标：

```sh
roam hunt --profile=byte-buffer-native --cases=24 --steps=2 --seed=98 --timeout-ms=30000 --out=out/runs/bytes-001 --queue=out/qc/issues
roam generate --profile=byte-buffer-native --seed=98 --steps=2 > bytes.case.json
```

`zil-byte-buffer-native/1` 每个程序最多 16 个操作、容量最多 64 字节，每 case
最多 8 程序、默认 2 程序。操作包括显式 reserve、满容量不分配的 push、受实际
长度保护的范围 fill、give relay、消费后重建 reset；负容量、较小 reserve、空
范围及无效范围均有可观察结果。每步输出成功标志、长度、容量和全部有效字节，
最后比较正常或提前返回的退出值。生成器的具体操作才是重放权威。

Rust 模型使用 Vec 与逻辑容量；C 参考以独立 switch 解释相同操作表，使用有界
数组和显式逻辑容量；真实侧使用 ZIL ByteBuffer、动态借片、函数转交与清理。
C 参考不读取 Rust 期望、ZIL IR 或实际输出。每个程序实际构建并执行 ZIL O0、
ZIL O2、C O0、C O2 四个程序，四个结果都须等于独立模型。它是特定语义域的
差分参考，不是通用 ZIL→C 编译器。两种真实程序仍共用同一个 clang 后端；
Rust 模型提供额外独立预期，但整个有限测试不构成全语言正确性证明。

每个 `nativeEvidence` 保存一份 ZIL/C 源码及各优化的命令、编译结果、实际
stdout/stderr、退出、二进制和 LLVM 摘要。clang 仍按已有身份协议固定，所有
子进程归监督 worker 进程组管理；C 编译参数由工具生成，不接受 case 中的
命令文本。旧标量与资源闭包 profile 的 O0 合同不变。

`bytes-omit-push` 只省略 ZIL 发射器中的 push，独立 Rust/C 语义保持；该控制
必须明确标注人工变异。新域复用现有 issue 的原件、重放、claim、同 property
缩减、坏候选拒绝和回归后关闭；缩减可以删除程序内操作及提前返回标志，不
声称全局最小。零执行、过大/无效输入、工具错误和预算耗尽均不能报告 pass。

旧 `/1` 域未注入分配失败，也未跟踪 free 次数或泄漏；正常/提前返回只是实际经过
清理路径。Package/闭包捕获、错误载荷、外部回调、并发和 Windows 不在本域。
现有专门机器测试中的故障分配器证据不自动记为本 profile 的覆盖。

### 当前作用的审计

098 前的三个 profile 已提供状态历史回归、独立标量/有限资源模型以及保存和
缩减失败的实用通道，但没有持续 C 对拍，ByteBuffer 等新能力也没有进入 hunt。
既有记录尚不足以把未经注入的生产语义缺陷首次发现归功于持续 Roam；人工
变异和独立审查发现应各自记账。已有 O0/O2 Node/Python 集成 oracle 与 Rust
对照脚本也不能转记为 Roam hunt 的成绩。098 补的是新的有限语义域及双优化
差分，不把这些历史不足改写成已完成。

## 拥有字节载体与实际分配观察（102，当前生成版本）

当前 `generate` / 随机 `hunt` 仍选择 `byte-buffer-native`，明确生成
`zil-byte-buffer-native/2` 和 `owned-byte/1` 模型。上节 `/1` 是098的冻结合同；
旧输入继续按原模型解释，不向旧case补载体或分配含义。旧快照不能认识 `/2`
时明确unsupported；固定case的hunt收据scope绑定实际合同。

```sh
roam hunt --profile=byte-buffer-native --cases=24 --steps=2 --seed=102 --out=out/runs/owned-bytes-001 --queue=out/qc/issues
roam generate --profile=byte-buffer-native --seed=102 --steps=2 > owned-bytes.case.json
roam replay --case=owned-bytes.case.json --out=out/runs/owned-replay-001
```

每程序两个字节槽，载体为 `local`、`package`、`fnmut`、`fnonce`，仍限制16动作、
64字节容量、每case最多8程序/默认2程序。局部载体由普通give参数逐步转交独立
拥有者，不依赖尚未开放的Managed局部赋值。Package真实取走/补入/替换字段；
保存工厂实际返回FnMut或FnOnce，FnMut错误返回后可继续调用，FnOnce覆盖部分
消费的正常与错误清理。零动作程序仍实际创建/清理载体；零程序case仍incomplete。

除旧字节动作外新增 `replace`、`take_refill`、`prepare_error`。replace先完成
新分配/初始化，成功才释放旧值；失败保留旧槽。take_refill先独立持有旧责任，
用新头补齐（分配失败时为空头），再消费旧责任。prepare_error在真实普通错误
出口清理刚准备的临时值，保留原槽。`finish`明确正常/error或take0/take1及其
错误出口；FnMut不接受部分状态退出。`early`仍只选择最终进程退出0或7。

`failAt`为0时不注入分配失败，正数指定第几次实际增长请求返回NULL；负容量或
容量已够不计该序号。默认随机hunt始终failAt=0。计划OOM案例有独立标记，不能
与无注入hunt合并统计；`bytes-omit-push`仍是另一个明确的语义变异控制。

Rust模型和C有界数组解释器分别计算两个槽的字节、容量、逻辑分配事件和清理
序列。ZIL侧只链接测试内置的malloc/realloc/free观察器：成功新分配给逻辑ID，
成功realloc更新真实指针映射并保留ID，失败保持旧映射，free退役ID。日志记录
A新分配、G增长、F计划失败、D释放，最后L00表示零遗留；未知/重复释放明确失败。
不同进程的裸地址从不比较。宿主非计划OOM明确报工具环境error，不算模型通过。

C参考不使用该观察器、Rust期望或ZIL结果；它仍是独立有限语义参照，不是通用
生产降C后端。四个变体仍是ZIL/C各O0/O2。监督clang proxy只附加本冻结worker
内置、逐字节及hash核验的观察器，记录实际完整argv和观察器身份；不允许case
提供链接命令，时间/输出/证据预算保持。旧scalar/resource-closure路径不附加它。

新性质 `byte-buffer.owned-c-model/1` 使用原队列、快照、回放、同性质缩减及
关闭流程。仍未覆盖借入捕获、环境裸片返回、拥有错误载荷、一般元素、任意
分配器、并发/取消或Windows；测试shim不进入语言运行时，也不构成安全证明。
