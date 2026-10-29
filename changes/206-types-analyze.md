# 206 Types / Analyze 作者交接

生产冻结点：`7b2b1f58a5b9d14a027f67ec746eb375328dc94f`。本记录只覆盖作者的 Types/Analyze 写集；独立 Verify、Lower/LLVM、消费者、非作者复核及最终 workspace 归各自记录。本记录不授予 206、完整 1.4、B0 或产品验收。

当前规范为 spec `f26a0d8701b9a00a38286f486b7bcae1b96cf1d9254add7c48d395bb3e6ccbc3` / norm `d71ea98d7c05e1491c62d535124f0076281db9cc625942de3672b55ef40a784d`。最早 `types01` 实际运行在旧 206 norm `20e0ebc805...`；已保留该身份，未改写为新规范实测。

## 实现

`zil-types::packages::ordinary` 为普通值位置增加私有 Product 入口；旧 `supported` 不改为递归总资格。完整 child-only wrapper 合法，子树至少一个显式 Managed 叶。Copy 伴随字段保持原资格；新内部 Store、具体 Closure、拥有 Sum/数组/借入字段不因 Product 准入而放行。`aggregates` 在克隆前预费整个新类型树，并检查包名义身份、非空及重复字段名；普通签名、推导和完整 child 读/替换接入该入口。

`zil-analyze::resources::product` 独立使用实际 SSA/Local 加物理叶路径记录 Created/Incoming；不调用 Verify，不增加深叶 owning debt。直接 Managed 仍以原 field Fact / Package SSA map 为准，父 child 只持一个直接 debt。构造、whole move、取 child、refill 都逐叶复制实际来源；未知普通 Call 结果仍为 Incoming。按真实 carrier/path 合流，不能按同类型 recipe 合并两个实际 child。新表、路径、集合复制/合流在扩张前付费；无 child 的旧路径不建立新表、不新增费用。

`budget::local_value` 仅普通顶层值位置可走 Product；旧 array/union/capture 分支仍调用 `legacy_value`。真实 raw 对抗另补 Address、Store 元素及未用资源头上下文的关闭检查。资源头上下文复用已有付费类型遍历，不把 Copy 函数指针签名误当成所拥有的内联资源负载。

## 实测

同一 clean 冻结点的 `final01`：11 passed / 0 failed / 0 ignored；`clippy02`：修改库、driver lib 和三个 206 测试目标的 `-D warnings` Pass。没有运行整仓、独立 Verify 或 native。

- `impl206_source`：原样 9 源到 Types；两个公开 Types API 对 unused complete Product 正例和空父名、空 child 名、重复 child 字段、child 无 Managed 四个 malformed 输入分别检查。
- `impl206_analyze`：三个 Created/Incoming 原样源族；unknown ordinary call；MakePackage operand/field index 变异；partial child/parent、refill；同类型两个实际 child 的 source/raw retarget；分支 Created/Incoming 合流及零次循环；旧 Address/array/ErrorUnion/Store/capture 的合法 flat control 与新 Product raw 负例；unused resource payload。
- `impl206_budget`：每个入口均实际执行 N 成功且等于默认输出、N−1 ResourceLimit、fresh N 输出逐值相等。

| 输入 | Types N | Analyze N |
| --- | ---: | ---: |
| 原 flat 186 源 | 418 | 8673 |
| mixed child | 753 | 21120 |
| 两次 whole transfer | 796 | 27114 |
| child-only wrappers | 752 | 26951 |

旧 flat 对照另在 accepted205 `61c2d252...` 冻结生产树上执行同一原始探针及源字节，实际 N 也是 418 / 8673。冻结树只有明确列出的 untracked 探针和 fixtures，生产 diff 为空。固定精确断言在该对照完成后才加入；不靠猜测或修改默认额度获得通过。Verify 旧 N 由 root 独立记录。

## 保留的真实失败

1. `parent-gates01` / `d3fb0712...`：raw `Address<new Product>` 被 Analyze 错误接受；在该首例失败后测试中止，不能声称原 Store 变异也实际失败。`c6281a59...` 修复，`parent-gates02` 和最终限定测试通过。
2. `origins-metadata01` / `838e2952...`：unused resource.payload 含 Product 被 Analyze 错误接受；6 passed / 1 failed，source 测试二进制尚未执行。`e4112ad4...` 修复，后续完整限定 11/0。
3. `clippy01` / `e4112ad4...`：direct/deep origin 返回类型触发 type_complexity；`7b2b1f58...` 仅增加类型别名，后续 Clippy 和同头限定测试通过。

## 证据

原始目录为 `run-logs/206-types-analyze`。`RUN-IDENTITIES.json` 从各次不可变 commit 树补录完整 source/norm 身份并核对规范文件 SHA；原 command.json / stdout / stderr 未修改。每次实际 dirty 状态明确保留，原补丁、源、Core、Typed、candidate 和预算输出均在独立证据容器中。

`206-types-analyze-evidence-compact.zip`：925125 bytes，SHA256 `009583c10e8a3d918d5944bc14a2fa85d1bc50864260ac3083749c5a31342f2f`，pack/verify Pass；1093 logical files / 276 unique contents，0 execution products。该容器只封装记录，不执行编译器，也不产生新验收结论。仓库仅收小收据；缓存、执行产物及整套证据不进入源码包。
