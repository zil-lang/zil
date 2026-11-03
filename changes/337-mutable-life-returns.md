# 337：life-only 普通函数的可变借用返回

规范已由 root 在 `c4da43441b03661e61d0087000a13dbbe7a2a75a` 冻结；
基线生产源码为 `e5a4d51d844e29c4f4f5184eaff5704b1b27dd24`。
本片只实现 `_zil_design.md` profile337，不改变公共 schema/Wire34、ABI、
默认额度或产品门。

## 实际基线与有限差异

源面使用 `&'a mut T`。实际 B CLI 对 life-only 和 CopyT/life 的 mutable
签名均在 Expand 返回 Unsupported。无 life 的 mutable forward、重借、
两实参精确来源、slice tail 和 Copy record 字段投影已有 Verify 成功；
其中四份控制源的 Clang/API × O0/O2 共16次构建/执行均打印42、exit0。
这些是已有能力，不重记为337新增。

本片开放 life-only 普通 Direct 函数及具体 inherent 方法的一层有限
Copy/slice 地址输入和返回。life 标签只声明真实输入位置的来源上界，
共享和独占输入可以同标签。真实 mutable 返回仍须从合法权限产生；
标签不制造权限、apart 或借期缩短。保留 Core binder/dense map、Typed
返回位置上界，Types 检查结构，Analyze 和 Verify 从各自实际 CFG 来源
核验上界，Verify 仍独立重建独占窗口。调用方继续使用实际来源摘要，
未实际返回的伴参不能被声明上界误当成持续借用。

CopyT+mutable life 保持明确 Unsupported。实际既有无 life 未用模板：

```zil
fn bad<T:Copy>(x:&mut T)->&mut T{
    let a=&mut *x;
    let b=&mut *x;
    *a=*b;
    return x
}
fn main(){}
```

在 B 为 Verify Success；同一模板被实例化则 Verify/E-BORROW-CONFLICT。
这是未用定义符号独占检查的 KnownIncomplete，不是已执行源码 UB 的证据，
也不能靠实例化替代定义期证明。原源作为未来回归夹具保留。本片不修改
普通 generic 独占检查，不重做331共享 CopyT/life。

## 交付责任

在既有 cross_function_borrows 目标补真实源/原生、错来源/局部逃逸、共享
升级、重叠实参/窗口及清理近邻；原始 Core 和 Typed/Wire 的声明、权限和
来源伪造须独立拒绝。跨模块、CT 重建、source/live/restored cache 必须重核
声明、实际来源和当前调用窗口。新路径给出累计 N/N−1/fresh，旧精确费用
控制保留；不以删除安全检查或调高默认额度迁移失败。

本文件先于生产编辑提交工作区；后续结果按实际命令和精确提交追加。
完整语言、B0/ZUI、一般 lending/异步和 Rust 四目标仍未完成；没有新增
运行/RSS、维护或编译性能比较，也没有产品级净突破结论。

## 独立审核发现的生产者责任

Astra 在 checkpoint `dbad2f8ce6fd565be9388cc83b3f4f75c0cf0f98` 指出：
声明上界可以包含共享输入，但 mutable 返回的实际 CFG 来源必须全为独占
输入。原 Analyze 仅核上界，raw 将参数及其 Load 类型改共享、保留 mutable
Reborrow 时仍能生成候选；Verify raw/Wire 已独立以 E-VERIFY-ADDRESS 拒绝。
本片补 Analyze 实际来源模式核验，并对额外根访问累计收费；混合模式上界
中的未返回共享输入不被机械拒绝。Analyze 不是全套 raw 指令类型核，单纯
返回声明/值模式不一致仍由 Types/独立 Verify 的 E-VERIFY-RETURN 拒绝。

## 作者限定完成记录

实际受测生产/测试提交为 `51788f2b95d22394967ef102ad68b8976135596c`。
先 source `/workspace/.zil-env/env.sh`，设专用 target337、TMPDIR
`/workspace/.zil-env/tmp-batch336/337`、jobs1、debug0/incremental0，执行：

- `cargo test -p zil-driver --test cross_function_borrows --test generics --test semantic_cache --features llvm-api --offline -- --nocapture`：37/16/8 Pass，零失败/忽略/过滤；其中337新增8项。
- `cargo clippy -p zil-expand -p zil-types -p zil-analyze -p zil-verify -p zil-driver --test cross_function_borrows --features zil-driver/llvm-api --offline -- -D warnings`、`cargo fmt --all -- --check`、`git diff --check`：Pass。
- 新10份 source 的 Clang/API × O0/O2 共40构建/运行打印42、exit0，包含真实 defer 效果。原 forward 另由精确 clean CLI 在四配置重放，backend 匹配、fallback=null；两组分计。CLI 构建始末同一 clean51788，二进制 SHA256 为 `15bd3dc8cec04668c84d7bb4ca6f9ae4518ffc8320e2f90eceeb613e2b9d0746`。
- 新 select 的 N 为 Expand143、Types124、Analyze1178、Verify4818；各 N−1 为 ResourceLimit，fresh N 完整相同。原322的99/95/714/993与331的757/1229/205/1369均保持。Analyze新增实际权限根检查由1177变1178，未提默认额度。

完整紧凑命令、阶段差异、原失败与缓存/raw/Wire观察见
`tests/language-readiness/mutable337/validation.json`；无大 raw 归档。
旧测试中原 `&'a mut i32 -> &'a i32{return p}` 字节保留：开放 header 后
它仍缺显式共享重借，实际 Types/E-TYPE-MISMATCH；没有把旧 Unsupported
当作新的来源/权限证明。第一次 fixture 失败和独审实际发现分别记录。

Rust四目标反思：真实源、raw/Wire、两套独立来源重建及mutable实际权限根
已形成有限安全证据，完整语言安全仍未完成；机器继续既有地址ABI，无新隐式
分配/RC/锁，但仅核行为，未做同语义Rust运行/RSS比较；普通mutable helper和
inherent投影现在可表达显式life关系，框架维护成本尚未测；新增根访问真实收费，
没有编译速度优势结论。本片实质工程增量是原Unsupported的具体mutable life
能带声明来源和独占权限直到真实机器执行，尚非产品级净突破。既有product-gates
字节及B0 Doing、完整语言/ZUI NotSatisfied、产品/Rust成本NotRun保持。

作者未跑正式全量、reference整仓、不合入、不push；非作者精确审核与root共同
集成裁决另记，本片不代替父任务Done。
