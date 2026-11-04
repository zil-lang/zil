# 343 同步、返回登记与挂起终态提案

本提案先于接受性、ABI、清理顺序与费用变更。权威为主树
`_zil_design.md` 11、12、18 节及 `spec/grammar.ebnf`；339 的共享读取和
328 的 join 仍为 Direct，不作为异步证据。父 B0 与本单终态义务保持。

## 同步的第一可执行边界

增加密封 `MutexI32` 和 `AtomicI32` 载荷，不能从普通整数、地址或
`c_handle` 制造。首个显式作用域 `mutex_scope(initial, body)` /
`atomic_scope(initial, body)` 在稳定 helper 内保存实际单元直至 body 完成；
body 为一次调用的 `FnOnce(&Cell)->i32`，结果为 `(initialized,body_result)`。
单元共享地址可作为现有 `thread_scope` 的实际载荷；该入口仍在返回前 join，
不会把单元作用域与线程创建融合成只能固定使用的执行根。
这是有限真实同步并发边界，不宣称一般拥有 Mutex<T>、一般 Transfer/Share
或 Suspend 已完成。

专用 load/store/fetch_add 操作只接受对应密封单元的共享地址；不提供普通
`&i32` 或 `&mut i32` 逃逸。Mutex 的每次操作使用真实 native mutex
acquire/release；Atomic 使用真实 LLVM 原子操作，首片顺序固定 seq_cst，
后续显式 ordering 不由默认普通 load/store 冒充。fetch_add 统一按 i32
位模式模 2^32 运算并返回旧值；Mutex 内部同样不触发 checked 算术展开。
Mutex init 失败不调用 body，清理 body 一次并返回 `(false,0)`；Atomic
作用域建立后返回 `(true,result)`。native lock/unlock/destroy 失败固定 abort；
当前源 panic 同样固定 abort，无 poison 恢复或在未退锁时展开的承诺。每个
标量 Mutex 操作持锁期间不调用用户代码，不引入隐式全局锁。普通非原子 payload
不因包围 Atomic 而获 Share。两核分别重建实际参数、producer、调用图、
返回、存储及 cleanup 依赖，源注解和同名函数不作为权限。

最近反例包含：普通 `&i32` 冒充单元、普通解引用或投影单元、传回单元
地址、worker 外部调用、把非原子包围字段并发写、未使用定义中的不合法
操作、伪造 raw Wire 的操作数类型与重复/缺失结果。

## 返回后的外部登记

采用显式可失败稳定分配，真实拥有具体 F 的密封登记资源；返回的移动头
不移动公布的 context。注册前初始化 F 与全部管理字段。分配/初始化/
安装失败分别只清理已建立部分，F 恰好 drop 一次。不能让现有
WithForeignCallback 的栈 userdata 逃出 helper。

callback/waker 的准入和用户登记必须在同一锁或等价原子协议中线性化。
close 顺序固定为 Closing（拒绝后来入口）、后端 no-future、排空已准入
callback/waker、处理结果责任、销毁 F/context。active=0 和 cancelRequested
单独都不表示静止。raw userdata 记录在后端不再访问前保持；晚通知不能
先解引用已释放 frame 才判断过期。close 失败不得提前 free；固定 abort
或继续保留的实际策略须与平台合同一致。

## 真正的 Suspend / SABI

五入口为 init/poll/request_cancel/poll_close/finish_destroy；它们必须来自
源 Suspend CFG 降低为持久 pc、spill、清理责任和结果槽，不能仅给模型
方法取五个名字。poll/poll_close 对同帧互斥；Ready 只移动结果一次，后续
观察完成标签不能再取得结果。取消也走 Closing/no-future/drain；结果槽、
帧借用、已保留 waker 和注册根都进入销毁条件。Direct 无执行根入口不得
调用 Suspend，也不暗中阻塞。

## 交付与费用

新增遍历在队列、集合或类型复制增长前从当前累计预算预付；不提高默认
限额。实际 primitive 类型、操作、调用模式、cleanup/SABI/frame ABI 必须
进入 Wire、指纹、CGU、对象缓存与两后端身份。源码正控、最近反例、
独立 raw/Wire、真实争用、失败注入、晚通知和取消排空分别记录，模型/
后端机器夹具明确标注级别，不代替源码到机器闭环。

## 首个同步里程碑的实际实现

密封作用域与八个操作已连通真实源码、两核、Wire 与两后端；原有
`thread_scope` 接受精确共享单元地址，并分别重建 worker 的实际可达图。
普通 `&i32`、错误 cell kind、非原子读取、改型重借、错误 operand 和
ordering 都不取得同步权限。scope 的实际 closure 参数保持逐调用来源；
kind 和 ClosureId 不是储存来源或唯一性证明。

四个原源正控覆盖两个局部作用域与两个 parent/child 各 25000 次更新的
真实争用，在 LLVM API/Clang、O0/O2 共 16 个配置中运行到零退出码。
独立 raw/Wire 的六个变造同时拒绝；新路径沿现有累计余额核验。
另一个拥有 body 正控在四个编译配置中观察 mutex init/lock/unlock/destroy
及资源 Drop：init 失败仅有一次 body 环境清理且不执行 body；后三种
native 操作失败固定 SIGABRT，无提前销毁成功声明。

这是提交供独立复核的同步里程碑。一般 Mutex<T>/mutex_with、一般
Transfer/Share、拥有单元构造、返回登记、Suspend/SABI、取消排空和晚通知
仍属于本单持续施工；不能用这个 Direct join 结果替代它们。

| Rust 四目标 | 当前实证与限制 |
| --- | --- |
| 同类安全 | 密封同步访问和实际共享图由两核、raw/Wire 重建；尚无一般异步安全结论。 |
| 运行与内存 | 实际 pthread mutex 和 LLVM atomics 争用及失败运行；无隐式堆盒、RC 或全局锁；Rust 同语义运行/RSS 未测。 |
| 表达与维护 | 普通闭包作用域可组合既有线程根；完整拥有泛型 Mutex 与异步 API 尚未完成，维护成本未测。 |
| 编译成本 | 新路径累计 N/N−1/N 边界，不提高默认限额；Rust 编译成本比较未测。 |
| 至少一项突破 | 原本没有的受保护并发更新已由原源到实际机器运行；相对 Rust 的可重复产品净突破及四目标合取未建立。 |

本提案及里程碑不变更父 B0、完整语言/ZUI 和既有 product-gates 状态。

## Scalar cooperative SABI construction contract

`task_start(worker, i32)` explicitly allocates one stable frame and returns a
nullable `Managed(TaskI32)` head. `task_valid` observes allocation success through
its descriptor. Allocation failure creates no frame and the null head drops as a
no-op. The independently reconstructed initial worker profile contains only
scalar storage, scalar operations and `TaskYield`; it admits no foreign code,
callbacks, wakers, addresses or owned cleanup. This actual closed profile, rather
than a producer flag, alone permits immediate close after a poll activation exits.

`task_poll(&mut task)` returns `(status, value)`: Pending=0, Ready=1,
Complete=2, Cancelled=3 and Invalid=4. Only Ready carries the worker result and it
occurs once. The mutable borrow is of the actual owning head; generic Managed
pointees acquire no address permission. `task_request_cancel` is sticky and
idempotent. `task_poll_close` returns true only after the activation has exited;
`task_finish_destroy(give task)` consumes a closed owner. Default Drop requests
cancel, closes and destroys only this checked scalar task kind. Future tasks with
external users cannot inherit its close proof.

Each worker has distinct init/poll/request_cancel/poll_close/finish_destroy
symbols. Source yields become saved instruction positions; actual scalar locals
and SSA values live in the persistent frame. Actual reachable yields determine
Suspend transitively. Ordinary calls, function references, thread workers and
exported Direct entry signatures cannot grant execution of a Suspend function.
This bounded frame is a continuation milestone, not completion of retained
registration, general owned spills, wakers or no-future/drain obligations.

The scalar machine candidate now covers conditional yields, backedges and
locals/SSA live across several polls on both native backends at O0 and O2.
Cancellation is exercised before the first poll, after each of two yields and
after completion. A moved owning head can pass through an ordinary factory and
relay and be polled through an actual checked `&mut TaskI32` helper parameter.
An unused yielding definition still receives all required runtime declarations.

The init boundary is deliberately `noinline`: even a source-visible
`task_valid` observation was insufficient to preserve the explicitly fallible
allocation under O2 inlining. The same default-Drop source now produces AVF on
success and AN on injected allocation failure (A=allocation attempt,
V/N=source observation, F=free). This preserves the frozen fallibility contract
with a real ABI call; its runtime/code-size cost has not been compared to Rust.
Explicit destroy before Closed traps without freeing. Null operations have
defined results and null Drop is a no-op.

The first native task lane is Linux x64. The five worker entry symbols are
currently internal implementation boundaries; semantic public SABI interfaces
and physical export artifacts are the next milestone. No frame-layout assertion
is accepted from Types. The actual closed worker graph excludes foreign calls,
callbacks, addresses, owned cleanup and wakers; this is the sole reason its close
does not need an external no-future handshake. General task cleanup cannot reuse
this shortcut.

New task lowering shares the existing request planning balance across parallel
function batches. Kernel loan checks use actual owning-head ancestry and
liveness. Exact task fixture limits are Analyze 26182 and Verify 142578, with
N/N−1/fresh-N checked and no default limit increase. The sync regression bounds
remain 5152/11506 on their original fixture. Broad all-target lint remains blocked
by the old shared cp34 reference encoder's missing new schema variants; changed
libraries and the changed test target pass lint.

| Rust four goals | Scalar task evidence and limits |
| --- | --- |
| Comparable safety | Actual head loans and closed worker graphs are checked independently; malformed raw and decoded Wire candidates reject. General retained/external task safety remains unfinished. |
| Runtime and memory | Persistent native frames, one-shot results, cancellation and real allocation/free failures execute on two backends. Explicit allocation and init call are present; matched Rust runtime/RSS costs are unmeasured. |
| Expression and maintenance | Ordinary source branches, loops, owner moves and mutable helper parameters resume after poll returns. General owned spills, public SABI, registration and wakers remain under construction. |
| Compile cost | New work consumes the existing cumulative balance, including parallel lowering. Matched Rust compile-cost comparison is unmeasured. |
| At least one advance | Source suspension now produces a real persistent native continuation; no product-level four-goal or net-performance conclusion follows. |

Independent review of the first scalar candidate found a missing Direct boundary:
`c_callback(yielding_worker)` could still create `ForeignFunctionRef`. Both
kernels now reject that target using their independently derived Suspend graph.
The retained original source and a tenth raw/decoded-Wire mutation exercise this
case. Its ordinary non-Suspend static callback neighbor is accepted and builds
on both backends at O0/O2. The first candidate is superseded by this correction;
its passing tests alone did not establish that boundary.

## Public scalar task SABI continuation

The public semantic `TaskInterface` is separate from `ScalarInterface`: it names
the actual worker, source name, parameter/result type and mode, and ABI version.
It contains no frame layout or no-external-users permission. Types routes a
yielding public definition to this interface; Analyze and Verify independently
rebuild its closed scalar worker profile. A forged scalar export cannot grant a
Direct entry to the same worker. Only independently checked semantic exports may
request external linkage for the five generated SABI entry points.

The backend publishes a separate physical artifact derived from the actual
lowered frame and five machine functions, bound to current verified input,
target and codegen identity. External poll uses a packed i64 (high32=status,
low32=result bits), with explicit wrappers around the internal tuple-returning
poll. This avoids assuming that an LLVM aggregate return is the C aggregate ABI.
The raw external caller must own the frame, serialize activation operations,
close before destroy, and honor the one-shot Ready transition.

The public checkpoint executes a genuine external C caller against all five
entries on LLVM API and Clang at O0/O2, including cancellation at each suspension,
one-shot signed results, null allocation and exactly-once free. C implements only
the caller and allocator observation. Its `sizeof`, `_Alignof` and `offsetof`
probe agrees with the artifact's actual lowered fields, and both native tools
report the frozen Linux x64 DataLayout. Unsupported target/type layouts reject
before an artifact is produced; Rust host layout is never used.

Seven independent public metadata mutations reject in Analyze and Verify,
including a forged ScalarInterface. The original module and decoded Wire module
are each checked with the raw semantic export candidate. Explicitly selecting a
yielding worker as an executable Direct entry also rejects. Public metadata has
its own paid N/N−1/fresh-N boundaries (Analyze 1955, Verify 1414). The semantic
cache stores candidates and re-verifies task exports after cold/warm replay;
LLVM object cache hits preserve the actual ABI artifact, while changed checked
arithmetic changes both frame and codegen identities.

This checkpoint completes the bounded public scalar SABI lane. General owned
spills, retained registration, callback/waker admission, no-future/drain, generic
Mutex<T>/Transfer/Share and native Windows remain task343 continuation work.
Matched Rust runtime/RSS/compile-cost and maintenance comparisons remain unmeasured.

Independent review of the public checkpoint found that instruction-count CGU
costs did not prepay the subsequent recursive machine hash's strings, nested
types and global initializer bytes. Codegen identity now reserves every actual
machine node and variable-length byte sequence before hashing a unit. The walk
stops on the shared remaining balance and bounds recursive type/value depth.
A retained adversarial machine neighbor combines a 65536-byte initializer,
nested aggregate/array signature types and long type/global/function/operand
names; its N/N−1/fresh-N boundary is 257884. This is machine-level accounting
evidence, separate from the source-to-native SABI tests.

## 集成 Float×Task 的固定原子身份计费

Task machine identity walker 在与 Float 实现合并时显式处理 F32/F64 类型及原始位常量；这些固定大小原子仍由每节点16费用覆盖，不增加默认额度，不跳过Hash前预付。原字节/类型/长名累积反例及N−1门保持。
## Returned same-thread Registration construction

The first retained route uses the existing explicit unsafe Register/Close roles:
register-null means every raw entry/access has ended and none can begin; strong
Close returns only after the same condition. A site-specific stable allocation
stores the actual concrete F, backend handle, close-code pointer and one atomic
Busy/Closing word. FnMut admission owns Busy only on old=0; rejected old=2 owns
and clears only the Busy it just set, while old=1/3 never clears another entry.
Closing is monotone. The word checks F exclusion and reentry; it never proves
that pre-atomic/rejected raw entries have ended. Only the actual strong backend
return permits environment destruction and free. This route is same-thread and
non-Transfer; asynchronous/no-future-only backends cannot reuse the shortcut.

`Registration<F>` retains the actual F type under the ordinary Managed/Desc
ownership category. `c_register` consumes F using ordinary callable rules;
allocation and null-install failures each destroy that actual environment once
and return a defined null owner. `registration_valid` reads the nullable head
through its actual descriptor, and explicit/default close use the same consuming
strong-close path. The returned helper source must survive stack clobbering and
then accept callbacks. Raw descriptors do not expose frame bytes.

Unused generic installation definitions explicitly require
`F: FnMut(i32)->i32 + ClosedEnvironment`. The bound is a raw compiler-sealed
requirement; Types and both kernels separately walk actual capture/ownership
graphs. Captured loan/data addresses or descriptors reject, including inside
owned containers; code pointer signature addresses are not stored capture data.
The owning registration factory is derived from actual Managed Registration
return, matching concrete F input and create/give responsibility. It does not
reuse a synchronous finite callback trace for callbacks occurring after return.
All new inventories and frame/helper construction debit the existing remaining
request budget; fixed helper reservations include actual nested capture work.

The retained source candidate now returns the actual owning carrier through a
parameterized helper and an imported public source-library helper. Native tests
on both backends at O0/O2 observe an owning Mark environment and a mutable
counter across installation, helper return, stack clobber and later callbacks.
A second source keeps two distinct mutable environments live at the same time.
Explicit and default close each destroy the actual environment once. Allocation
failure and null installation after synchronous callbacks are executed with the
same source; no failure path treats a non-null invalid owner as a result.

The native source configurations contain 16 success and 16 injected failure
executions. Fourteen raw mutations reject independently in Analyze and Verify,
and the same raw modules reject after Wire decoding. Controls include real
borrowed-factory laundering and reusing an already consumed owning F while
closing the first registration to avoid an unrelated leaked-head failure. The
exact source kernel boundaries are 15202/26326. The backend planning fixture has
N=158890, with separate long-symbol, repeated-child-DAG and dynamic-heap-head
neighbors. Inline layout has an active-path guard and a native depth bound of
128; dynamic owning heads are leaves for physical alignment, while environment
qualification separately follows their stored actual types.

| Rust four goals | Returned registration evidence and limits |
| --- | --- |
| Comparable safety | Actual F/owner moves, recursive closed captures and unsafe roles are rebuilt independently. Strong raw-entry quiescence is an explicit external backend contract; no general asynchronous close claim follows. |
| Runtime and memory | One explicit fallible stable allocation retains actual F without RC or implicit global locks. Two native backends exercise once-only cleanup and mutable persistence; matched Rust runtime/RSS cost remains unmeasured. |
| Expression and maintenance | Ordinary F-bound helpers return Registration<F>, source-library calls preserve the environment, and default cleanup composes with ordinary owners. General cross-thread registration/wakers and general owned Suspend frames remain under construction. |
| Compile cost | New traversals, actual symbol bytes and expanded physical layout occurrences use the current cumulative balance. No default limits increase; matched Rust compile-cost comparison remains unmeasured. |
| At least one advance | Actual source can retain its concrete closure after an installing helper returns; a product-level net advance and the four-goal conjunction have not been established. |

This is a bounded same-thread StrongClose milestone, not task343 completion.
Owned Suspend child frames, PendingExit responsibility, stable waker roots,
asynchronous no-future/drain, general Mutex<T>/Guard/Transfer/Share and native
Windows remain continuation work under the parent task.

Integration with the current integer/float and borrowed-code-pointer types keeps
ClosedEnvironment structural: stored u64/usize/f32/f64 and static borrowed
function pointers contain no external data loan. A pointer's invocation signature
is still validated by the ordinary signature/lifetime walkers; captured actual
Address/Desc/Slice positions still reject. The split recursive Types dispatch is
preserved. The combined generic_regressions target passes all 52 tests, including
registration native success/failure/close paths and existing Task/synchronization.
This is limited integration validation, not terminal or full-workspace acceptance.
