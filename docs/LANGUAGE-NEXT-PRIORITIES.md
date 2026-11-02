# ZUI语言前置：普通拥有Sum之后

本次以最终R `15498f14af8f11d3a3bf4b198fe69208be5913ce`、PR17实际main `8b2ae1fdc88b4e1ae8dc73162b13a425d0f6f1ad`及当前规范为准。239–244完成有限普通拥有Sum验收；227/235/236/237/238此前完成的同步组合保持。最新五个探针在同一最终R CLI执行，原源与报告见`evidence/planning/owned-callback-finalR/`；这五次只读检查没有native执行。

| 能力 | 当前状态与下一义务 |
| --- | --- |
| Session/Store/泛型Product/整体替换/defer | 已有有限同步闭环，继续复用；不重派已有集合、ID关系和固定回调家族。 |
| 拥有Enum、Product内Sum、含Store Product错误载荷 | 本批有限已支持；真实tag、准备后提交、消耗式match与实际清理已验收。拥有泛型Enum、capture、StoreElement、数组及任意ErrorUnion槽仍各有边界。 |
| `dispatch<F:FnOnce(Event)->Reply>` | **下一单一主目标。** 最终R三个独立原源分别确认公共dispatch、具体closure拥有Event参数和拥有Reply结果仍在Types Unsupported；不能只去掉一个签名门。 |
| `keep<F:FnOnce(i32)->i32>(f:F)->F` | 最终R仍在Expand Unsupported。需要真实输入环境到返回同一值的来源关系；新建包装闭包不能代替它。已有State<F>完整转交和Store控制已到Verify成功。 |
| 通用库抽象与生成 | 有限静态trait、Copy泛型和既有非泛型生成可用；拥有泛型Enum、关联项/default trait方法、lending/dyn和完整Type/Syntax仍需各自实际合同。Type/Syntax接续声明式ui!入口，不作为手写同步API的重复前置。 |
| 外部长期callback生命周期 | retained userdata仍需稳定注册根、失败、重入、停止准入、注销排空/no-future及最终回收；普通defer或active=0不能替代外部静止证明。平台、挂起与并发按真实义务推进。 |

下一批先冻结一个真实三模块API：model拥有Event/Reply/Fault，dispatch提供公共泛型调用，consumer将输入完整转交并取得完整拥有结果。一起覆盖正常/错误返回、未调用、后续参数准备失败、环境剩余清理和O0/O2实际分配失败。Fn/FnMut/FnOnce描述环境访问，消费每次新Event不能自动消耗环境或降低其可重复性。

Concrete closure、公共合同、Analyze、独立Verify、机器ABI及当前请求费用同批贯通。数据参数/结果权限与capture、State/Store、FFI等既有profile分开核验；不导出callee私有Created，不隐式引入Box/RC/锁。未调用泛型定义和实际具体函数应有相邻合法/非法对照，避免把组合误拒留到整仓之后。

`NEXT-CONTRACT-DRAFT.md`仅是后继派工准备，尚未冻结或实施。已有源码、机器执行与原失败继续保留；作者、独审、PR/main重放不相加。完整1.4/ZUI、Rust四目标合取及产品净突破尚未建立，product-gates不变。
