404 layout/oracle 精确冻结独审

结论：对 da06→404 的完整六文件差异（215+/9−）及其既有发布消费者只读核查，未发现本次新增的阻塞性安全或事务问题。此前 helper 角色与 class layout 缺失两处接线已闭合；这不是 native quality 或性能通过声明。

精确对象：commit `404aafdc7844ff3d85a08edb3fd0da0f2046816a`，tree `2ac33abd7e069c0cdc5f949b0e4852ec8863ffca`；基线 `da06f87194b7e21c2ae2f77bebf600e878e0aa1a`。本次没有编译、测试、fmt、strict、release 或计时，未修改仓库源码。

**Oracle。** `scalar_model` 的 64hex 名称仅定位候选。每个 class 的 `(result, params, suffix, body)` 必须与同份当前 IR 中至少一个 zil.fn.N 完整相等；参数名、SSA、block、常量、实际 callee 和 suffix 均不擦除。随后限制 i32 返回、有限整数宽度、纯操作白表，拒绝 ptr/undef/poison/zeroinitializer；调用仅能指向本次 classes 或原精确 intrinsic 表，逐个核返回类型、参数数目和 i32 实参，并以迭代拓扑遍历拒绝自环/互环。全部成功后才返回 classes/class_sources，不把 hash 后缀或 scalar exports 当授权。

`layout_roles` 的 importable 集、`inventory` 的可删除定义集仅增加该验证集合；完整 closure、prelude/declarations、canonical ownership、protected/cross 排除及后续 ELF/relocation/O2/runtime 门未被删除。source witness 是当前 pre-partition diagnostic 的结构见证，不是新的源语义或 Verify 证明；该边界保持。新增 Python 回归源码覆盖 body/ABI/来源/名称错配、memory/poison、foreign/source/self 调用拒绝及 imports 集合/closure 拒绝。

**真实 layout 与域隔离。** `partition_classes_reusing` 先完成原 `partition_view_once`，fallback 仍直接 None；只对该实际记录的 pre-prune Layout 创建 Pending，并用同一 source 和实际 units 创建 Materialization。没有从 ELF 或符号前缀推造布局。`Cache::new` 原 key 包含 installed compiler identity、完整 machine triple/items、planner 实际读取的 LIR 投影、entry/count/split/import policy；不是完整 LIR 每个无关字段的序列化。`with_classes` 再绑定专用域盐、原 key 和完整有序 class 集。此次 class 路径没有调用 load，普通路径 key 不变，也没有增加持久布局读取授权。

**发布事务。** build 将新 Pending 接入原 PendingObjectCache，PreparedCache 可用后才 pin；普通完成与 final-artifact 成功出口均携带 pending_cache。driver 对真实 layout 目的路径登记保护，在取消检查、dump、artifacts.validate、publication.commit 成功后再复核路径并 commit_object_cache；失败/取消丢弃 Pending，目的冲突仅丢弃可选 layout。底层仍临时文件写完整 envelope/payload 后 rename。class planner 拒绝时继续实际 original emitter 的既有回退链，不会发布失败 class plan。新增 Rust 回归源码明确检查 drop/pin 均未发布、commit 后 layout replay 等于真实 plan.units、payload 校验和、class/ordinary key 隔离及 ordinary load miss。

保留两项判读边界：layout 写入仍是 best effort，内存 partition 命中不会重新补写已缺失的磁盘 receipt；harness 因此仍须拒绝缺乏实际当前布局来源的质量判定。另既有 `layout_roles` 只在 `has_imports` 时执行完整 closure 检查，全零 imports 不是该函数独立证明 Complete 策略的充分条件；本次未改动这一旧分支，也不能把新增 import 回归称为覆盖该边界。这些不构成本次真实 class planner/Pending 接线的新增 blocker。

root 通报的 21 Python 通过、两份实际 IR 的 217/126 helper 全值见证通过，以及 gate08 状态均未在本次重跑；首败记录保持，正式 native quality 仍等待完整现有验收。
