# 320：普通 Struct 直接 Copy bound 的实际消费者

## 提案

原 `pending/ct-type-generic-record.zil` 在 CT 成功后仍被运行时 Expand 的聚合
bound 守门拒绝。本片仅开放普通 Struct 的直接保留 Copy bound；不以删守门或
删除运行时声明绕过原输入。where、名义 trait bound、Package/Resource 新 bound
和一般约束求解不随本片开放；规范窄扩展由 root 发布。

复用现有参数 bound 索引及 check_arguments，在定义期对每个书写的嵌套 Struct
应用检查完整 arity 和所需 Copy 事实；未使用的定义也不能等待某次实际调用来
证明自由 binder 满足 Copy。实例化仍按实际实参逐个核验，phantom 参数不能因
未出现在字段中被跳过。结构 Copy 扫描也必须核嵌套 Struct 的全部 Copy 实参，
避免通过只扫布局字段漏掉 phantom bound。保留完整名义/类型实参身份和旧限额；
新工作只在含新 Copy 约束的路径计费，不新增 IR/schema 或隐式分配。

正反例覆盖原 CT record、普通运行时/嵌套/phantom、多实参、非 Copy 实参、
未用非法定义、名义异型和边界；限定测试写已有 generics.rs。实际结果待执行，
不继承 314 的 CT 成绩，不宣称完整语言或产品门完成。

## 本轮实际检查

规范采用 root 发布的 `_zil_design.md` 320 条款（575b05b98）：仅约束具名
binder，允许其它无 bound 参数，不据此推断整体 Struct 为 Copy。

独立 target320、jobs1、Rust 1.99.1、clang 23.1.4：
`cargo test -p zil-driver --test generics copy_record_bound --offline -- --nocapture`
为 4/0、8 filtered。一个运行时组合源经 O0/O2 返回17；另2个未用合法定义通过；
15个形状/phantom/未用义务/名义身份/关闭入口反例按实际阶段拒绝。新嵌套输入
Expand精确 work N=740、旧无bound控制N=255，各N−1拒绝、N及fresh通过。
首次测试为2/1：名义异型已在Expand正确拒绝，测试最初误期望Types诊断；
修正为实际E-GENERIC-ARGUMENT，原源保留，未改生产规则。

随后按授权合入314未验收候选2190a9179做组合，
`cargo test -p zil-driver --test generics --offline -- --nocapture` 为13/0。
原CT record文件字节原样保留（SHA256
`fb52945235cc9bbb7dbb4c469666d1d589fc0d05290d9613efec4c1e7e2c6b49`），
与返回i64的另一CT分支各O0/O2原生退出0；运行时声明均保留。
加上普通组合源，本片新增3源共6次原生执行。最后增加零长度地址数组和资源
phantom反例后，限定 `copy_record_bound` 为5/0、8 filtered，拒绝总计17个。
`cargo clippy -p zil-expand --lib --offline -- -D warnings` 和 `git diff --check` 通过。

Rust四目标：本片有限结构Copy义务、未用定义与实际phantom反例有实证，完整安全
未建立；沿旧内联记录表示且原生结果通过，未测Rust运行/RSS；原CT记录与普通
泛型wrap自然组合，框架维护收益未测；共享预算有740/739/fresh与255/254/fresh，
无编译提速结论。实质突破为原CT record不删运行时声明即可进入Verify及原生，
不升级B0、完整语言/ZUI或产品门。未跑正式全量、Windows、参考后端或性能比较。
