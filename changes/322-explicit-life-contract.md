# 322：显式 life 与真实返回来源上界

本片从已备份基线 fe4cb19ae 接续，不继承未恢复的 301–312 成果。依据
ZIL-ORIGIN、ZIL-CONTRACT、ZIL-MODULE、ZIL-TRAIT 及 316 的真实探针，贯通
普通 Direct 函数的一层共享 Copy/slice 地址输入与结果，以及既有具体 inherent
Self 接收者。life 只约束来源，不授予 Copy、同一性、apart、独占或延寿。

公共合同由 root 实施：CoreFunction.life 保存 CoreLifeContract 的函数内 binder
名字、有序真实参数 life 索引和可选结果 life；最终参数序包含 receiver 0。
TypedFunction.reference_return_bound 保存显式结果允许的升序唯一参数索引集合，
并由 ScalarFunction 对外绑定声明上界。Types 从 Core 声明生成该上界；Analyze
与 Verify 各自从最终 CFG/调用固定点重建真实来源，逐 Return 核其为声明集合的
子集。候选 Hold 不授信。接口继续发布独立核出的实际来源，上界与实际来源分别
进入签名/合同/缓存身份，实际来源变化继续触发既有失效链。

没有 life 的旧函数、返回位置省略 life 的函数继续正文来源推导，保持原多输入
select 接受性。显式返回 life 必须有非空实际共享地址参数集合；无标注输入不被
擅自归入某个 binder。两个不同 binder 的来源即使某次调用 actual alias，也不能
在定义期互换。同 binder 多输入表示允许来源并集，不证明对象相同。

life-only 声明不产生按 life 名称的机器单态化。原 AST 名字须保留到 Core 合同
构造后才擦除；所有其它类型位置不得借 type_ref 静默丢弃标注。初片不开放混合
普通 T/F 泛型、fn pointer/HRTB、where life、别名 life binder、借入存储、可变
life、Product/资源头借入返回、外部 ABI 或异步。Self 正规化保留原 life/receiver
对应关系；CT source 重建、driver CT query 克隆和预费必须保留合同。

schema/Wire33 与公共 None 构造迁移由 root 管理，无新运行时布局、分配或权限位。
只做已有 cross_function_borrows 目标内源/raw/Wire、跨模块、CT、O0/O2 与
N/N−1/fresh 限测；正式全量、Windows、Rust 产品比较未运行不得记为完成。

首个生产消费者 checkpoint 已完成：合并 root life33 公共提交 8e46a4615 后，
原 pending life-parameter-return 原源 Verify/native、同 binder union、双 binder
精确来源、Self receiver0、未调用错误定义、最终固定点后的 raw/Wire 上界反例、
CT 重建及 semantic-v3 warm interface 共 4 个新增限定测试均通过。命令为
`source /workspace/.zil-env/env.sh` 后设 target322/jobs1，运行
`cargo test -p zil-driver --features llvm-api --test cross_function_borrows explicit_life --offline -- --nocapture`。
尚未以这一次限定结果宣称整单完成；预算、旧借用回归与独审继续进行。

所有 AST 类型位置借用现有有界 Visitor 在模板移除前扫描；显式头信息才可经过
signature_type 降到地址布局，其它 type_ref 遇 life 保留 Unsupported 而非静默
擦除。新扫描进入同一泛型剩余预算，不提高默认限额；原 private fresh-budget
wrapper 已无调用，经 root 授权、323 确认无依赖后删除，保留累计预算正规化核。
缓存格式/键/目录升级 v3，显式编码上界并与恢复 Typed 函数逐一核对；每次命中
仍重新 Verify。root CT source clone/preflight 与本片 driver CT collector 分别保留
Core life 元数据，不把缓存、哈希或 CT 重建当成已核权限。

环境第二次断开前，借用目标已实际 18/18 通过，新增原源逐阶段 N 为
Expand91/Types95/Analyze702/Verify985，各核 N−1 ResourceLimit 与 fresh N。
该测试增量于环境恢复后先 checkpoint；它尚未最终验收。既有 semantic_cache
目标当时 0/7（候选查找空/路径不存在），Clippy 因环境进程创建失败未启动。
现已只读确认该目标两处硬编码 semantic-v2，而生产目录及格式已为 v3；按 root
授权继续迁移测试定位并保留 hit/tamper/invalidation 与旧 v2 不复用控制。
接口身份探针首轮把待比较函数写为 private，比较到了空 exports，出现 17/18；
修正为 pub 后 18/18，不改变生产检查或上界。

## 最终作者限定交付（schema/Wire33）

生产/测试冻结提交为 `e86e79a0a0730e7533ceae2dcee6e4d11c59ce68`；前置
checkpoint 为 `f71f8049b`、`8acbada61`，公共字段由 root 的 `8e46a4615`
提供。本节仅记录已执行范围，不把 root 的 schema34/thread 组合当作本次受测。

```text
source /workspace/.zil-env/env.sh
export CARGO_TARGET_DIR=/workspace/.zil-env/target322 CARGO_BUILD_JOBS=1
cargo test -p zil-driver --features llvm-api --test cross_function_borrows --test semantic_cache --offline -- --nocapture
cargo clippy -p zil-driver --features llvm-api --test cross_function_borrows --test semantic_cache --offline -- -D warnings
git diff --check
```

```json
{
  "cross_function_borrows": {"passed": 18, "failed": 0, "new322_tests": 7},
  "semantic_cache": {"passed": 8, "failed": 0, "retained_tests": 7, "new_version_boundary_test": 1},
  "new322_native": {"backend": "Clang", "optimizations": ["O0", "O2"], "executions": 12},
  "new322_source_rejections": 13,
  "new322_core_metadata_rejections": 5,
  "new322_typed_attacks": {"cases": 6, "analyze_rejects": 6, "verify_raw_and_wire_rejects": 12},
  "life_ct_and_warm_hit": "Pass; one fresh Verify per warm request",
  "interface_identity": "declared upper bound changes signature; actual source changes verified contract",
  "original_phase_N": {"expand": 99, "types": 95, "analyze": 714, "verify": 993},
  "each_phase_N_minus_one": "ResourceLimit",
  "each_phase_fresh_N": "Success",
  "targeted_clippy_D_warnings": "Pass",
  "diff_check": "Pass",
  "formal_full_suite": "NotRun by author",
  "schema34_combination": "NotRun by author",
  "native_windows": "NotRun",
  "rust_product_comparison": "NotRun"
}
```

补齐实际重复 shared Copy/slice 资格遍历与源 binder/header 查找的预费后，原源
N 从 checkpoint 的 91/95/702/985 变为 99/95/714/993；Types 增费未改变其主导
边界。默认限额不改，没有以保留旧数跳过工作。无 life 的正文推导、选择返回、
递归、实际最后使用和旧悬垂/冲突反例均随原目标完整复验。

缓存旧 0/7 已核实为两处测试定位仍指 semantic-v2；仅迁为生产 semantic-v3，
原命中、篡改、失效、权限、取消和发布断言全部保留。新增控制将有效 current
候选放到旧 v2 目录，要求当前路径重新从源编译且旧文件不改；又将 current 目录
中的 semantic 载荷头改为 ZILSEM02 并重算外层完整性摘要，要求旧格式不能命中。
它不是伪称归档的 v2 完整载荷。首个控制版本误把外层 ZILRAW02 头当 semantic
头导致 7/8；修正为载荷位置后 8/8，不改生产拒绝。

Rust 四目标：显式 life 现在有可执行来源合同，减少普通库表达缺口；内存安全
依旧取决于真实来源与独立 Verify，未引入运行时 life 表或新分配。运行/内存、
维护工作量及编译成本未作 Rust 配对，N 边界不是速度优势，产品级突破仍未建立。
混合 T/F、可变 life、HRTB/间接调用、借入存储及一般 life/Self/trait 组合继续
未完成；完整语言、B0、ZUI 和产品门不由本片升级。
