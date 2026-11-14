# 大项目三臂测量工具：冻结源码独审

**结论：接受本次限定范围的工具源码复核。此前两项 P1 均已闭合；未发现需要阻止首轮测量的新增具体 bug。该结论不代表 benchmark、当前原生质量门或性能目标已经通过。**

审查对象为 `/workspace/zil-push-main` 的 `tools/research/incremental-demand/large_project_matrix.py`，并沿其调用核对既有 matched runner、独立整数 oracle 和磁盘统计边界。本会话没有参与该工具实现。本轮只复核两项修复并完成此前已经展开的具体质量边界，没有增加框架、扩大源码审计或运行任何测量。

## 冻结身份与差异绑定

| 项目 | 精确身份 |
| --- | --- |
| 冻结提交 | `ad81486b976634e71bf1c61544d0fd28a783fbae` |
| 直接父提交 / 本次修复 base | `6958d700e4d09b050d70d930e7f296c41e1275ae` |
| 冻结 tree | `eef81d10a19a21fc7588d76b934e2001a680b214` |
| base 脚本 SHA256 | `16409409cf768dace14b09947518dbe56367ee87f29051d3262de5fa15baef8c` |
| 冻结脚本 SHA256 | `9e7cf9cbbd1f5db46d22a73da48e25eb4e09b291b6f946b41aa139b750ca5f73` |
| 本次完整差异 SHA256 | `b4c2460ad743faf950651baed38a2fe209792839f7fb1963ca93b73889ffebaa` |
| 差异长度 | 3142 bytes |

diffsha 的确定编码为：对 base→freeze 的全部变更路径按名称排序，用 Python `difflib.unified_diff`、3 行 context、`a/<path>`/`b/<path>` 文件名、UTF-8/LF、无 timestamp/index headers，连接结果后计算 SHA256。它明确绑定这一编码，不混用不同选项产生的 diff 摘要。本次全部变更只有上述一个脚本。通过只读现存对象及树记录核对父链、完整变更集合、两版脚本与当前文件；没有调用 Git 命令，也没有改变 refs/index/worktree。

差异内容只包括：导入 `subprocess`；提取七文件 `implementation_paths`；新增和前后调用 `verify_frozen_tools`；向 args 绑定两臂 source；在 push/O2 收尾调用 SDK archive。没有修改生成算法、history、oracle、编译 flags、matched 质量门、磁盘 roots 或统计实现。

## 两项 P1 的关闭证据

**P1-1：实际执行工具与 tool_source 的绑定，已关闭。**

[implementation_paths / verify_frozen_tools](/workspace/zil-push-main/tools/research/incremental-demand/large_project_matrix.py:287) 列出实际加载的五个 incremental-demand 脚本，以及两个 reactive-query-graph 文件。路径解析后必须属于 `args.repo`；每个实际文件都与 `tool_source:<relative-path>` 的冻结原字节比较。不是只看 checkout SHA、宽松 source_identity 或自报哈希。

[pin_inputs:302](/workspace/zil-push-main/tools/research/incremental-demand/large_project_matrix.py:302) 在工具/编译器准入和任何 compiler request 前调用该检查；[收尾:459](/workspace/zil-push-main/tools/research/incremental-demand/large_project_matrix.py:459) 在成功汇总前再调用。任一不匹配、路径越界、对象缺失或读取失败均沿异常路径结束为 invalid，不能生成成功的速度/磁盘比较。`subprocess` 已导入，不存在本修复引入的未定义名称问题。

七个工具的当前快照 SHA256 如下；源码检查确认新的 guard 会逐一执行字节比较。本审查没有运行 guard 中的 Git 子进程。

| 文件 | SHA256 |
| --- | --- |
| incremental-demand/large_project_matrix.py | `9e7cf9cbbd1f5db46d22a73da48e25eb4e09b291b6f946b41aa139b750ca5f73` |
| incremental-demand/rust_compare.py | `75caebcec1ef29a0cdbdfd83d2ad623e22833458761f400d85722f70ce56febf` |
| incremental-demand/push_counters.py | `b268f773eeba186f05d39ef26f026d5833a592614389d25989b5ab413eb491af` |
| incremental-demand/native_link_matched_measure.py | `f78cd7c56098fc74a287cbbe9b54112c61db1b032a6f1b57c6640c17cb5ee304` |
| incremental-demand/native_link_matched_controller.py | `89080de996ce316bb90ee69b7bdcf74fc6bb97e97830529b558fca04ff04fd29` |
| reactive-query-graph/native_workloads.py | `df6e95b48825063b718fd65a0c4ab8afaa940f6880a40215d60ac9105ced6964` |
| reactive-query-graph/native_measure.py | `a6ada1c84e880896d3a10a8aaf5e52c79aa2b3fc6f006e0d72a028977a509b52` |

**P1-2：push/O2 的完整 native-link SDK 证据闭包，已关闭。**

[pin_inputs:315](/workspace/zil-push-main/tools/research/incremental-demand/large_project_matrix.py:315) 现在为 baseline/candidate 分别设置完整 `*_source`，满足固定 helper 使用 `args.candidate_source` 读取已编译 policy 的接口要求。`native_link_profile` 及其 hash 在原准入处仍绑定。

[收尾:460](/workspace/zil-push-main/tools/research/incremental-demand/large_project_matrix.py:460) 只在 `candidate_policy == 'push' and mode == 'o2'` 调用 [matched.archive_native_profile](/workspace/zil-push-main/tools/research/incremental-demand/native_link_matched_measure.py:705)，把结果存入 manifest。实际调用位于全部请求及 worker 结束后、成功汇总前，不进入 request timer，也不会预热首个冷请求的 SDK 首次加载。

helper 重新核 manifest hash、候选 source 的编译 policy 原字节、完整文件和目录集合、所有文件 size/mode/hash，拒绝 symlink/额外或缺失项，并归档 manifest 与文件字节。失败使本窗口 invalid；没有只记录路径/hash 就宣告 SDK 归档成功。完整 SDK 仍写入 evidence blob，不进入用户磁盘 roots。

stock 不加 profile flag、不调用 archive、不读取该 profile 内容；pin schema 中保留的 profile 路径/hash 只是未使用字段。stock candidate 另要求与 baseline 的 pinned LLVM DSO hash 相等，实际 mapped DSO 还须与各自 pin 一致。push/check 不归档 native profile，因为没有 native-link 消费。

## 先前已展开的质量边界：完成核对

### 生成源、独立 oracle 与完整定义闭包

[fixture](/workspace/zil-push-main/tools/research/incremental-demand/large_project_matrix.py:97) 同时生成两语言完整源，再成对施加 graph、simple 和位置变更。既有 validator 先核原始 helpers/分支系数/调用 inventory，变换后再核两语言的实际 import/call edges、helper inventory、分支或 straight-line 系数。Rust 的根 `mod` 目录保留全部文件；ZIL 从根 imports 的可达集合必须覆盖全部模块。

large presets 每模块保留 helper、pub value 和一个未调用定义；没有为了让 error 或性能窗成功而裁去 unused callee/模块定义。small425 控制不额外加 unused 定义。error 在 large 的 `m0.unused_checked` 内注入唯一 `absent_name`，不把它接入运行路径。

[Oracle](/workspace/zil-push-main/tools/research/incremental-demand/large_project_matrix.py:180) 使用固定六输入。wide 的 branchy/short 复用独立整数解释器；simple/DAG 用反向拓扑整数求值。它不解析生成文本、不调用编译器，也不复用编译器答案；edit delta 是明确共用的输入规格。branchy 是 16 次原分支，short 是一次分支，simple 是一次算术表达式，三者没有混称。

wide 在根累加全部 module.value；chain 在各节点累加下游值；diamond 是 `0→{1,2}→3→tail chain`，下游只执行两次。oracle 的复用节点计算与源程序重复执行的纯标量结果相同，未引入指数运行时或有状态共享。输入、乘加及各级模运算保持 i32 范围。此静态结论不保证所有规模都能在当前默认编译资源预算内通过；ResourceLimit 必须按失败保留。

### 未用定义错误与当前质量门

[matched.zil_request](/workspace/zil-push-main/tools/research/incremental-demand/native_link_matched_measure.py:669) 对 error 要求 SafeReject、types 阶段、verification_runs=0 和精确唯一的 E-UNKNOWN-NAME 路径/字节 span；Rust 要求退出 101、build-finished=false 和精确唯一 E0425 主 span。先前已发布 exe/IR 不得因错误被替换，error 不执行旧 exe，也没有伪造 native oracle 值。

成功的两条 ZIL 臂要求当前独立 Verify 一次，query event 亦必须恰有一次 executed Verify。push 的额外 observer 检查仍保留，未以它代替 current Verify。这里的“独立 Verify”是 ZIL 门，不能推广为 Rust 执行相同安全工作。

O2 沿 [matched.quality](/workspace/zil-push-main/tools/research/incremental-demand/native_link_matched_measure.py:226) 检查：

- 每臂成功请求均有六输入×两轮的完整 exit/stdout/stderr oracle 相等。
- main/candidate 的当前完整 ELF 归档经长度/hash/ELF 头检查后逐字节相等；并核聚合函数 exports。
- 两条 ZIL 臂的物理 16-unit inventory、当前 LLVM ownership/layout 与报告相符。终端产物复用按固定 helper 的当前 preflight/保留 producer 证据分域，不能冒称重新执行 LLVM verifier 或 16 次新 codegen。
- Rust manifest 固定请求 CGU16/O2/jobs1，保存实际 current incremental objects，不能把“请求 16”推广为与 ZIL 分区相同或 Rust 恰好 16 个物理对象。Rust 不要求与 ZIL 的 ELF 字节相等，其行为由独立六输入 oracle 检查。

LLVM 文本是否相同是观测，不是这里额外放宽 full-ELF 门的理由。check 窗只验检查流程，不具有 O2 native/full-ELF 验收。上述门在本次两项修复中均没有变化。

### 失败、样本、pin 与窗口身份

三臂任一 unexpected failure、Unsupported、ResourceLimit、超时或质量门失败都保留失败行/原始证据，完成该请求的三臂记录后终止窗口。成功汇总必须有全部预期 rows/pairs，且每臂每请求具有每 block 一个有限正 wall-time 样本。失败窗口不发布速度/磁盘比较；预期名称错误通过的是“正确拒绝”质量门，不能描述成成功编译。

每窗只有固定 main9170、一个 candidate、Rust 三臂，CGU16/O2/jobs1 策略保持。当前 research pins 把 candidate 绑定为 `8ce584f81a405b8c5aa69e555b80ede276f07143`；stock pins 绑定 `aaa72a6b84c097844d7484280992a1c7e4696ab1`；两者 tool_source 均为本报告 ad814，baseline 均为 `9170bb4210554392e36eb7528596612596ebef80`。这些是已读 pin 文件，不是本审查新执行的 compiler 身份/运行收据。

二进制 hash、成功 build receipt/source/tree、实际 worker mapped LLVM DSO 均有准入检查；二进制/工具在结束前复核，LLVM DSO 在运行观测及最终 fresh rehash 核对。不得用尚未完成的源码/SDK恢复或未运行窗口提升 valid 状态。测量当时应称固定 main9170 对照；之后合 main 不得把旧对照重新标作新 main 性能。

### warm restart 和磁盘口径

warm-restart 先关闭两条旧 resident，再启动新 Worker/新进程；保留同一 block 的显式磁盘缓存和恢复后的源。新 worker 从自己的 ready/请求计数开始，启动时间单列、不进入 send-to-response 时长。Rust 每次 Cargo 都是新进程，因此这是 ZIL resident 重启后使用磁盘缓存的窗口，不是三臂具有相同驻留生命周期或包含全部启动成本的窗口。没有把普通 resident hit 直接改标签当 restart。

用户磁盘只扫描 `user/block-*/{baseline,candidate}/{semantic,objects,program.ll,program}` 和 `rust-target`；Rust 进一步分 incremental/deps/binary/target-other。源、compiler、LLVM DSO、SDK、runtime maps、fixtures、原生运行/nm日志及 archive blobs 不计入用户缓存。

disk_snapshot 采用 lstat、全 roots 共用 `(st_dev,st_ino)` 去重、`st_blocks*512`；目录/链接自身分配块计入，不遍历 symlink 指向的外部数据。别名的 path_logical_bytes/files 另外记录，不再加到唯一 inode 的逻辑/物理字节。共享 inode 记入先到的字典序 root/path category，所以单个 category 不是脱离其他 root 的独占磁盘占用；全局 total 才是该显式集合去重后的实际分配块和。

磁盘是 request 边界观测，peak 仅为边界 peak，绝非 transient build peak；扫描和归档在计时外，可能暖 OS cache，未宣称 OS 冷缓存。源码 hash/source snapshots、SDK 和 evidence 保留仍然必要，但不能拿它们计入一臂用户磁盘、另一臂却不计。

### 标签与规模

新 history 为连续 8/32 次 private edits 后接 unchanged-after-edits、positions、broad、unused-def error、recovery、restore、warm-restart，不是原固定 runner 的九格及其 parent preparations 协议。两 blocks 下 8 epochs 共 102 行、32 epochs 共 246 行；即使行数恰与历史窗口相同，也不是原窗口证据。

1000/2500 等是规模 preset，精确函数数量由 manifest 计算。例如默认 100 modules 下 large1000 有 8 helpers/module，总 named declarations=1005、definitions=1003；large2500 有 23 helpers/module，总 named declarations=2505、definitions=2503。所有这类窗口应称新生成大项目族。只有 `small425 + wide + branchy + cold` 标记原425冷源控制；它也不能使新的连续 history 等同原九格完整窗口。stock original425 window 独立记账。

## 已收到的轻验与运行边界

- 已读 `out/runs/large-project-disk-controls-01.json`：valid=true，五项控制包括跨 root hardlink 单计、path 别名分账、不追外部 symlink、missing root=0、目录/链接块计入；该收据的脚本 SHA 为 base 的 `164094…`。base→freeze 完整差异没有触碰 disk_snapshot，因此可按源码等价复用这五项控制，不要求 root 重跑。
- 已读 generator-02 manifest：generated_only=true、2500/simple/wide/8 epochs，脚本 SHA `9e7cf9…` 对应冻结文件。root 报告该 gen 与此前 1000/simple/diamond recipes/native oracle 生成通过，compiler processes=0。这证明生成/静态轻验范围，不证明大项目编译、机器运行、Verify 或性能通过。
- 本审查执行的只是源码/已有记录读取和字节摘要；没有调用 Git 命令、导入执行测量工具、构建 compiler、启动 resident、运行 native 或 benchmark。没有争用重构建锁。唯一写入为本报告。

**交回 root：两项 P1 关闭，没有新增确认 blocker；允许按已声明三臂和默认预算启动首轮。实际 run failure 仍须按原门保留并裁决，尚无速度、磁盘优势或产品父门通过结论。**
