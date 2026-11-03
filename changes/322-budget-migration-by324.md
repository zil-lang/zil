# 322 完整类型位置扫描后的六组旧预算控制迁移

冻结生产基线337d14d01，仅负责root派定的Expand generics、impl202_store_templates、
impl240_generic_enum_budget，以及driver impl200_independent、impl204_review、
impl246_front六个既有target。driver229和其它Wire/partial控制由root单独处理。

322的lifetimes::validate完整AST/类型位置访问使用generics::Budget，并将实际剩余
额度沿aliases::normalize_with_budget及generic展开累计传递。旧输入即使没有显式
life标注，也确实执行了新增扫描，故原固定Expand N不能继续标作未变。不删除扫描、
不重新给预算、不提高默认值，也不把无新trait的旧费用变化归因新trait求解。

本迁移只实测并更新旧固定N与历史注释，保持原源字节、N−1 ResourceLimit、fresh、
冷/暖重复及最近拒绝控制。原费用保留；实际新N及限定结果随后填写。

## 实测固定值

以下在337d14d01上二分实测，并已逐项核N−1 ResourceLimit、N/fresh及默认Core相等。
测量时旧固定N也真实失败；未把源码准入或其它阶段费用一并改写。

| 原控制源/target | Expand work旧→新 | Expand nodes旧→新 |
| --- | --- | --- |
| generics binding | 880→1036 | 301→368 |
| generics argument | 871→1024 | 290→361 |
| generics independent | 1440→1612 | 457→536 |
| generics enum direct | 624→782 | 242→310 |
| generics resource scalar | 288→381 | 144→194 |
| generics resource owned | 554→735 | 279→376 |
| generics resource local | 558→730 | 281→372 |
| 197原状态源（expand202、driver200双控） | 1938→2358 | 原检查未固定node |
| expand202 ordinary Store | 2255→2713 | 原检查未固定node |
| expand240 generic enum | 336→403 | 126→165 |
| driver204 byte capture | 2→97 | 原检查未固定node |
| driver246 Copy helper | 489→608 | 原检查未固定node |
| driver246 enum Copy helper | 665→836 | 原检查未固定node |

driver204另三阶段保持Types196/Analyze2232/Verify2492；driver246两个Types控制保持
420/610。197双target为同一源的重复控制，不叠加覆盖数。新helper/Store等动态边界
原测量代码、fresh及错误类别检查保留。只有六个测试文件中的常量、归因注释和
断言说明更新，不留临时测量helper，不修改任何ZIL夹具或生产代码。

原失败保留：测量时expand202普通Store仍断言2255而实测2713（16/1）；driver200
旧1938断言与2358不符（0/1），driver204旧Expand2与97不符（0/1）。这不是语义
或安全反例，旧输入默认Core及其他阶段通过。最终固定常量后的限定执行待填。

## 固定常量后的限定执行

独立target-budget-migration-by324、jobs1：

- `cargo test -p zil-expand --test generics --test impl202_store_templates --test impl240_generic_enum_budget --offline -- --nocapture --test-threads=1`：分别14/0、17/0、2/0。
- `cargo test -p zil-driver --test impl200_independent --test impl204_review --test impl246_front --offline -- --nocapture --test-threads=1`：分别10/0、21/0、12/0。

六个既有target合计76/0，零忽略/过滤；包括原最近反例、完整旧源码消费者及所有
原N−1/fresh/冷暖控制。检查全部Rust引号字面量，只有两条断言说明文字改变；
ZIL源字符串/片段和include夹具均原样，未用临时测量器替代最终固定N断言。
本地日志为/tmp/budget322-expand-final.log与/tmp/budget322-driver-final.log。

这只是旧预算控制的透明迁移，不新增语言/安全权限、运行表示或产品突破；运行/
内存、维护和编译速度的Rust比较未测，不升级已有product-gates，不属于324自审。
