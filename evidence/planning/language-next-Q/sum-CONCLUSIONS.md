# Q 上普通拥有 Sum：最小实际预检

冻结CLI来自Q `e92508be996a190fa188d08319d37cc8b522bae6`，SHA `ebc418f594c91f1c54b4f655cf6fdd0d99a9461852aec8e1f7ad09701d433395`。全部源、原始阶段文件和命令在本目录。六份源与可接受源的原生输出预期均在执行前冻结；没有修改core、规范或任务，没有Rust构建或全量。

| 源 | 实际check/Verify | 实际原生 |
| --- | --- | --- |
| 01 `void!Leaf`，Leaf管理Mark与ByteBuffer | Success，Verify1 | O0/O2成功及Unit错误分支共4次，`SRA`/exit0、`SA`/exit1 |
| 02 `void!Root`（child Leaf，无Store）；同源另有`InitError!void` | Success，Verify1 | O0/O2共6次；另以`SER`观察拥有错误载荷的清理 |
| 03 `void!Session`，Session child管理`Store<Leaf>` | Types Unsupported / `E-UNSUPPORTED`，Verify0 | 未构建，不以其他早期错误代替 |
| 04 `InitError!Leaf`，两侧均为不同的非Copy普通Package | Success，Verify1 | O0/O2两种活动tag共4次，`SAR`/`SER`均exit0 |
| 05 `enum MaybeSession{Empty,Some(Session)}` | Types Unsupported / `E-UNSUPPORTED`：`enum payload requires a finite Copy value`，Verify0 | 未构建 |
| 06 在01最近正例上再次对同一result执行`?` | Analyze SafeReject / `E-UNINITIALIZED`，Verify0 | 未执行；不是功能Unsupported |

**校准：两侧拥有ErrorUnion并非全部缺失。** 两个不同flat Package的构造、返回、运行时tag及临时销毁已实际贯通；`void!`平坦和无Store嵌套Product、反向拥有错误/Unit壳也已有。这里没有证明任意两侧载荷、拥有错误传播、匹配、捕获或替换位置都闭合。14次原生仅核输出与退出，没有本轮allocator注入或分配平衡断言。

真实相邻缺口是：已能普通拥有的含Store递归Product还不能进入该ErrorUnion成功载荷；普通enum仍只接受Copy载荷。03的原始诊断为`error union payloads require finite Copy values or complete owned Packages; direct owned resource success requires explicit mode and a Copy error`，而该Session本来是完整Package Value，不是要求额外result(create)的直接Managed返回。前一Q236矩阵独立证明其普通构造/返回/替换及实际Store清理；本预检不重复那些364次。

**最短通用下一片：有限普通拥有Sum。** 以已合法普通拥有形状为责任依据，闭合Unit/Tuple变体中的多个拥有字段、完整Product及Store叶；共同核真实tag、布局、载荷转交/consuming match、变体切换前准备、失败残余与活动分支清理，并把同类含Store Product在既有ErrorUnion载荷边界贯通。不能仅对`void!Session`、单payload或名字Option开口，也不重做已经能工作的flat两侧拥有联合。普通Enum、ErrorUnion、完整Product各自的名义和来源事实需在Types/Analyze/Verify/Lower中独立一致。

下一片的最近反例应包括旧variant窗口跨切换、同一payload重复取走、错误tag/字段路由、重复责任与未成功准备就清旧分支；保留本轮六份原源与旧拒绝。泛型Option/T?、借入/lending、dyn、捕获、并发和retained不能随这一片默许开放。本次是施工范围预检，不是新的功能验收或Rust产品成本结论。
