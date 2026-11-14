# 原 short5000 capacity harness：轻量非作者只读复核

**结论：接受本次 P2 修复，原 provisional 叶 summary 伪成功问题已关闭；限定复核范围内无剩余确认修复项。最终 `diagnostic_success` 仍要求完整源、当前 Verify、report 实际16 units/jobs1及非零 native 证据。仅接受 harness 源码，不宣告实际容量运行通过。**

对象：[large-capacity-current.py](/workspace/zil-recovery/large-capacity-current.py)，初审版本 SHA256 `97f618be6697168141335a9e5282a8ec3f6dcd97d001e5c99303f6038de09b1a`；本次修复复核版本 SHA256 `2aacf81ee91aabcbc1e42e6fc942968b6022c589354898183aecc7d881686f9b`，26512 bytes / 465 lines。本会话不是作者，只读该脚本、直接复用的命令/运行 helper 和具名原输入/历史记录；没有执行 harness、Git、构建、编译器、原生程序或 benchmark。唯一写入为本报告，未改任何其他文件。

## P2 补审：provisional 叶记录伪成功已关闭

原版本在 compiler/Cargo 成功响应后、artifact/native 校验之前，把 `capacity_success=true, native=[]` 写入叶 summary；后续异常时可能残留该真值。最终汇总原本会拒绝，但单独读取叶记录可能误报。这一历史问题保留在上述初审身份下，不改写成原版本无缺陷。

本次只读复核 ZIL 与 Rust 的对应修改，确认：

- 初始叶记录均只用 `compiler_success` 描述编译响应，`capacity_success` 明确初始化为 false。保存原始响应后发生 units/artifact/native 异常，磁盘上的 provisional 记录仍是 false。
- O2/native 成功须显式满足 `len(native)==12` 且每条 valid。ZIL 和 Rust 两处都已修改，不能让空列表通过 `all([])`。
- ZIL 在后继 `verify_zil(project, expected)` 全102源哈希检查成功后，才赋最终 `capacity_success=bool(success)`、保存并 emit。check 模式也在这个全源检查后才记录最终成功。
- Rust 在后继完整 `.rs` source_metadata 等于 rust_meta 后，才赋最终 success、保存并 emit。源变化、artifact 失败或执行异常均不能提前留下容量成功。
- ZIL 的 current native_codegen requested_units=16、actual_units=16、jobs=1 检查仍位于 artifact/native 前，未削弱；当前 Verify=1、六输入×两轮和最终 candidate四行/24次 native 汇总门也保留。

因此原先给出的 units不符、缺 executable、native启动异常或后继源哈希变化情形，都不再产生真值的 provisional 叶记录。ZIL catch 仍 emit false 的 failure，Rust 异常仍进入顶层 false；实际 native 校验返回 false 的正常失败亦保持 false。

**本项接受、P2关闭。** 不要求额外运行 harness 或扩框架；仍按顶层完整 summary、rows、退出状态和原始 native 证据判定实际运行。其余源码绑定、SDK/工具链隔离、physical-CGU及非formal范围限制见原报告下文，均未因本修复升级。

## 原源完整性：已独立轻量核对

脚本固定 original fixture 为 `out/runs/large-project-initial-probe-02/short5000`，要求 100 modules、50 helpers/module、1 step；完整文件集合是 main.zil、io.zil、m0…m99.zil。

本审查实际只读重哈希了原 102 份输入，共 **843875 bytes**，与 fixture 全部 file_hashes 完全相同；汇总 identity 为：

`892fbc295a4cd51abec61c1cd600079126b1d473e1a85671f506353dfe333627`

具名原 fixture.json SHA256 为 `1c0d646573d16a65c96e80da7af0ce0ee6a49ca0e2b047229695e98155c07e03`。fn inventory 为 **5105 项，含两个 extern 声明**，不能描述成 5105 个定义都产生机器 body。

[main 的准入](/workspace/zil-recovery/large-capacity-current.py:368) 还重生成固定源，逐文件 hash 比对原输入；用原 rc.validate_equivalence 核 Rust/ZIL，并要求完整 equivalence 记录相等。真正送编译器的是从原目录复制、经完整哈希再核的源，不是删减后的重生成子集。没有 unused callee 裁闭包、错误注入或预算改写；原文件和工作副本在请求前后及结束时都复核。

## 成功协议与执行非零

- `pc.zil_command` 固定 check/build、JSON resident、独立 semantic cache；O2 明确 llvm-api/O2/CGU16/jobs1/object cache 和输出路径。只有 candidate 得到 push-reuse，candidate O2 加 native-link profile。没有 max-*、stop-after、Verify bypass 或编译器预算/deadline override。
- 每 arm/mode 使用新 work 目录和 resident，先 ready/protocol=1，再依次请求 1/cold、2/warm。同源、同进程和同缓存构成 warm；不是另一次冷编译或 warm restart。不同模式缓存相互隔离。
- 当前 Success 必须完整 102 路径集合、正确 request 序号、诊断空、verification_runs 为整数1，且 executed Verify event 恰有一次。缺 source inventory/计数/事件不能成功。错误响应保留完整 report/diagnostics，不冒称 Verify 已运行。
- root 新加的 [native 字段 require](/workspace/zil-recovery/large-capacity-current.py:225) 要求 report 实际 requested_units=16、actual_units=16、jobs=1；缺字段/不符均失败。它是实际报告检查，不是只把 argv 里的16复制进摘要。但没有读取16个物理对象，因此不能升级为独立 physical-CGU 验收。
- O2 成功还核 executable 报告路径恰为当前 work/program、存在且非 symlink，保留 ELF 命名 archive/hash/bytes，然后执行固定输入 `empty, 00, 31, 41, 80, ff` 各两轮。oracle 使用原 100×50×1 scalar 参数，逐项要求完整 exit/stdout/stderr 相等且没有 timeout；长度必须为12。
- [最终 candidate_ok](/workspace/zil-recovery/large-capacity-current.py:446) 要求 candidate check/O2×cold/warm 共4行全部 capacity_success，native 总数24；不是 `all([])`，也不能由单次成功凑足。Rust 要有历史 check 1行及本轮 O2 cold/warm 2行全部成功，且所有 arm 的 protocol_valid 均为 true。

main9170 的正常编译拒绝可以作为容量负例保留，不要求它成功才能判断 candidate 单独是否通过容量诊断；但它不会替 candidate 满足任何成功条件。基础设施/协议失败另记 false，不冒充正常 ResourceLimit。原 main 失败仍会留在 rows；没有正式 paired ratio、速度/内存优势裁决，formal_ratio 明确为 null。

## 资源与退出边界

默认 external timeout 为180秒，作用于 controller 等待 ready/response 或 Cargo 进程；native 每次10秒。这些是外部 watchdog，不是提高或修改 compiler 默认逻辑预算，也不能把 watchdog 超时当作编译器预算结论。

resident 使用独立 process group；close 尝试 quit、短时等待，最终清理该组及遗留 helper、回收进程、关闭流并保存 exit/buffer。`rc.run_process` 通过独立 process group、timer/kill/wait4 处理 native/Cargo 超时并保留日志。没有发现 timeout 后仍按成功执行 oracle 的路径。close/基础设施异常也会使顶层失败；本审查未运行这些路径。

所有输出根必须新建，拒绝已有目录或 symlink 根；不复用过去的 exe 来填 cold。代码保存原始响应、stderr/stdout、stdin、native result/oracle、失败记录。未采到的内存等观察不能据辅助 helper 中的默认值对外宣称优势；本 harness 的 SCOPE 本来就不允许这种性能结论。

## 身份与证据的有限保证

candidate `--source` 只检查完整小写40位 SHA，记录候选 binary hash，并明确写作 **root-supplied SHA，不是 build attestation**。它不自行证明该 binary 由该源码构建。root 必须使用已冻结且已有精确 build 收据的 binary；不能把此 binding.json 当作新的源码构建证明。

baseline 绑定 main9170 source、binary hash 和既有 build receipt 的 source；profile hash 与 helper checkout 的编译 policy 字节须一致，结束再核 profile/helper 字节。三份 helper 先复制入 implementation，再从复制件导入，不是运行中偷偷切换另一份 helper。

当前读取的 helper SHA256：

| 文件 | SHA256 |
| --- | --- |
| rust_compare.py | `75caebcec1ef29a0cdbdfd83d2ad623e22833458761f400d85722f70ce56febf` |
| push_counters.py | `b268f773eeba186f05d39ef26f026d5833a592614389d25989b5ab413eb491af` |
| native_workloads.py | `df6e95b48825063b718fd65a0c4ab8afaa940f6880a40215d60ac9105ced6964` |

该轻量脚本没有 formal runner 的完整实际 LLVM mapping/DSO rehash、工具链冻结 attestation、SDK 全归档或 full-ELF/物理对象交叉比较，也不清理并拒绝全部 ambient loader/Cargo 配置。Rust O2 有明确 manifest/command 和部分 env 去污染；不能据此宣称完整 formal toolchain 隔离。这些是已声明单机容量诊断的证据边界，不把本轮升级为 formal ratio，也不构成取消此前 full-ELF 门的许可。

Rust check 复用具名历史 short5000 收据，核完整源 hashes、相同命令、CGU16 manifest、成功退出/非 timeout、唯一成功 build-finished 及原 stdout/stderr hashes，再归档；标签是 historical-cold/reused_receipt。审查者已读该历史 result 并核两份日志 hash：returncode=0、timed_out=false、summary/result 相等。它不是本轮 timed check，更不是 Rust 执行了 ZIL 独立 Verify。Rust O2 cold/warm 则是本轮新项目和 target 下真实构建并运行。

## 交回 root

这份源码可按现有固定范围作容量诊断；以顶层 `complete=true && diagnostic_success=true`、对应 rows、退出0和完整 native 证据共同判定，本次 P2 已关闭，仍需保留最终收据资格检查。未提出新编译、补测试或扩大框架要求。

root 报告 30c 的 allworkspace 6122/0、f7 集中门进行中，仅作为协调背景，本审查没有重跑或独立验收这些门，也未把进行中的门填成通过。本 harness 自身是否实际成功仍需未来运行收据；当前没有编译/原生/容量通过的新增结论。
