# 202：普通 StoreElement helper 的五模块消费者

最终消费者实际源码为 `a66a7c0070024e41da2517b483c036b4dbc71518`，编译器来自干净 `7698369cc8c4b6f50d5971ec975040417c92f841`，冻结 CLI SHA256 为 `0f41b91e1c34a6831b3e9a882642eef0df6c5d5b30520f76a7b9c8a77e4ecf2b`。两者的完整 Rust 生产文件、清单及 build 脚本逐字相同；消费者提交另含文本证据收集工具。规范身份为 `10bd65c2…`，未把旧运行重标为新生产。

## 实际完成

| 运行 | 普通执行 | 分配探针执行 | 合计 | 真实分配失败注入 |
|---|---:|---:|---:|---:|
| Linux `linux04` | 16 | 110 | 126 / 0 失败 | 86 |
| Win64 / Wine `wine01` | 16 | 110 | 126 / 0 失败 | 86 |

这里的“通过”指实际退出码、输出、分配计数及清理符合逐用例的独立预期，包括有意失败时的 exit 1 和无效输入的 exit 9。每个平台分别进行了 O0/O2 两次真实构建，每次五个实际源模块、一次独立 Verify。所有分配探针结束时 `live=0`，Wine 探针的 `bad_operations=0`。原生 Windows 未运行。

`store_helpers.zil` 公开完整 T 的 empty、reserve、push、pop、len、capacity，`state.zil` 通过这些真实函数边界执行 Store 操作。两个不同实际环境从 Pair 移入两个完整 Item，元素窗口中的 ABBA 调用仍经过同步 invoke；扩容之后经普通 pop 返回的完整拥有元素继续调用。普通 Tag 身份、参数切片写入和父窗口规则都经过完整编译链。

输入 0/1/2/3/7 分别覆盖零次、一次、continue、两轮和 break；扩容从 2 槽到 4 槽，随后 pop B 并在需要时调用。新增输入 6 从容量 1 开始：第二次 push 返回整个 B 元素作为错误载荷，跨普通 helper 返回后清理，再清理 Store 中 A，原两个切片字节不得改变。该路径与输入 0 虽然同样输出 `2b1aZ`，分配峰值分别是 64 与 256 字节，独立 oracle 同时核验计数和峰值。

预期按源码约定的 64 字节 Item 布局、ABBA 序列、Store 容量与两个 ByteBuffer 的独立分配序列事先计算。每个真实分配点分别注入失败，另有不失败和最后一次之后的控制；没有从 LLVM 或观察值回填 oracle。

## 保留的先前结果

最初旧 CLI01 对照在执行前被完整生产文件身份门拒绝：root 的 IDE 单元测试位于 src/lib.rs，源码文件确实不同。该次 0 编译、0 执行，保持 NotRun。CLI02 的原消费者在 Types 阶段丢失 `pop<T>?` 成功形状，Verify 和执行均为 0；Types 作者修复后才使用 CLI03。

CLI03 的 `linux03` 已通过 Types、Analyze、Verify，随后因本次命令遗漏 LLVM17 动态库目录而出现 Link InfraError exit 127。该失败保留；`linux04` 只补实际 `LD_LIBRARY_PATH`，源码、CLI 和 oracle 均未修改。Wine 显式使用既有 WINEPREFIX、WINESERVER、WINELOADER、WINEDEBUG，运行前清除 TMPDIR；构建继承 LLVM 库目录，Wine 执行使用其实际库目录，环境均记录在命令收据。

## 交付与界限

`evidence/impl/202/consumer/SUMMARY.json` 是小收据。完整文本证据为独立的 `202-consumer-evidence.zip`，约 1.74 MB，包含 899 个文本 payload 及 MANIFEST；原失败、实际源码、命令、输出、Core/Typed/候选/Verified/LLVM 与平台环境均保留。ZIP CRC 和每项 SHA256 已验证，执行文件、object 和缓存被排除。

`tools/language/202/collect_evidence.py` 只有在两个实际最终平台矩阵均完整通过、源码/编译器/规范身份一致后才封包。没有成功目录的实际控制返回 NotSealed，且没有创建 zip 或 partial 文件。旧 201 源码和结果未被替换。

本消费者支持普通完整 T helper 与已有 State/Store/回调责任的真实组合验收，不等于完整语言 1.4 或 ZUI 产品已完成；最终独立复核及整仓集成由主线另行记录。
