# 325 结构化线程消费者

`thread_scope(worker.compute, observer.payload(), component.prepare(mode))`
创建真实OS线程，在子线程计算21，在父线程执行拥有Mark/ByteBuffer的body，
清理body后必join，再返回`(created,worker_result,body_result)`。worker通过私有
静态helper和循环计算，完整调用图由Analyze和Verify各自核验为纯标量。
payload准备P、body环境准备E各一次，顺序先于创建C。

`observer.c`只包装真实pthread入口观测异线程、父body线程身份、join及清理，
并注入创建/join失败；实际worker计算、context、pthread调用和结果转交由生产
编译器生成。其测试用mutex/cond不代表ZIL已支持Mutex。

| 输入 | 退出 | 输出 | 覆盖 |
| --- | --- | --- | --- |
| `0` | 0 | `PECBDJA` | worker与父body、Drop D、成功join J、返回A |
| `1` | 1 | `PECBDJA` | 父body提前业务错误，完整清理及join后传播 |
| `2` | 2 | `PECDA` | 创建失败，不运行worker/body，不join，body环境仍析构 |
| `3` | SIGABRT | `PECBDX` | join返回错误X，固定abort，不返回A |

`controls/parent_borrow.zil`验证父body可合法借用父局部，子线程仅收到i32；
`negative`覆盖真实worker/间接依赖/借用冲突/CT边界。`pending/parent_closure.zil`
保持原路径/字节，已由332晋升正控；callable wrapper仍Unsupported，非永久安全拒绝。
普通Direct函数中的调用已开放；worker资格对可达静态函数的全部CFG块保守
扫描，不宣称路径精确。完整async/SABI、一般Transfer/Share、Mutex/Atomic、
一般借入/共享子载荷、可脱离handle、取消和其他平台均未由325实现，339有限扩展见下。

限定复现（已有测试目标，无新integration target）：

```sh
source /workspace/.zil-env/env.sh
export CARGO_TARGET_DIR=/workspace/.zil-env/target325
export CARGO_BUILD_JOBS=1
cargo test -p zil-driver --features llvm-api --test generic_regressions thread325_ -- --nocapture
```

显式feature使测试同时验证Clang和LLVM API，各O0/O2。每次断实际backend及
无fallback；API O2请求4个编译单元，要求实际多单元。保留产物可设置
`ZIL_TEST_REPORT_DIR`，默认成功后清理临时目录。实际结果及历史失败见
`changes/325-concurrency-production.md`。

## 339：真实共享i32地址进入joined child

`shared339/shared_child.zil`保持原B失败字节：worker接收`&i32`并通过普通静态
helper读取，父body同时读取同一位置，join返回后父方才写入。有限参数来源及父
helper返回沿真实根核验；直接未核Store/lending/拥有投影保留Unsupported。
`nested_disjoint`将同一个shared x与可变y置于被借入writer，不能用整个闭包FnMut
位替代逐捕获权限。`helper_writer`及raw helper-return alias防止遗漏一般&mut结果。

新pthread context为`{ptr,i32}`（16字节、对齐8），旧i32 context保持8字节、对齐4；
返回tuple及失败策略不变。create发布已初始化指针/目标，child实际读取原地址；
非原子写被共享窗口排除。child结果写入独立槽，join成功才观察；无法证明静止
仍abort。没有新语言Mutex/Atomic、隐式RC或Box。

`shared339/observer.c`仅包装真实pthread，比较父body的真实input与context指针，
延迟production start到body Drop以后；生成child仍实际解引用父局部。两backend
O0/O2覆盖成功、业务错误、create失败及join失败，共16观察场景；另12源码控制
×4配置=48执行，cache三次执行分列。重放不叠加，初版与审核失败见changes记录。

```sh
source /workspace/.zil-env/env.sh
export CARGO_TARGET_DIR=/workspace/.zil-env/target339 CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0
export TMPDIR=/workspace/.zil-env/tmp-batch336/339
cargo test -p zil-driver --features llvm-api --test generic_regressions -- --nocapture
cargo test -p zil-backend-llvm --features llvm-api --lib scoped_thread -- --nocapture
```

精确作者受测源码、原输入SHA、阶段拒绝、预算与不支持项在
`shared339/validation.json`；独审分开记账，不把局部成功替代完整语言或产品门。
