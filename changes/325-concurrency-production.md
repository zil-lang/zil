# 325 Direct 结构化 OS 线程首片

基线 `fe4cb19ae9bf82cedc1c80dd7677808aa46b4072`。按规范 §11/12/13，
本片新增实际线程创建、并行执行、必 join 及一次结果，不实现完整 Suspend/SABI、
Mutex 或取消。现有 `WithForeignCallback` 仅同线程 Direct；其普通 Closing/active
字段没有跨线程资格，本片不放宽或复用其外部角色合同。

## root 已批准的源入口和公共接点

```zil
thread_scope(worker, payload, body) // -> (bool, i32, i32)
```

裸名保留且不可遮蔽。worker 是解析到实际静态普通定义的名字，不接受任意
FunctionPointer、foreign、closure、teardown 或未实例化模板。精确签名
`fn(i32)->i32`。payload 为 i32；body 是实际具体零参数、可一次消费的
`FnOnce()->i32` 环境。先解析 worker，再按源序求 payload、body，各一次。
结果依次为 created、worker_result、body_result；不公开 thread handle。

root 持公共表示及 schema：

```rust
InstructionKind::WithScopedThread {
    worker: FunctionId,
    argument: ValueId,
    body: ValueId,
}
```

TypedIR/LIR 保留同一节点及实际操作数。结果严格 `Tuple[Bool,I32,I32]`。
建议新 wire opcode 95（94 已属 WithForeignCallback），最终值/公共版本由 root
统一冻结；不增加虚假的 Transfer/Share bool 或普通同名 record 权限。

## 真实迁移资格、存储与收口

首片仅明确准入 x86_64 Linux 的受支持 pthread ABI，其他目标 Unsupported。
payload 的 i32 是完整、无地址和清理依赖的真实值。worker 的完整 transitive
普通调用图由 Types/Analyze 和独立 Verify 检查：纯标量参数、局部与结果；
不允许 FFI、TLS、分配、资源/地址/闭包、未知 indirect、同线程 callback 或
嵌套 thread_scope。函数名、签名、普通 Copy 标签或空 impl 都不是线程资格证明。
所有实际可达静态被调体都检查，不能只检查直接 worker 或省略 teardown/driver
依赖。新遍历/实例/集合增长前使用当前请求余额预付，默认限额不增加。

生产 lowering 生成独立 helper 和 pthread start entry，真实调用
pthread_create/pthread_join。helper 栈上 context 含完整 payload、worker result
和 thread handle；不把 context/userdata 发布成用户值，helper 在 join 前不退出、
不移动存储。create 前初始化的数据经真实线程创建同步交给 child；child 对结果
的写入经成功 pthread_join 的 happens-before 交给父。结果不能在 join 前读取。
不靠普通 done 位、SC 交错模拟或同线程调用声称完成同步。

create 失败返回 `(false,0,0)`，不运行 worker 或 body，不做同步 fallback；
body 尚未消费的真实拥有环境在父线程清理。成功时 worker 一次、body 一次，
body 的完整清理在父线程完成，然后必 join，再发布两个 i32 结果。
join 失败不能证明 no-future，固定 abort，禁止返回并释放 context。
本 Direct profile 的 trap/panic 固定 abort，不声称展开/取消清理。

body 及其 ByteBuffer/资源等责任从不迁移到 child。仅同步调用中保活的父对象
不是子线程载荷；本片不凭父 body 的资源赋予 worker 的 Transfer 资格。
无捕获/借入/共享子载荷、脱离任务、可返回登记、线程杀死、Mutex/Atomic、
Transfer/Share 一般 trait、Suspend 帧/执行根、取消或晚通知入口。

## 写集与并行接口

- root：`zil-base` schema，`zil-ir` lib/wire/fingerprint，规范/文法/诊断/接口附件；
  和322公共版本合批，未冻结前不在325树修改这些公共文件。
- Expand：保留名与 `generic_check` 中对本片未开放 wrapper 的明确拒绝；不能
  用串行 callable trace 代表同时运行的 worker/body。
- Types：`src/scoped_thread.rs`，以及 call/保留名分派；实际 worker/参数/body 检查。
- Analyze：`src/scoped_thread.rs`，必要 instruction uses、body消费/作用域/责任分派；
  从实际 module 重建完整 worker 调用图。
- Verify：独立 `src/scoped_thread.rs`，实际类型/CFG/worker 图及 body 一次责任；
  不调用 Types/Analyze 的求证函数。
- Lower/backend：准入分派、`emit/scoped_thread.rs`、必要 machine CGU 依赖及目标链接；
  稳定 context、真实 pthread、失败与清理都在生产链中，不以测试 C wrapper 代替。
- CT 消费者显式拒绝 thread_scope/该运行时节点；如需越过已派 driver 写集由 root 协调。
- 复用 `crates/zil-driver/tests/generic_regressions.rs`，源码置
  `examples/mainline/concurrency325`；不新增 Cargo integration target 或 runtime crate。

公共三字段节点冻结后，Types/Analyze、独立 Verify、backend/OS 适配可并行施工。
当前槽满，325可先实施前端/Types与Analyze专用模块；最有价值的后续分派是独立
Verify/raw-Wire核验，另一个可拆点是 backend pthread adapter。最终非作者审核
与实施身份分开，不把作者自测当独审。

## 必要限定验收

真实 ZIL worker 计算与父 body 拥有资源构成消费者；成功/创建失败及真实清理，
两后端 O0/O2，API builder 报告与实际 backend 核对。测试观察者可包装 pthread
核对确为不同 OS 线程、create/join 次数、join 前后与一次结果，并注入 create
失败；该观察者不承载 ZIL worker 计算或生产线程安全资格。join 失败以 abort
核对，不能期望释放仍可能被访问的栈 context。

最近拒绝包括错误静态worker身份/签名、worker间接或传递FFI/资源/地址依赖、
伪造普通Transfer/Share impl、错误body、未开放wrapper/嵌套线程、CT执行；
raw/Wire须覆盖实际worker替换及body重复消费，保持阶段/诊断归因。新预算测
N/N−1/fresh并留旧控制。受测固定源码、实际命令、阶段IR/机器结果之后补记，
不把本提案或普通同名函数当作实现完成。
