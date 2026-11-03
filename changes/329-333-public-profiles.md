# 329–333 公共合同与迁移冻结

依据当前规范及用户查缺补漏指令，root冻结以下有限profile，登记不表示实现完成：329记录式Copy枚举构造/显式完整具名match模式，330符号检查的普通trait默认体，331单T:Copy与life binder组合，332真实具体父闭包的thread_scope，以及333版本化大型Copy生成ABI。详细语义只定义于_zil_design.md §15.2/§18.1/§11.1/§19.4及spec接口。

329批准唯一新Syntax AST形状RecordConstructor{path,fields:Vec<RecordPatternField{name,pattern,span}>}，尚不更改Core/Typed/Wire；CT/Hygiene/有界遍历须保留标签，不允许半截擦除。字段源码求值序与声明payload序分开，pub/default/manages及新shorthand/rest/ref权限不开放；直接enum Copy前提须定义/实际实例均检查。

330默认体在未用定义期验证真实Self/bound，生成的body helper保留trait声明模块词法权限，wrapper在impl绑定完整实例。显式实现优先、递归为普通静态边，不能通过具体字段或命中缓存制造资格；module/alias/default/collector/publication共享实际剩余预算。331与330共享入口由root串行集成，保留life元数据与普通类型词法作用域区别。

332允许既有合法具体Fn/FnMut/FnOnce父入口，其自身可有既有合法参数/结果，须恢复完整ClosureDefinition/Function绑定和PassingMode；只有worker固定(i32)->i32、body固定()->i32。未核callable参数/helper入口仍Unsupported。推断必须发现捕获body被移动且消费，不能使Fn复制Once环境；worker静态子图仍不得含线程、地址或资源。没有新线程IR、机器ABI或Transfer许可。

333原a18-by-value已由作者实测O0机器崩溃，O2为资源受限未完成而非证实优化逻辑崩溃。root选择显式版本化大型ZIL生成ABI迁移：受影响生成函数符号加.lc1，默认External契约仍在，不以旧同名符号偷偷改变签名。旧大型机器消费者必须重新生成/链接当前符号；不承诺旧大型ZIL机器ABI兼容adapter。小值和foreign/C回调ABI不变。大参数copy-in与首隐藏out指针、函数引用/普通间接调用、CGU和缓存必须同一映射；尚未接通的特殊消费者在产物前Unsupported，不回巨型SSA桥接或fallback。此决策不增默认额度、不制造借用权限，主树独占公共表示和集成。
