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
