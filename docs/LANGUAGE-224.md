# 224：完整普通 Product 的共享借入

本片基于 `a8e8831a9e28ac2ec83be6eac740a8f04a00af7d`，合同以主树 `changes/224-shared-product.md` 与规范 §8.1.2 为准。施工方只交付实现与限定实证，独立审核、整仓验收及集成由父任务管理。

## 可用行为

完整有限普通 Product 可以作为共享 `&Product` 参数、局部地址或同步转借。普通局部 `&root` 和直接 `&root.child` 取得实际存储地址；嵌套 `p.child.value` 及 `p.child.owner.n` 只读取 Copy 叶。`&p.child` 和 `&p.child.owner` 可传给同步只读函数。局部别名、分支及循环保留同一实际来源。

普通共享头的资格独立于 Store 元素。零存储用户资源可以属于普通 Product；Store 的非零元素布局条件不用于限制这种普通借入。Store 指令仍独立检查其元素资格与真实父容器来源。已有 callback Store 地址保留其原支持边界，本片没有授予普通 callback Package 头借入。

BorrowLocal 在 Analyze 与 Verify 分别检查 shell 及所有 owning child 的完整责任。内联子包只能完整进入其父字段；父字段债务代表完整子包。AddressField 保留实际父地址与共享权限，不创建拥有值或可变 Desc。Verify 从指令重建 Hold 和活跃 loan；候选不是安全证明。

活跃借入期间，whole move/drop 及相交 child take/replace 均拒绝。最后一次读取结束后，现有 NLL 允许移动整个 Product 或取走 child。借入返回/逃逸、`&Product` 的闭包捕获（包括当前立即同步调用形状）、可变嵌套 Product、共享地址下复制拥有 child/owner、ByteBuffer 的可变 Desc 和异步均未开放。

## 实现和表示

Types 的 place 分派区分直接拥有位置与间接共享投影，避免把 `p.child` 错当成命名拥有局部或隐式 take。新 owning-child 地址快照先计费，类型提示只做形状分类；真正的权限由后续资格与安全核检查。

Analyze 的普通 Product 地址准入与完整性检查不依赖 Store 元素准入。Verify 独立验证普通头的有限字段、名义资源定义、共享权限及责任状态；原来源、作用域、loan 和 NLL 检查仍是执行边界。没有新增 IR 操作、schema、Wire 版本、LLVM 布局、RC、锁、Box 或全局表。

实际后端继续使用 BorrowLocal、BorrowPackageField 和 AddressField。限定原生测试明确要求 `llvm-api` 的 `input_kind=builder`，核对无 fallback、优化前后验证均开启；O0/O2 均执行真实对象。

## 作者限定证据

`tools/language/224/validate.py` 在全新目录运行本片 10 组测试，记录精确提交、源码/产物 SHA256、工具链、命令、阶段产物、原始候选/Wire、实际对象、退出码与标准输出。`raw.tar.gz` 保存完整原始运行；结果收据单独归档到 `evidence/impl/224/author/RESULT.json`。作者证据不能被写作独立审核。

覆盖包括六类真实源码组合、零存储普通头、活跃借入冲突及有限边界；11 类攻击各走 raw 和 Wire，并对照 8 组合法候选。攻击覆盖可变升级、越界投影、拥有读取、Hold 擦除、父来源伪造、whole move/drop、child take/drop、借入不完整 Product 与过期存储。每个攻击更新真实模块指纹并检查具体 Verify 诊断，拒绝不能由 stale fingerprint 或资源预算失败冒充。

本片逐阶段 Types/Analyze/Verify 的 N/N−1/fresh-N 在真实输入上二分测定；默认限额不变。旧 206 flat 的具名 Types=418、Analyze=8673、Verify=10398 控制保持原值。新增预算边界与完整命令以实际收据为准。

机器测试的正常析构输出为 `XAB`。拥有 ByteBuffer 的共享 Product 在 O0/O2 正常输出 `XA`；使用真实对象重链接 allocator probe 后，每个优化级别执行正常、第一次失败及第二次失败三条路径。两个真实故障点均保留正确返回/析构行为，全部最终 `live=0`，成功分配数等于释放数。计数只描述这两个消费者；没有运行时间、RSS 或同语义 Rust 对照结论。

## 旧边界迁移需独立裁决

施工分支不修改旧测试。旧 206 `ordinary_product_admission_does_not_widen_other_raw_parent_contexts` 的 address 子例把全部元数据一致改写为完整 Outer 与 `&Outer`，现在是本片合同内合法输入；capture、Store、array 等原破坏须保留拒绝。旧 175 的共享 `view.r.code` 和普通 `read(&value)` 两条源码同样属于新能力，不得删除它们来制造绿灯。

旧 171 的 `var r` 经过前端隐藏 Package 槽表示。有限资源字段共享借入加入后，活跃借入期间替换该槽由旧 Types/Unsupported 前进到 Verify/SafeReject `E-BORROW-CONFLICT`，仍拒绝执行。基线和候选的实测差异保留；需要集成方保持原源码并只迁移精确断言。其它 mutable head、Desc、escape 和 capture 仍拒绝。

首次施工也保留了嵌套 place 分派误拒、测试夹具插入位置错误及新攻击错误假设 Analyze 必须拒绝全部 loan 的失败日志。它们各自修复或纠正证据等级，不能冒充曾经通过。shared_addresses 初次运行因其硬编码 clang 不在 PATH 而失败，加入已指定 LLVM bin 后原样通过。

## Rust 四目标与突破判断

| 目标 | 本片实证 | 尚未建立 |
| --- | --- | --- |
| 同类安全 | 有限共享 Product 的独立来源、完整责任、原始/Wire 攻击与实际拒绝 | 完整语言、一般逃逸、并发、异步安全 |
| 运行和内存成本 | 实际内联地址、O0/O2、真实分配失败与 exactly-once 释放 | 与同语义 Rust 的时间、RSS 或成本同级 |
| 表达和维护 | 普通函数可同步读取完整嵌套拥有组件，无须移动整个值或伪造 descriptor | 完整框架维护成本优势 |
| 编译成本 | 新工作有阶段临界值，具名旧 flat 预算不变，默认额度不升 | 本线编译速度优势或全输入零成本 |
| 实质进展 | 普通完整 Product 的共享读取由源码贯通至实际 LLVM API 机器行为 | 产品级四目标合取及可重复净突破 |

完整语言、B0、产品门的状态由父任务维持，本片完成不自动关闭这些目标。

## 独审后返修：直接拥有字段地址

ct_for 在初始交付 `2b799ed7f405c89c7725b86978286c144070e87f` 发现 `&root.child` 经 Verify 成功后在后端被旧 Copy/Closure 字段资格误拒。此前作者直接 child 正例只验证到 Verify，而已有原生程序走 `&root` 后的 AddressField，未覆盖 BorrowPackageField 的真实机器路径。这一覆盖缺口和原 Codegen/SafeReject 失败必须保留，不把初始作者 Pass 解释为该路径已经原生通过。

返修仅改 backend `emit/packages.rs::borrow_package_field`：对已核的 `mutable:false` 指令接受有限普通 Product 或有限 Managed Record 头；使用同一真实父 Package 布局和 local 指针发射字段 GEP，不加载拥有字段，不授予新的核验权限，也不放开 mutable/Desc/ByteBuffer 头。新增 reviewer 原字节 `child-nll-control.zil` 及直接资源字段借入的 O0/O2 机器回归，工具目标更新为11组。新代码与作者限定实证分开提交；非作者复核交还 ct_for，未自行宣布审核通过。
