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
`negative`覆盖真实worker/间接依赖/借用冲突/CT边界。`pending`中的父闭包和
callable wrapper是尚未实现的位置，报告Unsupported，不是永久安全拒绝。
普通Direct函数中的调用已开放；worker资格对可达静态函数的全部CFG块保守
扫描，不宣称路径精确。完整async/SABI、一般Transfer/Share、Mutex/Atomic、
借入/共享子载荷、可脱离handle、取消和其他平台均未由本片实现。

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
