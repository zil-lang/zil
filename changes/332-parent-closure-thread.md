# 332 具体父闭包中的结构化线程

## 独审后累计预算修补边界（root，2026-11-03）

`cb17b74ab` 的新增捕获查询只扣各闭包独立 Graph 的余额，未进入
State 的请求累计余额；Analyze/Verify 的边界通过不能替代 Types 收费。
本次授权修补仅覆盖新增 thread_scope 推断分支：查询、实际签名重建、
名字/捕获扫描及本分支新增的直接 Graph 操作必须预付或以实际收据汇入
请求 work_left，嵌套和多个父闭包共享同一剩余额度，失败不重置。
不为保持旧 N 免收费，不提高默认限额，不改既有非线程 Graph 全局收费
策略；通用 expr 递归中原有 Graph 算法不在本片统一迁移，历史债务须单列。
实际签名含 pending body 所属独立 Graph，不能错误记到调用者 Graph。
租约不得在失败时重置余额或给嵌套操作重复授额。限定控制须包括 Types 的长名字、
多个/嵌套闭包累计 N/N−1/fresh，另保留两核及原生清理回归。
这执行既有累计预算合同，不新增接受权限、公共字段或机器 ABI。

基线 `40fbcca6ef506f3cb8e1d40a4f8995c788627da0`。先冻结接受性合同，
不改变325/328的pthread ABI、跨线程载荷、失败收口或worker资格。

现有三个门分别为closure预捕获Checker的thread_scope Unsupported、Builder的
environment/helper guard，以及Analyze/Verify的caller environment guard。
旧pending父闭包源在Types门被拒；另一个callable wrapper在Expand被拒。
本片只晋升前者，原文件路径及字节保留；wrapper继续Unsupported。

## 拟开放的有限入口

在既有合法具体Closure入口（Fn/FnMut/FnOnce）中允许thread_scope。这里只沿
已有具体closure的参数、结果、捕获及实际调用资格，不新增capture形状、嵌套
地址、借入返回/逃逸、泛型callable参数或helper角色合同。源码推断须识别真实
静态worker、i32载荷和具体零参数body，并按源序准备，不能把thread_scope当
普通未知函数或把parent直接标成FnOnce来规避责任推导。

两核从caller.environment恢复实际ClosureDefinition，核真实id/function绑定、
非foreign入口、参数LocalId/完整Type顺序、实际结果与模式，结合各自已有的
capture/借用/作用域/清理核验；仅有Some(id)不能制造入口资格。caller带未核
callable参数/helper合同仍Unsupported。错误实际绑定属于raw伪造，应拒绝。

父body依旧是一次消费的具体()->i32环境，只在发起thread_scope的当前线程执行。
父Fn/FnMut是否可重复、FnOnce是否已消费由实际capture用途及调用检查决定：
取走父环境资源给body须有真实消费权限；共享/独占父环境不能被借此复制或夺走。
父body合法借用沿现有来源和窗口保留到返回，重叠写、资源重复取走、Once复用仍拒。
创建失败不调用body，但其已准备环境仍准确Drop；成功消费body并完成清理，join
成功证明静止后才返回父closure。父closure及其环境在这段Direct调用期间保持活。

## 嵌套与明确保留的边界

父线程的具体body若再启动结构化scope，两个独立栈context分别等待各自join，
这不是把worker资格放宽为可spawn。每个worker及其静态可达子图仍严格纯标量，
排除closure/environment、资源、地址、FFI、未知indirect及thread_scope；两个
Kernel分别重建。worker仅收到i32，父closure环境/拥有body不迁移给child。
join失败仍abort，不允许从父closure退出并留下存活child的context。

不开放callable参数/helper合同入口、跨线程借入/共享payload、脱离handle、
Suspend/SABI、取消、Mutex/Atomic或一般Transfer/Share。

## 公共表示假设与实际验收

预期可复用已有caller.environment、ClosureDefinition、CallClosure族和
WithScopedThread{worker,argument,body}。节点已保留真实body，旧资源/借用和
backend使用具体FunctionId生成各entry/helper；没有理由先加权限字段或新ABI。
这只是待验证假设：须实际源码→TypedIR/Wire→独立Verify→两后端O0/O2和多实例
运行核回；发现缺表示先报root，不在私有实现暗加语义。

写集按主工单：Types closures.rs/scoped_thread.rs、Analyze/Verify scoped_thread.rs、
已有generic_regressions、concurrency325源码和本说明。规范由root冻结；不改
生产backend或公共schema，不新增Cargo目标。至少核原pending原字节、三种实际
parent kind、真实父body资源/借用、创建失败Drop、nested roots、Once复用和
worker子图不能进入线程的近邻；新增工作模块累计预付并核N/N−1/fresh。

限定结果和精确提交将在实际完成后补记，当前提案不计实现通过。

## 实施与限定结果

公共合同冻结于root `e28e08d8825085e9a3c6bae57d2a85d3a2418481`
（主树§11.1、changes/329-333-public-profiles.md；施工读取主树权威）。
生产checkpoint `514b4634c`，消费者checkpoint `72333cf76`。本片没有新增
IR、Wire字段/schema版本、线程机器ABI或backend修改；schema34的同一个
WithScopedThread节点由既有Lower及Clang/LLVM API真实消费。

InferBuilder核实际静态worker及payload/body签名；旧Copy推断器的私有函数
签名只用于推断，Builder仍要求真实Closure，因此普通thin函数指针不能成为
body。转交捕获body明确moved+consumed，父入口能力由真实用途推导，不强制
把全部parent标成FnOnce。Builder从pending恢复实际入口，Analyze与Verify各自
核ClosureDefinition的id/function、environment、参数LocalId/完整Type、返回
类型和result PassingMode；没有把worker图的environment/资源/thread禁区放开。
各自既有capture/借用/清理核继续独立检查，不复用对方证明。

原pending/parent_closure.zil路径与字节和40fbcca6基线相同，现为成功正控；
pending/callable_wrapper.zil仍在Expand明确E-UNSUPPORTED。新增正控涵盖真实
Fn/FnMut/FnOnce、带i32及共享地址参数的parent、两次合法Fn/FnMut调用、父body
合法借用、两个嵌套scope、实际拥有body转交。最近反例固定阶段/代码：

- Once父闭包复用：Analyze E-UNINITIALIZED；
- 父body借用仍活时写参数：Analyze E-BORROW-CONFLICT；移除唯一写入即成功；
- worker可达子图含thread_scope：Analyze E-THREAD-WORKER；
- thin函数指针冒充body：Types E-THREAD-BODY；
- 父闭包内CT thread_scope：Ct E-CT-FORBIDDEN，未执行线程。

实际源码→TypedIR→Wire34 roundtrip→独立Verify→Lower通过。raw控制不依赖
Types许可：旧普通函数转实际无capture父闭包成功；坏env索引、坏function绑定、
错结果、错参数数及错结果模式分别由两核拒绝；把真实消费父环境伪成共享Fn也
由两核拒绝。原worker越界/teardown/重复body负控保留。

所有命令source /workspace/.zil-env/env.sh，CARGO_TARGET_DIR为target332，
CARGO_BUILD_JOBS=1，显式启用llvm-api。限定命令：

```sh
cargo test -p zil-driver --features llvm-api --test generic_regressions -- --nocapture
cargo clippy -p zil-types -p zil-analyze -p zil-verify -p zil-driver --features llvm-api --all-targets -- -D warnings
cargo fmt --all -- --check
```

本轮既有generic_regressions目标23/0、零忽略/过滤；Clippy和fmt通过。没有跑
workspace全量。两核在同一请求累计新parent工作，实际三kind源码N/N−1/fresh：
Analyze N=26216，Verify N=92685；旧325控制N=6873/10587不变。新增捕获名查找
按候选数及比较长度预付，入口完整类型比较也在执行前预付；默认限额未提高。

本片新增52个独立机器场景，重复重放不累计：

- owned父闭包：两backend×O0/O2×成功/业务错误/create失败/join失败=16；
- kinds、nested、borrow、原pending四个源码控制×两backend×O0/O2=16；
- 两scope的成功、内层create失败、外层create失败、内层join失败、外层join
  失败×两backend×O0/O2=20。

每次构建断言actual backend及无fallback；LLVM API核builder输入及前后verify，
owned API O2实际多CGU。单文件nested明确units1。机器oracle核精确stdout、退出
码或SIGABRT、父线程body/Drop、真实异线程production start、内层先join、失败
不调用body/不join未启动handle及准确环境Drop。嵌套观察器延迟外层child进入
production start，直到内层scope结果/Drop/join后才放行，核两context地址不同并
保留外层context到最终join；不以worker先完成掩盖栈活性问题。观察C仅包装真实
pthread入口，不提供语言线程实现或同步fallback。既有325/317等消费者在23/0
目标内重放，不并入这52个新增场景。

四目标反思：有限父环境消费/借用及worker资格有独立安全核和机器实证；表示
沿用稳定栈和现有清理，无新增隐式Box/RC/锁，未测Rust运行/RSS对照；原合法
pending与实际Fn/FnMut/FnOnce/nested组合得到表达增量，未测完整框架维护收益；
新增检查有累计预算和精确耗尽边界，未证明Rust编译性能优势。有限突破是父
闭包可拥有一次body并进入真实结构化线程，包括嵌套失败收口；完整语言四目标
合取、一般Transfer/Share、callable helper、async/SABI及Mutex仍未完成。非作者
审核、root整合与正式全量待执行，不据本地限定结果宣称main/CI晋升。
