# 187：局部同步 ByteBuffer 描述捕获

## 身份与裁决

作者 `/root/restore177_inventory`；源码冻结 `472b3ce052b850ef51fbfa3f8d818c51e080a112`，基线 `ebaddd7faf7b83b257b873e8544af0fef13e670a`。规则1.4、schema23，设计摘要 `8df5f5bb3cb25fdea8670124b09941713529c859ab18864d0fc87090864786d5`，规范摘要 `1068cda3fe79b906f49eeee4bd3f62863f9db4ae85e8a0dbaa3c439d310777b8`；七个规范输入逐一重新计算相符。

本收据是作者限定验收Pass，独立生产审核、主线合入与正式整仓验收分别由root记录。没有把此处393个测试或机器执行次数称为全语言完成。当前分支尚未合入184/188/189的新公共接口，所有身份与机器结果绑定本片schema23源码。

## 实现与真实漏拒修复

局部同步、不逃逸的Desc<ByteBuffer>可以进入闭包，实际权限从每一捕获及执行的操作重建。Types、Analyze预检、Verify、Lower、LLVM的旧形状门精确开放该项；LLVM环境字段调用既有ptr表示和8字节对齐。Desc不拥有Buffer、不延长其生命，也不成为公开强Hold。Slice捕获、显式Address callback参数、借入工厂逃逸和普通R描述额外helper参数仍明确拒绝。

源码纵切暴露了真正的安全缺口：闭包内部从read形成view，再经edit reserve；read/edit实际指向同一Buffer时，旧私有Apart仅记录普通参数对，错误接受。该候选只执行check，未运行不安全机器。当前修复保留Parameter、Capture和ParameterCapture三类真实外部来源，把每条实际普通/闭包调用边的分离义务逐槽代换；同源拒绝，异源可执行。拥有捕获保持内部责任，环境位置与外部捕获目标分开。分支、有限前缀、两层普通helper及直接/借入环境调用均进入有界独立重建。

helper的实际CFG效果同时包括普通显式参数和实际被调用的捕获；看到callable_contract后只导入callback而丢掉helper本身写入的捷径已移除。额外Desc<ByteBuffer>参数在Types、Analyze、Verify三处同形准入，源反例和raw/Wire都验证捕获与本次参数之间的别名。公有Hold/Call/writes含义和默认预算没有改变，也未调用Analyze替Verify作证。

## 精确验收

| 范围 | 实际结果 |
|---|---|
| 冻结driver限定9组 | 130通过、0失败、0忽略 |
| Analyze库 | 23通过、0失败、0忽略 |
| Verify库 | 240通过、0失败、0忽略 |
| 上述合计 | 393通过、0失败、0忽略 |
| 新187自身 | 23组；68次真实native，O0/O2各34 |
| 源拒绝 | 29项Verify借用冲突、2项Analyze描述生命周期、4项Types明确未实现 |
| 独立raw/Wire | 9对、18次精确拒绝；修改候选后不重跑Analyze |
| Verify实际预算边界 | raw与Wire均11776成功、11775 ResourceLimit |
| fmt与Clippy | 最终冻结源码均通过，Clippy拒绝警告 |

driver邻接含原094、096、099 native、065、168、182与借入闭包；root的094迁移提交20dcec0f只把原字节相同的未调用局部描述捕获移为Success，其余负例未动。094本轮12项通过。099含184的源预期迁移由root在合流时处理，本分支没有编辑该共享文件。

## 四模块机器与失败责任

`examples/mainline/borrowed_bytes187/{main,model,operations,visits}.zil`组成真实程序。运行时输入决定零到三次callback调用；callback只读source，source共享view在调用期间存活，另一个target经普通helper逐次增长。最后调用和view最后使用后执行clear（两层helper中的truncate(0)）及give，未使用的描述环境不制造清理债务。

最终CLI由同一干净源码重新构建，SHA256 `ce756a21ab82ff8a06a000b0e7d765c7b608b2657b1674e906426d6d79df7066`。两种优化合计48次实际执行：12次普通、36次真实realloc/free包装器执行，含20次实际分配或增长失败。每次逐一核对alloc_calls、allocated、frees、live、peak_live、peak_bytes、输出和退出码；live始终为0，allocated=frees。失败增长还通过源码检查原长度、容量及末字节保持，不以手写C重实现消费者行为。

错误路径守卫输出固定为F/T/S/Z的实际析构顺序；错误tag返回1，成功返回0，非法输入及EOF返回9。机器IR与编译/链接命令在consumer02收据中。平台是x86_64 Linux；本片没有运行Wine或原生Windows，不借用旧阶段结果代替。

## 原失败与证据选择

`failure-history.json`逐项保留初始门、原漏拒和失败身份：磁盘满与E0463缓存问题发生在测试执行前；一处新增Rust测试format转义错误也是0测试编译失败。旧094断言以及初次Slice拒绝阶段错误均保留原记录。内部同源错误Success探针保留原源码、诊断和阶段哈希；后续修复没有把正例改为Unsupported，也未降低同源拒绝目标。

冷启动handoff仍有历史规范/任务快照失败，原完整输出保留；本片没有修改正式工单或检查器使其通过。最终限定结果不覆盖该失败，更不覆盖全仓或产品门。

本目录排除可执行构建产物。新187的源码输入、编译结果、机器输出、全部raw候选及双入口诊断，以及两份实际LLVM文本均保留。大量可重建阶段文本和邻接payload以SHA256/长度索引表示；原始文件仍在run-logs中，root可另作完整压缩归档。`SELECTION.json`明确这个索引不能代替原执行日志。完整源码、证据与构建缓存按交付策略分别打包。

## Rust四目标与阶段边界

| 目标 | 本片证据与限制 |
|---|---|
| 同类安全 | 捕获/参数别名、内部窗口、give/drop后未来使用和raw伪造有真实独立核验；发现并修复了一个实际漏拒。仍只是已实现有限形状，非全语言安全证明。 |
| 运行与内存成本 | 既有内联ptr捕获表示，真实分配失败保持状态且责任守恒；未做同语义Rust计时、峰值内存或尾延迟比较。 |
| 表达与维护 | 普通ByteBuffer描述与同步helper能直接组合，不增加运行时保活机制；未测六类维护演进成本，不能以代码短或测试多断言优势。 |
| 编译成本 | 新的私有闭包与槽替换计入实际预算并有ResourceLimit边界；未作编译性能比较，速度支线仍独立。 |
| 至少一项净突破 | 此处的工程增量是可执行的局部借入字节回调及真实分离义务闭合；尚不构成产品级Rust净胜出或一般图内存管理突破。 |

完整语言NotSatisfied、产品NotRun；公共schema、全量门和后续循环callback合同由root继续组织。
