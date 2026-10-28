# 180：独占 Store 元素窗口

## 为什么

ZUI 的布局及事件更新需要原地修改子项的位置、尺寸和标志。175 的共享元素窗口已绑定实际 Store、初始化前缀和索引检查，但不能授予写权限。180 使用设计 1.4 §3.4、§4.1–4.4、§5.3、§8.1–8.2、§9.7 的有限合同补齐这一纵切，不改变一般资源转移或外部回调合同。

## 是什么

- `store_at_mut(store, index: i64)` 生成公共 schema 22 的 `OwnedStoreBorrowMut { store, index }`（opcode 88）；旧共享 opcode 86 保持原义。
- 接收者与索引各求值一次，按源码从左到右顺序；先以实际 `len` 核验 `0 <= index < len`，通过后才形成元素地址。机器布局仍是已有 Store 头与内联完整元素。
- 返回一个同步、不逃逸的独占 `Address<Package>`，只允许现有有限 Copy 字段及其受支持 Copy 投影读写。整个 Package 加载、Managed 字段读取/覆盖/转移、部分初始化孔洞均不开放。
- 独占窗口保守覆盖整个实际 Store。新共享/独占窗口、Store 元数据读及结构写、责任转移和清理均检查交叠；未知索引或不同参数名字不证明分离。兄弟 Store 的既有已证字段来源保持有效。
- 普通同步 helper 的显式 `&mut P` 参数使用真实独占再借用。最后一次使用结束后允许后续结构操作；派生字段地址也延续来源与贷款。直接捕获 Package 窗口、窗口或其派生地址返回仍不开放。

## 怎么做

Types 构造指令并检查精确元素形状。Analyze 传播实际来源并发布对应资源写效果。Verify 独立核对指令类型、初始化、来源、贷款互斥、函数参数分离及逃逸边界，不调用 Types/Analyze 求证函数。Lower 与 LLVM 只消费 VerifiedModule，复用现有长度检查、Copy 投影及 WriteAddress，不增加隐式分配或运行时借用计数。

公共 IR、Wire、指纹和 schema 由 181 集成者持有；180 仅实现其已发布指令的语义消费者。179 的 callback 字段精度由独立分支提供，本片不预先引入其代码。Win64 Store 支持仍由 181 接管，本片沿既有 Linux x64 目标准入。

## 验证与拒绝边界

限定回归覆盖实际 Copy 标量/数组更新、同步 helper 及再借用、最后使用后 reserve/pop、独立 Store、同 Store 共享/独占冲突、间接别名及父状态移动/字段替换、索引负数/等于 len/仅小于 capacity、资源头写入与地址返回/捕获拒绝。独立 raw/Wire 变异核对共享权限升级、错误 Store 或索引类型、伪造来源/计划、错误字段与整 Package 写入。

O0/O2 执行实际生成的原生程序，核对输出、退出、析构和 checked trap；不将手填 IR、测试数量或精简样例计为完整语言/产品门通过。默认预算、性能门、B0 与 FullLanguage 状态均不由本片改变。

## 施工基线

- baseline：`8c4ed992138fc103c0fbaf6318e2e34d261bda81`
- design SHA256：`fe11dddb8fd64aebd773e90dea5ceaa545da3e41e280fd939806c4dc0ea212e2`
- normative SHA256：`1cf7aadb8800c31436f158ee3cdf78bcea330ed2bfafc29112b8d11211655a6e`

## 原断言迁移

180 开放明确的同步 `&mut P` 参数，因此 175 测试中的原输入 `fn bad(x:&mut P){} fn main(){}` 从 blanket Unsupported 迁移为 Success，原源码保留；同时新增共享 `store_at` 结果不能传给该独占参数的 SafeReject 检查。其余 175 返回、直接捕获、资源字段、共享写与预算断言保持。首次冻结回归因旧期待失败的原始结果保留，不追改为通过。
