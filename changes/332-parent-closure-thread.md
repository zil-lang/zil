# 332 具体父闭包中的结构化线程

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
