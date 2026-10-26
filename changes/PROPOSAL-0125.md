# 125 工厂正文组合既有高阶 helper

## 为什么

ZUI组件工厂需以通用初始化、计算和事件预热helper准备状态，然后返回具体拥有handler。现有opaque callable工厂与非逃逸callable helper分别已支持，但Types把工厂正文的helper调用与递归/转发工厂一并禁止。真实探针make→apply(&init, seed)在Types Unsupported，Verify未运行。

## 是什么

只缩小Types直接调用分支中的保守限制：factory_bound存在时仍拒绝callee.callable_return，但不据callee.callable就拒绝。helper仍经过现有call_helper具体环境实例化、正常Call、有限实际调用轨迹与公开合同。工厂返回仍须同一自包含创建身份。

## 怎么做与边界

生产改动只在zil-types/src/lib.rs。Analyze/Verify/backend/schema/默认额度不变；各阶段仍独立核对helper的Hold/Call/Cleanup与factory的具体身份/捕获清理。允许组合现有能力不会给普通Call制造新的权限。返回借入capture、factory递归/转发、多hidden身份、helper循环/递归/转发、超过现有两次调用、FnOnce重复调用仍按原边界拒绝。

接受性扩大先提交本提案。新增可接受程序的实例化、生成CFG、接口和复制按既有预算收费；不声称扩展前后同一新程序有相同预算，因为扩展前程序被提前拒绝。用实际Types和Verify N/N−1控制记录新路径受限。

验收：三种helper调用模式与返回Fn/FnMut/FnOnce组合；跨模块公有接口；创建handler后helper预热再返回；两个工厂运行时实例相互独立；拥有初始化临时与返回环境析构顺序、错误值与未调用清理；最近源反例；完整raw与wire合同篡改；O0/O2原生与实际清理。

## 施工发现与内部准备时序（实施前补充）

首个候选8fa6c79d去掉表层误拒后，发现single/project入口都在opaque工厂prepass结束后才安装helper模板，导致实际新组合报`callable template definition is absent`。原结果、源快照及有原始阶段产物的单独重放分别保留。

将既有有界helper安装移到工厂prepass之前，一次安装，不复制第二套模板。该私有lookup快照在整个工厂prepass期间保留未解析opaque返回，因此helper正文不能借工厂检查顺序隐式取得其它工厂的结果。直接调用分支对尚未解析的opaque结果明确拒绝（含make→helper→make间接依赖）；现有直接工厂递归/转发仍拒绝。所有工厂prepass完成后，把已核具体Closure结果同步到私有表，再检查普通调用者。普通调用者中的helper原有的合法工厂使用不因此被全局封禁。

同步使用独占Rc私有表、只替换已经解析为Closure的结果；不对全表做第二次深克隆。新同步按名称长度加固定工作收费，初始快照按实际未解析类型走既有完整计费。改变只限Types内部时序与预算工作量，默认额度不变；不声称旧特定低额度程序的接受性完全不变。新增Types N/N−1涵盖安装和同步路径，且保留旧有限模式、factory逃逸和callback合同反例。

写集新增zil-types/src/program.rs与helpers.rs，由root在主树工单追加；无公共schema、Analyze/Verify、后端或规范变化。
