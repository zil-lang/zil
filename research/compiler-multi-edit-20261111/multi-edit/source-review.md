# Partition layout 投影缓存独立源码审核

结论：**源码审核通过，未发现需要阻止进入后续验证的正确性问题。** 这只是精确源码的读集与缓存重放审计，不代表编译、测试、性能或生产采用已通过。审核过程没有构建、执行测试、采样或修改候选源码。

## 精确审核身份

| 身份 | SHA |
|---|---|
| 基线 main | `99035931d358add0eaed6d4535035c38413701dd` |
| 初始本地冻结提交 | `8c098a50820976234663c92239c59206d543ae1a` |
| 发布生产候选 | `b5dbe03d38ed3c7c9f11547c5d074b2f0f96aec8` |
| 两候选共同 tree | `08f873e1ff1c49a788723baf852b545186d74232` |

独立读取本地 layout-projection 与 layout-validation 的 HEAD/tree/status、两提交 diff，并通过 GitHub Git commit API 读取发布候选：发布 tree 精确相同、父提交精确为基线，两个候选之间内容 diff 为空。变更只有 `crates/zil-backend-llvm/src/machine_cgu/layout.rs` 和同目录 `layout/tests.rs`。本结论绑定上述发布 SHA 和 tree，不借助作者身份或历史候选结论。

发布提交：https://github.com/zil-lang/zil/commit/b5dbe03d38ed3c7c9f11547c5d074b2f0f96aec8

## 为什么投影足以决定 ordinary layout

逐项读取 `partition_view_cached`、`partition_view_recorded`、`partition_view_once`、`small_caller_components`、`source_name`、`Layout::valid/replay`，以及 machine model 的 `cost/references` 与 `direct_calls`。ordinary planner 的决策只读取以下结构；新 key 全部覆盖，部分字段保守绑定得更细。

| 决策读集 | 新 key 对应覆盖 | 判断 |
|---|---|---|
| compiler、entry 类型及指定 function ID、unit count、split、import policy | action_key 开头逐项 Hash | 不会跨策略、入口、编译器混用 |
| machine item 顺序、种类、定义/声明状态、全部符号名及重复/clone 冲突 | item 数量与逐项 variant tag、name、blocks/initializer 的 presence | indices、prelude、owner 与命名冲突决策一致 |
| LIR function 顺序、id、name、foreign presence、file | 每 function 全部绑定 | 文件分组、256 函数 chunk、main 定位与 source identity 一致 |
| LIR caller 扫描的函数/块/指令预算与首次命中位置 | functions.len、每函数 blocks.len、每块 instructions.len、每个位置的四类 callee ID/None | 不只是收集 callee 集合；early-break 前实际花费可重建。foreign 与其他指令形状即使保守多绑，也不会漏绑 |
| local identity 与引用闭包 | Function::references、Global initializer 的递归 symbols、完整 Linkage | 内部函数地址、local global、初始化器引用决定的并集与声明筛选一致 |
| direct-call import 闭包 | direct_calls 与 references 分开绑定 | 同 refs 的直接调用与间接调用不会被误判成同 key |
| small caller 选集、total/selected cost、machine block fuel、256/4096/16x 阈值 | 每函数 cost、blocks presence 和 block 数，definitions/refs/calls 也已绑定 | 预算与选择结果一致；不需要绑定每个算术操作数 |
| 完整 importer 的 External 判断及总复制上限 | 完整 linkage 枚举 Hash、每函数 cost、source ID/name/foreign、refs/calls | `External` 与 `AvailableExternally` 即使同为非 local 也分离；max-copy 判断一致 |
| `.lc1` ABI 名、`main`/`zil.pe.entry`、`zil.import.{owner}.{name}` 冲突 | machine 原始精确 name、LIR id、item 次序 | 不进行模糊化或去版本，source_name 的优先选择一致 |

Function signature/attrs、Global type/constant/unnamed_addr/alignment、NamedType 完整内容和 target 仍被保守绑定。被省略的是与 ordinary placement 无关的当前算术/字面量、SSA/block 名及 global 常量值；每次 replay 使用当前 Module 的真实 item/body/initializer，不把旧 body 当作缓存产物。Module fingerprint 也由当前请求重放，不构成验证权限。

在排除通常密码学 Hash 碰撞与沿用可信私有缓存目录假设后，没有找到“相同投影、不同 ordinary layout”的两输入反例。此结论不宣称整个 ResourceHelpers planner 只读这些投影。

## ResourceHelpers 与验证边界

`partition_view_cached` 每次先对当前完整 module/LIR 调用 `resource_helpers::prepare`。成功时立即 fresh replay 并返回，不读取或写入这个 ordinary layout cache；失败时才走 ordinary cache/planner。因此，即使闭包 metadata、地址操作数分布等 ResourceHelpers 专属读集变化但 ordinary 投影相同，也不会重用旧的 ResourceHelpers 成功/失败结论。fallback 的 ordinary 读集如上完整覆盖，policy 本身还单独入 key。

缓存命中仍要求 Layout::valid 及当前 `valid_declaration_prelude`。当前声明全量校验继续覆盖随后可能被过滤的未使用声明；signature/attrs/type 内容也继续绑定。build 入口与验证链没有改动：object hit 在 build.rs 787 行校验当前整个 unit，object patch 路径在 828 行校验当前整个 unit，fresh LLVMBuilder 路径在 native.rs 344 行进行当前 machine preflight，并保留 LLVM 前后验证。缓存键不生成 VerifiedModule、不替代独立语言 Verify，也不跳过现有变换要求的原模型核验。

## 测试的独立性与版本迁移

新增 `layout_projection_reuses_multi_body_edits_but_replays_every_current_operand` 实际发布并加载 recipe，再对变化后的完整当前 Module 运行未缓存 `partition_view_recorded`，比较每个输出 unit 的完整结构；它还确认当前操作数确实变化，并把错误 body 类型交给当前 unit validator 拒绝。oracle 是原 planner 的当前输出，不是复制 key 投影公式。

直接转间接调用用例保留函数地址使 refs 集合不变，同时改变 direct_calls；其 key 必须不同。其余用例覆盖符号地址、External/AvailableExternally、声明/定义、global linkage/initializer 引用，以及 import cost 从可复制到超限后回退原 ordinary planner 的真实输出。已有测试继续覆盖结构损坏、声明隐藏、策略隔离和 commit 后发布。

静态审阅没有运行这些测试；少量未单设的新边界测试（百万 LIR fuel、`.lc1` 等）由上述逐读集论证覆盖，不将其写成已执行的测试证据。

对发布候选完整 tracked tree 使用 `git grep` 查找 `partition-layout-v1` / `ZILCGU01`：无匹配；新版本只有 layout.rs 中 `partition-layout-v2` 与 `ZILCGU02`。未发现需要同步迁移的外部硬编码测试。目录与 magic 同时升级，旧 v1 记录不会被当作新协议读取。

## 非阻断限制

沿用的 `Layout::valid` 在有 imports 时只构造 `zil.fn.{id}` 的 source_names，没有采用 ordinary planner 的 source_name `.lc1` 解析。因此含 `.lc1` import 的 recipe 可能保守失效并回到 fresh planner。这是基线已有的 miss 行为，不会产生错误重放；本候选未扩大该问题，也不能声称改善这类命中率。

projection 新增 references/direct_calls 扫描，净成本与真实 cache 命中收益必须由冻结输入的实际端到端比较判定。源码审核通过不允许抹除失败、扩大采用范围或绕过后续精确发布 SHA 的构建与原协议门。

## 仅测试 ownership 修复追加复核

原发布 `b5dbe03d38ed3c7c9f11547c5d074b2f0f96aec8` 的首次测试编译出现 E0382：测试先将 Cache 传给消费 self 的 pending，再尝试 load。该失败不能写成测试通过。

后续发布 `e9d392caa1e19bc722e276977165f2a7e5b33700` 以 b5dbe 为父，只修改 layout/tests.rs：将测试 Cache 变量改为返回相同 root/key 的 closure，并在 pending/load 三处调用 closure 获取新值。没有改变断言、用例、oracle 或生产语义；修复符合 Rust ownership。

独立 GitHub commit/trees 读取确认：生产 layout.rs blob 仍为 `997e679686a5f9e97310cc90f9256991da28c2c7`，与原已审发布相同；tests.rs 新 blob 为 `ffc6862fbf57b083922cde90d4299ad5af03c0ff`。因此生产算法结论适用于该修复发布；本审核没有代替作者的后续精确 SHA 重编与测试结果。

修复提交：https://github.com/zil-lang/zil/commit/e9d392caa1e19bc722e276977165f2a7e5b33700
