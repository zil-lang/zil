# 330 有限trait默认方法（提案）

基线`40fbcca6ef506f3cb8e1d40a4f8995c788627da0`。主树`_zil_design.md`
§18.1及`.task/impl/330.md`为权威；本文先登记接受性与新增费用，待root冻结规范后
修改生产。不新增公共Core/Typed/LIR、Wire、运行时字典或隐式分配。

## 实际拒绝点与最小合同

`static_traits::collect_trait_declaration`、`trait_instances::validate_declaration`
和`trait_bounds::Index::declaration`分别要求trait方法只有Declaration；静态collector
进一步要求impl显式提供每个方法。当前两份原待实现源均保留：
`tests/language-readiness/pending/default-trait-method.zil`（未用定义）与
`tests/language-readiness/recovery/pending/trait-default-method.zil`（实际调用）。

拟开放已有有限名义trait中的普通非泛型方法默认体；trait本身沿323允许的普通类型
参数和直接Copy前提，不开放方法泛型、方法where/life合同、supertrait、关联项、泛型
impl或新的资源接收者资格。Self仍是符号未知类型，只有当前真实trait实例的方法
权限，不因存在impl或实际i32实参获得Copy、数值运算、字段、callable或其它trait。
参数只有声明已有的Copy前提。缺required方法仍拒绝，显式等签名覆盖优先；即使
没有impl、没有调用或所有impl均覆盖，该trait默认体也必须定义期检查。

最小实际消费者不仅是常量返回：

```zil
trait Read<R:Copy> {
    fn read(self:&Self)->R;
    fn delegated(self:&Self)->R { return self.read() }
}
struct Point { value:i32 }
impl Read<i32> for Point { fn read()->i32 { return .value } }
impl Read<i64> for Point { fn read()->i64 { return i64(.value)+2 } }
fn narrow<T:Read<i32>>(x:&T)->i32 { return x.delegated() }
fn wide<T:Read<i64>>(x:&T)->i64 { return x.delegated() }
```

两实例必须分别委托到其自身`read`实现；覆盖`delegated`只替代对应完整实例。
直接调用`point.delegated()`仍按既有多候选规则报歧义，不猜目标。

## 实装接点

新增私有`generic_trait_defaults.rs`复用现有parametric checker。定义期构造真实
trait参数和独立符号Self，显式提供从原声明产生的Bounds：Self有当前名义trait
Reference，参数只有原Copy约束。该入口不得用具体实例签名回填前提，也不把
life binder混进类型变量。普通generic checker旧入口保留原初始化。

Engine建好名义/函数索引后检查每个默认体。具体impl处理中只为缺失默认方法复制
普通AST member，先按该trait完整实际参数替换正文内类型，再按同一实例绑定
`self.method`静态目标，进入既有签名conformance、Core、Types、Analyze、独立
Verify与机器生成。显式覆盖也仍按真实trait声明签名检查。默认方法互调/递归生成
普通静态函数边；不递归展开被调用默认体。无impl的合法递归定义不执行；实例化
每方法最多补一次，运行期递归沿既有普通函数规则。

完整身份包含trait定义模块/声明、全部有序实参（含phantom）、真实Self和成员。
现有`trait_method_names: BTreeMap<Span,String>`不足以区分同源码默认体的多个
实例，拟最小改成`(implSpan, memberSpan)`私有key，并在函数发射/导出中使用
实际impl owner；不伪造Span消歧。原trait/impl可见性、所属和coherence保持。
跨模块默认体的名字先沿已有项目正规化绑定其声明模块，不让实现模块同名helper
捕获原默认体。

拟扩展写集：`generics.rs`的早退、定义检查与具体Impl窄hook；`generic_check.rs`
显式Bounds入口；新私有`generic_trait_defaults.rs`；`lib.rs`私有复合method-map及
发射/导出消费者。已有330卡中四个trait模块和两个driver测试目标继续使用。
与331共享的Engine/checker接点已提前通知作者，由root串行整合；所有life信息保留。

## 费用及验证

新default路径沿当前请求remaining串接模块解析、alias、Engine、静态collector及
发布，不重新取默认预算。无default旧路径不添加新遍历。每次源扫描、成员/索引
访问、完整key比较、替换和正文复制均增长前预费，生成方法受现有函数上限约束。
新增default路径须启用现有Products累计trait费用链；不能因无trait类型参数而落回
旧单模块collector默认额度。`modules.rs`现有三个has_trait_instances预算链接点也需
识别无参default。发现default所需的新增member扫描会显式收费；若无default旧控制
因此发生实际费用迁移，保留原源/旧N并报告，不把检测声称为免费。递归/重复实例
查询仍扣访问费，队列不按调用树扩张。

限定既有`impl162_trait_bounds`和`impl233_modules`目标。拟覆盖原待实现源、常量
默认方法、required委托、覆盖、同Self多实例/phantom、跨模块同名helper与私有身份；
未用错误返回/字段/算术/缺Copy/缺bound、覆盖后的坏原默认体、缺required、错签名、
资源接收者及不支持方法泛型保持精确拒绝。真实消费者走两backend O0/O2；新默认
委托以实际source累计N/N−1/fresh及多实例控制核收费，默认额度不升。只限定测试，
不替代root正式全量及非作者审核。实际结果待实现后填写。

## 基线实际探针

`target330`、jobs1、显式`llvm-api`，现有`impl162_trait_bounds`目标仅过滤`trait330_`：
两份原文件按原字节输入，均为`Unsupported/Types/E-UNSUPPORTED`且Verify0。静态
Expand收集的unsupported feature在Types发布，不能把实际阶段记成Expand。首次
探针误期待Expand，1项失败；修正阶段断言后1 passed/0 failed/24 filtered。两个输入
fingerprint分别为`3ca5160507dc27302ae1a8f71f19ede492cfe974171eb4b204155e881275cb98`
与`29c6d48a71a4a9ad40a142c70d84d5d698f713559e2bf1cc5fa433ae4dadeca2`。
