# 183 直接资源成功载荷的拥有模式

## 为什么

1.4§8.1把拥有组合组织为按实际tag选择的Sum；现有实现只允许Copy/完整Package错误联合。`void!ByteBuffer`即使明确`result(create)`也未把成功载荷解析为Managed，阻止普通库返回可失败构造的实际资源。把资源额外包进Package不能算原形态完成。

## 本片的明确解释

对有限Copy错误E、已支持直接资源R，函数返回`E!R contract { result(create) }`或`result(give)`时，拥有模式作用在成功载荷R；IR为`ErrorUnion { error:E, result:Managed<R> }`，外层是普通非Copy Value。失败分支没有R的责任，成功分支恰有一个R载体。默认未声明拥有模式仍不从Desc制造Managed。create/give保持既有真实来源和一次转交规则，不由标签字符串制造权限。

`?`按真实tag取出对应载荷，成功给出实际Managed值，失败只传播E并清理本激活仍负责的对象；丢弃联合只清理真实活动载荷。读取或复制不能代替Take。cleanup先转责任再实际free，不能错误分支释放未初始化成功槽，也不能复制管理标志。

首批实现ByteBuffer及有限用户资源；Store的同构能力只有真实全链测试通过才列入。资源错误侧、任意嵌套Sum、递归拥有、lending逃逸等不由本解释自动开放。

## 兼容与接口

复用schema22现有Type、PassingMode与Make/Take/DropErrorUnion指令，无新wire标签。公共规范和interfaces由root单写；Types/Analyze/独立Verify/Lower/机器路径必须同批支持。原Copy和Package联合模式保持。

## 显式拥有参数补充

为了接收同一完整拥有联合，`give x:E!R`在本片有限形状下把成功载荷R声明为Managed，外层仍为普通非Copy联合Value。实际IR参数为ErrorUnion(E,Managed<R>)，不把整个Sum套入Managed。调用者按既有Value规则移动已形成的联合；`give union`不是另一种可选Value移动写法。默认`x:E!R`不改变R的Desc类别，不能隐式接收此拥有联合或由描述生成管理责任。错误tag传入没有R责任，成功tag转交唯一R责任。该解释与返回侧对称，具体实现仍需独立raw/Wire核验与原生清理证据。

## 最近反例与验收

原159直接ByteBuffer成功载荷探针、跨模块create/give、真实分配失败、错误传播、丢弃成功/错误、NLL与源载体移动、O0/O2精确Drop/free、raw和Wire伪造tag/模式/重复Take/漏清理。未声明模式的Desc返回、give描述别名、同一载体两次移入必须拒绝。预算不改，先提案后生产，独审绑定精确提交。
