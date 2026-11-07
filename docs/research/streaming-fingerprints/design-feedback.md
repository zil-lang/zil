# Boundary Lab 0.6.0 到真实生产的反馈

冻结生产提交 `38b9169fde05fa0c0effc7fdd74296b9a6c20711`，schema16。
主线后续 schema17 的字节容器不混入这批观察。所有源码由生产前端处理，
TypedIR、接口及诊断均来自实际编译输出，没有手填阶段对象。

## 先复用原研究，再判断生产缺口

原包 SHA256 为 `c22948a96d9f9de4a615dcf3314f393a44a3066b2701b6a50fa5b3e15dc6572a`。
318 个文件复制到独立目录，原 ZIP 和原目录没有修改。执行原测试、原 0.6 研究脚本、
原危险 prefix 接口命令；Node 24 默认隔离模式报告 14 个测试文件，通过
`--test-isolation=none --test-reporter=tap` 实际核对 147 个测试函数均通过。
另一个 26 函数定向执行与这 147 个重叠，不累计为新覆盖。

原研究本轮重放得到 511 个程序、12,168 个查询：4,687 安全、7,481 不安全；
误接收、误拒绝、返回可达性和后状态不一致均为 0。原 concrete oracle 实际
访问 28,538 个上下文、644,770 步，Verify 为 18,802 步；混合分支为 9,618 个查询。
这些数字是用户既有 Lab 的复现，不是本轮发明的算法，也不是生产语言安全证明。

原消融的近邻仍成立：去掉顺序产生 349 个误接收，只看正常出口产生 775 个，
假定实参不别名产生 13 个。`nonreturn-double-destroy` 和 `order-apart-required`
保留了具体反例。因此后续摘要必须记录危险前缀、返回可达性和真实实参对应，
不能把没有正常返回等同于没有执行风险。

## 重复调用：生产已经有摘要，不能继续以旧展开为基线

构造深度 d 的真实 ZIL 调用 DAG：`f0` 写入资源，`fi` 顺序调用 `f(i-1)` 两次，
由 `main` 调用 `fd`。仅编译到 Lower；不执行指数运行次数和整数溢出。
无新的配额或容量设置，8 个深度均通过真实 Analyze、独立 Verify 和 Lower。

计数版在实际函数体求解入口计数；非计数版的诊断、查询事件和所有产物 hash
逐一相同。以下是观察值，不是 CPU 指令数或正式计时。

| 调用 DAG 深度 | Types 写效果体访问 | Analyze 资源体访问 | Verify 资源体访问 |
|---:|---:|---:|---:|
| 0 | 2 | 4 | 4 |
| 4 | 10 | 12 | 12 |
| 8 | 18 | 20 | 20 |
| 12 | 26 | 28 | 28 |
| 16 | 34 | 36 | 36 |
| 20 | 42 | 44 | 44 |
| 32 | 66 | 68 | 68 |
| 64 | 130 | 132 | 132 |

本族符合 Types `2(d+1)`，Analyze/Verify 各 `2(d+2)`。源码核对也证实：Types
已迭代有限的参数写效果集合；Analyze 和 Verify 各有 080 的摘要消费者队列，
在调用点代入效果而非递归重跑所有被调函数体。有限族的线性观察不能推出一般
程序的复杂性上界，但足以排除“生产目前和 Lab 0.5 一样按每个调用重复展开”
这个错误前提。

## 顺序与失效：真实生产最接近的控制

18 个源案例中 14 个成功到 Lower，另 4 个得到精确拒绝。

| 请求 | 真实生产结果 | 能说明什么 |
|---|---|---|
| 两个形参传入同一资源后顺序写 | Success | 普通 Desc 写不会自动消费唯一释放责任；同源更新可以合法 |
| 读出值后 `give` 消费资源 | Success | 已复制标量可在消费之后返回 |
| `give` 消费后再读取旧描述 | Analyze / `E-ANALYZE-IR` | 正确拒绝失效描述，消息为 `head transition invalidates a description required later` |
| 写后递归、递归后写、有返回分支递归 | Success | 当前生产可编译这些函数，不能从成功推导完整有序安全摘要存在 |
| 有条件提前返回 | Success | 真实 CFG 与导出接口已留存 |
| 在 Desc 上调用 `destroy()` | Types / `E-UNKNOWN-FIELD` | 当前没有该操作，不能冒称验证了 Lab 的 destroy 协议 |
| 在 Desc 上直接调用 `drop()` | Types / `E-UNKNOWN-FIELD` | 清理入口没有自动变成公开 Desc 破坏操作 |
| `resource Node { next:Ref<Node>, ... }` | Types / `E-INFINITE-LAYOUT` | 这个递归对象形状未成立，不是链表安全性验证结果 |

因此本轮没有把 Lab 的 `destroy` 偷换成生产普通字段写，也没有把缺语法的拒绝
算作资源协议安全证明。生产责任转交与实际清理的最近近邻使用真实 `give`。

## 公共合同的具体缺口

单源 `compile_source` 本来不产生项目接口，不能拿它的空 `scalar_interfaces`
判断所有接口缺失。额外对 4 个真实源案例执行 `compile_path` 到 Verify，各有
cold/unchanged/private/public 四次请求，共 16 次；保留 Types 原接口、Verify
后的 `verified-interfaces.txt`、最终 CompileReport 和 TypedIR。

`run(r) { r.n += 1; run(r) }` 与 `run(r) { run(r); r.n += 1 }` 的实际项目接口：

| 字段 | 写后递归 | 递归后写 |
|---|---|---|
| signature fingerprint | `c8e2861767affccd71ff6130a70879c0ff6743ecad11960aedac2771de270182` | 相同 |
| contract fingerprint | `4e93f95c06568ba017f899bcdc5810be9c8d11c03cfdf73837c3ccde26a0e313` | 相同 |
| implementation fingerprint | `6d8dd83d88ddd9c3fcc1b847de55fb56d2d4d827e6ad7b158c33e936e0f6e77f` | `e63fcd7bcd2b77419f6c83c5230cfd2f6183e157c5c3acd6f35981ee06915a05` |

最终接口中普通 `run` 的 result origins 为空，callable contracts/returns 也为空。
TypedIR 保留了真实 Call 和 WriteDesc 的相对次序，却没有一般普通函数的有序
prefix/可返回性摘要字段。第一个程序可在每次递归前写入，第二个的写在不可返回
的递归之后；当前合同 hash 并不承诺区分这种观察。

**这不是已经发现的不安全缓存。** 现有 driver 仍保留不同的实现 fingerprint
作为依赖。本例阻止的是未经新增合同就改成“只凭公开 contract hash 判断普通
资源体的语义等价”的优化；它同时为新设计线提供了可执行的生产反馈。

## 研究下一步的可裁决契约

现有写效果集合和独立 Verify 保持。若设计线确实需要描述性失效或一般普通函数
协议，先明确观察接口：哪些 prefix 会失效、是否正常返回、正常返回后的必要关系。
实际别名、共享 guard、错误出口和控制支持必须在接口中有位置；生产合同和原 Lab
语义必须逐项对照。对同一语义先比较完整接受/拒绝集合及反例，再研究表示和调度。

若仍要求显式完整联合关系，则 [结构边界](STRUCTURAL-BOUNDARY.md) 的输出下界
适用；若只查询有支持的局部坏谓词，明确这是另一个消费者合同。先证明所提查询
保留原本所有必要安全观察，才能依据省去物化后的完整成本判定是否值得实现。

生产计数也说明摘要后还有身份成本：512 模块 cold 请求实际 SHA 输入
4,321,009 字节，其中 raw Debug 为 3,573,547 字节；unchanged 的 raw Debug 已是 0，
但仍有 2,573 次 SHA 调用及 5,150 次 Database snapshot 查询。H 只改变同字节流的
存储方式，不会消除这些工作，其正式完整成本单独裁决，不能归因为图切割突破。

## 证据定位

原始运行根为 `out/runs/093/`；正式便携归档位置及 SHA256 见最终结果收据。

- `lab-replay-001/`：原包逐文件身份、原始测试/研究/危险接口命令与输出。
- `bridge-B-001/`、`bridge-B-counted-001/`：18 源案例及逐阶段真实输出。
- `project-bridge-001/`：4 个实际项目接口和 16 请求。
- `design-feedback-001/receipts/`：沿用原包 0.6 反馈模板的 18 个 JSON，含源码、
  TypedIR、已核导出合同、诊断、工具链、真实计数与文件 SHA256。
- `design-feedback-001/baseline/`：精确基线规范、产品门和相关生产源码字节。

报告明确缺失的字段，不推测阶段耗时、递归运行行为或产品级 Rust 性能。
