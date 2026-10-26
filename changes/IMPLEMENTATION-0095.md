# 095：动态字节存储的独立核

本片锁定主树派工 `.task/impl/095.md` 与公共 schema17 基线
`1ec3bf43ee373c073c19f2342705924f221b8d2a`。规范/接口只引用
`changes/PROPOSAL-0094-0096-BYTE-STORAGE.md`，不在实现中改动接受门。

## 实现范围

- 独立准入封闭 ByteBuffer payload、保留资源定义及六条指令；拒绝普通构造、字段写、聚合/闭包/CT/FFI 等未开放路径。
- 复用既有 Managed/Desc 责任流；ByteBufferNew 建立 fresh 唯一责任，给入参数是本地拥有载体，不能返回其裸借片。
- 核内从真实 DescribeLocal/参数/Load/Store 重建 Desc 来源。ValueHold 可记录这些来源，但 Desc 不因此成为持续窗口；ByteBufferBorrow 单独建立 loan。
- 核内推导普通调用的字节存储读写效果和实际活性点所需的参数 apart 关系，按真实实参来源实例化，递归固定点保留危险有限前缀。
- raw/wire 正反例覆盖伪造、预算、操作数支配、来源返回、窗口冲突、跨函数同源及顺序控制。

## 边界

不依赖 Types/Analyze 的安全证明，不改公共 schema、后端、预算或产品门。真实 LLVM/源码闭环由 094/096 组合验证。本片的手工 TypedIR 测试仅报告独立核边界证据，不冒充源码或原生执行。

## 验证计划

限定运行 `cargo test -p zil-verify --test impl095_byte_buffers` 及受影响的 Verify crate 测试。记录精确命令、输入、工具链、结果与所有失败；正式全量、独立审核和合入由集成者负责。
