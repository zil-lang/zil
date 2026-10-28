# 196 前端验证的原始失败与更正

本记录保留作者验证中出现过的失败，不能用后续通过覆盖首轮结果。生产范围以主树工单及规范为准。

## 原 09 输入的实际 Copy 身份

预检 `09_same_f_duplicate_owned.zil` 的工厂只捕获 `i32`。同一个闭包值传给两个 `FnOnce` 角色时，原有结构 Copy 规则允许复制出两个独立值；`FnOnce` bound 不会把 Copy 捕获改为 affine，也不会给非 Copy 捕获增加 Copy 资格。实际返回值为 `2`。

原始 09 的字节保存在 `crates/zil-driver/tests/impl196_fixtures`，没有改成另一份输入。开门前的 Expand Unsupported 只证明旧角色数限制，不是重复 Move 的安全证据。root 已批准将原 09 更正为 Success 控制，并增加真正捕获 Managed 资源的非 Copy `FnOnce` 重复实参负例；新负例必须实际 SafeReject。

`changes/196-proposal.md` 中 09 的旧预期属于保留的预检记录，由本更正说明解释；不能据旧文字改变生产 Copy/Store 规则。

## 首轮纵向验证

`run-logs/196-frontend/vertical01` 绑定干净提交 `5e49997cc7f54cf0abaf8944146821c17a44ed87`，实际 **17 通过、4 失败**。四项失败分别为：

1. 新多模块夹具误用 `import "helpers" as h`；改为语言已有的 `import helpers`。
2. 新拥有资源夹具误用并不存在的 `drop` 内建；改为显式普通函数消费资源。
3. 新 raw 重复非 Copy SSA 实参实际先被 `E-VERIFY-MOVE` 拒绝；保留攻击，只更正预期门。
4. 上述原 09 的预期错误；保留源字节，补充真实非 Copy 负例。

后续 `vertical02` 在干净提交 `f544fb08857109be31ff0d6736893c434bf976b1` 实际 **22 通过、0 失败**；其中有 **28 次 O0/O2 原生执行**。本轮不能重标为首轮通过，也不包含分配失败或 Wine 运行。

## 嵌套闭包内第二角色写入

Analyze 作者新增的源组合发现：拥有 `FnMut` 工厂返回的闭包内部创建 `first`、`second`，再把二者传给双角色 helper 时，旧捕获扫描仍把 `&F` 当普通有限 Copy 地址绑定。原始 `&first` 在 span `257..263` 得到 Types Unsupported，Verify 未运行。原输入保存在 `12_nested_second_role_owned_write.zil`。

作者收据 `nested-before01` 绑定 `c32826c5b3bb9ecfa19e955c362bbc4e0413941f`，真实 **0 通过、1 失败**。修复将捕获扫描的直接 helper 参数逐个按 role 约束签名和传递模式；扫描中嵌套闭包使用的私有函数签名不成为公开环境身份。Builder 继续核验实际 ClosureId、同 F 身份、参数资格及来源，Analyze/Verify 继续独立核效果和权限。

`nested-after01` 绑定 `f4a00bc70676d87c1c98b88e76da90325a56dc65`，实际 **2 测试通过、0 失败**，包含原输入的 O0/O2 **2 次原生执行**，以及同 F 不同身份、第二角色错误签名、第二角色共享实参冒充独占资格三份最近反例。Analyze 的协调篡改写投影攻击由其作者另行运行，不以该原生正例替代。

## 197 精确合流

`ae8be01d89aa849438d9cda96d3072fe73cbedc0` 合入 `27d8e922` 的 197 Mixed State：冲突按实际块合并，保留多角色的实例绑定向量和 197 的 `State<T,F>` 普通 Copy 参数专门化。单角色 State 路径保持；多拥有 State 角色仍不开放。`combined197-01` 绑定该干净提交，196/197 四个目标测试实际 **42 通过、0 失败**。

本文记录作者事实，不是独立验收，也不宣布完整 1.4、ZUI 或产品门完成。
