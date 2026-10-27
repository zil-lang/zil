# 174：Store 直接拥有字段

## 为什么

172 的动态初始化前缀只能放在独立 Managed 局部；长期 UI State 尚不能
通过普通 Package 保存动态子项。单独放开字段类型会使旧的通用清理路径
把 Store 的 `teardown=None` 当作无清理，泄漏整个前缀及分配，因此字段
准入、独立核验、定义发现和机器清理必须同批接通。

## 有限合同

- `package State { manages items: Store<Item>; ... }` 的字段是实际
  `Managed<OwnedStore<Item>>`。未标 manages 的 Store 描述字段仍拒绝。
  Item 保持 172 的非零布局、直接有限用户资源/ByteBuffer Managed 加
  Copy 完整 Package，不开放嵌套 Store、Closure 或拥有数组元素。
- 复用 Describe/Take/Drop/StorePackageField；普通字段读取生成实际字段
  Desc，`give state.items` 转交整个头，字段赋值先准备 RHS，再清旧字段，
  最后提交新责任。缺字段 State 不得整体转交。旧字段描述不能在替换或
  转交之后重新指向新实例；不改变现有别名/来源和借用检查。
- 完整 State 支持原有普通参数/返回、拥有错误载荷及完整固定数组元素
  的值语义。170 的数组字段叶形状仍保持 direct-Managed 非 Store 加 Copy；
  不顺带开放更深拥有数组字段。Store 及包含 Store 的 State 捕获仍拒绝。
- 机器布局仍是内联 State 与原三字 Store 头。父壳/字段旗标先撤销，再调用
  精确密封 Store 的现有 drop helper；helper 清 `[0,len)` 后释放块。
  普通值、错误载荷和固定数组的递归清理共享这一叶分派，不生成静态 cap 槽。
- 从函数值/局部/返回类型递归发现嵌套 Store 密封定义；每个实际类型节点和
  复制在确定额度内收费，公开预算不放宽。ByteBuffer 仅在嵌套元素出现时也
  必须保留其真实密封定义。
- Linux x64 与 172 相同；不添加新的标签、指令、FFI、CT 或资源头借入能力。

## 实施与消费者

Types 开放精确 manages 字段并保留捕获边界；Verify 从原始 Type 和资源
定义独立重建相同准入；Lower 与 LLVM 接通完整 State 和专用叶清理。
Analyze/Verify 已有字段债务和旧描述检查若出现反例，只修实际缺漏。
公共 profile 由 177 集成者发布，本片不改 schema/Wire 或注册文件。

真实源码覆盖普通构造/跨模块转交、仅通过 State 签名发现定义、字段取出与
补回、替换、部分 State 早退、拥有错误载荷及固定数组父壳。四模块消费者
持有 Ticket 与 Store<Item>，检查实际运行时批次、失败准备、前缀内容、析构
顺序和分配平衡。最近反例包括借入 Store 字段、重复 give、缺字段整传、
旧 Desc 失效、Store-bearing capture、嵌套 Store 元素、伪造密封定义和预算。

原 172 的 Store 字段声明拒绝输入保留，预期按新合同迁移为 Success；其余
原反例保持。每次失败和后续定向通过分别记录；本提案不预先宣称任何测试通过，
不升级 B0、完整语言或产品状态，也不作编译速度比较。
