# 099 字节拥有结构与闭包组合

本片依据先行的 `PROPOSAL-0099-BYTE-OWNING-COMPOSITION.md`，接续上一会话未提交实现；原样恢复点为 `7ca63c30`，完成源码候选为 `86aa00bbf60ba93b214dedce758e73f1d7d0bca4`。沿用 schema/wire 17、现有 Package/Managed 责任和内联环境，不引入 RC、隐式分配、世代机制或新的证明通道。

## 为什么与如何

已有动态字节可以由函数创建、传参、转交和借片，但不能放入可保存的拥有状态。本片开放 Package 的显式 `manages ByteBuffer` 字段、直接 ByteBuffer 拥有捕获以及完整 Package 拥有捕获，普通工厂可返回其自包含 Fn/FnMut/FnOnce 环境。Package 的字段 take/refill/replace、普通返回和已有同步错误路径复用既有责任合同。

Types 识别真实拥有捕获和直接/普通 helper 写效果；Analyze 与独立 Verify 分别把动态片的依赖追溯到实际 Package shell 或捕获位置。整个 Package 作为保守根，字段名不同不制造 apart 证明；活片阻止相交增长、写、take、替换、whole move 和销毁，最后使用后恢复。Verify 从 raw/wire 重建来源、调用效果、Fn 权限和唯一责任，不读取前端证明。

LLVM 继续保存 `{ptr,len,cap}` 内联头。Package 与环境的责任标志先清除，再调用内建字节释放；转交不复制责任。原会话补丁在 Types/Verify/Lower 已准入直接拥有捕获，却漏掉 LLVM 捕获字段及环境对齐准入；本轮真实 native 测试定位并补足两处。这是未完成纵切的代码生成缺口，不转记为 Roam 发现的自然语义漏洞。

## 实证

作者限定验证包含33个源码编译结果（15成功、11安全拒绝、7显式Unsupported）；29个独立构造候选分别经过 raw 和 wire 两条核验路径。Linux O0/O2 实际执行覆盖 Package 转交、take/refill/replace、直接和 Package 保存回调、未调用即销毁、FnOnce 部分消费、错误和准备失败、增长失败保持与准确释放计数。

四模块 `byte_buffer_owned` 保存逐字节更新环境，零事件替换拥有字段并清理旧批次；5组二进制输入在ZIL/Rust的O0/O2下共20个进程，按独立数学输出同时核对。连同22个分配/释放观察进程，Linux原生测试共42次；另以portable源构建Linux及Win64，O0/O2共4个ELF/PE执行和50条检查。Win64由Wine执行，不能称原生Windows验证。

既有094源码11组、095独立raw/wire29组经回归；原有Unsupported夹具迁移到仍关闭的借入/非拥有形状，未删测试组。原会话两个helper冲突负例实际上提前命中FnMut绑定错误；本轮改成合法可变调用位置并要求Verify的精确借用冲突。首失败、代码生成缺口、测试预期纠正、全部命令和产物保留于作者原始证据。

## 保持的边界

ByteBuffer Desc 借入capture、返回借自环境的裸slice、拥有错误载荷、普通拥有聚合、一般嵌套拥有元素、CT/FFI载体、自定义allocator、异步/取消仍未开放。没有运行/内存/编译性能优于Rust的结论，也没有全语言安全证明或产品门通过结论。作者结果与原始证据见 `evidence/impl/099/author-result.json`；非作者审核和正式集成另行绑定精确提交。
