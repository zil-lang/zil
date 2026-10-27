# 177 公共接口独立审核

审核者 `/root/store_field174`，实现作者 `/root`。裁决 **Pass**，仅绑定
`aa235a3379492221197b4686bec042c042c4e100` 的公共登记。规范内容逐文件
重算得到 `8aecd0f052b3d8eb099d19d26cd0bad56b26d3a7be91c79aca5a962a010711be`。
生产代码未由审核者修改；规范文件与公共变更清单见 `SOURCE.json`。

## 审核结论与证据等级

本提交在实际 IR、typed fingerprint 和 Wire 中登记 86/87，保留独立 Verify
的明确 Unsupported 占位。传输成功仍只返回原始 module/candidate，不返回
Verified 证明或借用权限。此次审核没有把这些登记当作 Store 元素访问或回调
字段的语义完成，也没有审核我自己实现的 174。

原 IR/Expand 69 项和 public binding 1 项在精确公共提交、全新独立 target
通过。完整新增 7 组在审核源码 `061e22536784f4e568dfbcebbb8ca2086cdc54a7`
通过。该提交仅添加测试，与受审提交的生产文件完全一致。完整原始收据分别
位于 `original/` 和 `probe02/`；`probe01/` 保留最初的 6 Pass / 1 Fail。

## 独立检查

| 检查面 | 实际执行与结论 |
|---|---|
| 原始指令格式 | 6 个明确手写字节记录进入真实 module envelope，覆盖 86、87 false、87 true 与 result 有/无；字段值取不同高字节，逐项核对真实编码和解码结果 |
| 非法输入 | 真实 Reader 拒绝非规范 result/mutable 布尔值、每个新指令记录的截断位置、未知 88/254 opcode、旧/未来版本、反向 span 与 trailing byte；没有模拟编码器或快照 oracle |
| 指纹 | 17 个独立整 module 变异分别改变 Store receiver/index、field local/index/mutability、result presence、span file/end，全部改变实际 fingerprint |
| 预算 | result=None 的 86/87 envelope 分别 647/648 字节，节点 N=23，所在 block 指令 N=10；真实 Writer/Reader 每类 N 成功、N−1 返回 ResourceLimit，无预算放宽 |
| 权限边界 | 公共版本上 3 对 raw/Wire 新指令均精确 Verify/Unsupported；u32::MAX local 另由 Analyze/E-ANALYZE-IR 在局部表前置拒绝；结构 address predicate 未把 Package 或其地址变为 Copy/general pointee |
| 保留名 | 10 个新增源码负例覆盖资源/Package/枚举声明、未用泛型参数/局部、extern、循环绑定、closure 参数和模式绑定；4 个声明另走 standalone Expand 精确诊断 |
| 命名空间 | 同名字段、实例方法、较长标识符、`store_at.answer()` 模块限定路径仍 Success/Verify1，避免把保留裸调用名错误扩成全局成员禁用 |

源文件静态核对还确认：公共添加的固定宽度操作沿原有有界 instruction
collection 计费；87 接入局部初始化与清理访问分类；86 的两个 ValueId 均
接入 Analyze 与 Verify 的操作数遍历；共享 Package 地址只是结构分类，
`is_address_pointee` 仍未增加 Package 分支。schema/Wire21 与 fingerprint
domain 同步，旧 tag 字节格式不变。

## 首次失败和可持续回归

首轮失败是审核夹具要求项目入口的声明名称冲突返回
`E-OWNED-STORE-NAME`；实际项目归一化先在 Expand 返回
`E-NAME-DUPLICATE`。原样源码确已正确拒绝。夹具逐例修正预期，并增加
standalone Expand 的 `E-OWNED-STORE-NAME` 交叉检查，未修改生产代码或放宽
接受集合。首次提交 `2a11a3e5564144590d9086836aa7b69ecda4813c` 和原日志保留。

两组权限占位测试只对精确公共版本有效，175/176 实现这些操作后不应继续要求
旧占位措辞。完整 7 组源码按原字节冻结在
`tests/frozen/177_public/review177_public.rs`，SHA-256 为
`07500c9e5b4860a09e9571af14807ea9462e43d908490bab46a9336e3a03f68e`。
该目录不属于 Cargo 自动目标。其余 5 组留在 driver 的自动测试目标，并在
`ab9a2f3ca409987372644e62867a773d41698db7` 实际重跑 5/5；完整与可持续两个
版本都通过 fmt 和该测试目标的 Clippy `-D warnings`。没有把 5 组重标成 7 组。

## 明确排除

此裁决不覆盖 174 Store 字段实现、175 贷款/元素窗口机器实现、176 Closure
字段实现或 177 后续合并与六模块消费者。上述工作需要各自的精确审核与集成
收据。这里没有原生消费者、运行/内存比较、全量正式验收或产品净突破结论。
`B0 Doing`、完整语言 `NotSatisfied`、产品 `NotRun` 不变。
