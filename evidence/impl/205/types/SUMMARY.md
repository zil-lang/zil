# 205 Types 作者收据

受测组合为 clean `589c4d095f189f76844b96d87bc80cd23c7a73e6`，规范 `9813f43ee7c86d786f197f0ed4b8e65938ee354d94b2cdaec858e09e7871ce5e`，schema/Wire27。此为 Types 作者有限验收，不替代集成方整仓、独审、CT/raw/Wire或完整消费者结果。

- final01：源码14、原始Core5、预算1，共20/0；50次O0/O2实际运行全部符合预先冻结的stdout/exit断言，包括正常错误传播的清理。
- final-old20001：三组旧/新具名未调用定义3/0。原138字节保持SHA `5251f83be296a6273532690540cfde459938f39801afc614ba1cbb672df2b05a`；两State/借入输入等原反例不晋升。
- final-legacy01：原非作者旧single-State raw漏检复验1/0，两公开Types入口独立拒绝旧alias在take/refill后使用。未实例化坏正文没有native。
- final-clippy01：Types/driver库与四具名作者/迁移测试target，`-D warnings`通过。
- 精确Types额度：旧197 State从2642变为5293；普通eager88、新replacement4498保持。真实N−1失败、fresh N以及旧N2642失败另存；不与Expand历史1731混用。

实现保留一个F类型绑定，同时按State的两个实际字段与replacement参数记录三个值的位置。反序和非相邻实参必须全量统一具体身份；单F名义名称保持既有生成规则。FnOnce旧字段可消耗后补回；结构Copy是实际闭包属性，泛型Fn约束不授Copy。声明检查沿完整位置和别名代次运行，不依赖某个具体ClosureId取得权限。

源头与参数/RHS错误准备的清理有真实机器轨迹；旧单State新增检查也保留两FnOnce字段、重复exclusive view的兄弟字段窗口、旧lending正例。未新增两个State输入、F/G混合替换、E!State模板或隐式Box/RC/锁。

完整原记录（包括基础设施失败、作者夹具错误、实际漏检及原旧断言Fail）见外置 `205-types-evidence-compact.zip`：2,331,489字节，SHA `b99d72a9957836f61d955153d645f871e17dde2f3fc002e99adc57e7da1209d0`。2,914个原文件173,485,255字节逐字保留、跨文件SHA去重；128个执行产品仅保留路径/大小/哈希。包内有INDEX、MANIFEST与不覆盖恢复工具。文件包核验不计语言测试。

## Rust四目标与突破判断

本片有界安全、真实环境区分与清理已具名验证；完整语言安全未建立。未采用新的运行时包装，但未做同语义Rust运行时间/RSS/尾延迟对照。普通库中可实现State回调替换，是实际组合能力增量；完整维护成本优势未测。逻辑预算记录完整，但本片不构成编译速度优势测量。四目标合取、完整1.4、产品级净突破仍未建立，product-gates与默认limits未降低。

最末并行Expand循环刷新别名修复若追加，将以新的实际提交/交叉点记录单列；本收据不倒标未来生产。
