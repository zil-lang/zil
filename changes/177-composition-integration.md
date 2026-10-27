# 177：Store、元素窗与具体回调字段组合

## 为什么

174/175和176在同一Package准入、递归清理和借入分派处相交。各自通过不证明同一个State可以同时持有Store和具体Closure，必须组合并验证实际来源、环境位置和失败清理。

## 改什么

保留174直接Managed Store字段及原Store叶限制，叠加176直接自含Closure字段；保留175动态窗口借入与176回调字段借入各自独立分派，不保留公共阶段的Unsupported占位臂。资源清理保留实际Store drop helper并加入具体Closure drop路径。借入重建同时登记86和87；Package捕获同时保留Store、拥有数组和Closure字段禁入。

## 怎么验

私有组合吸收精确174/175/176源码。六文件、每入口五模块消费者在同一个State<F>中跨函数转交、重复FnMut调用、运行时Store读取/替换/扩容及字段取出；独立预置Drop/失败分配期望。作者定向套件和独审反例在最终组合重跑；正式workspace由root统一。该提案不扩展schema21有限合同、不提升产品门、不更改预算。

## 首次消费者失败与现有边界

ccb9a2f首轮在Expand拒绝泛型State.create内的store_new，原始consumer01保留，0构建/0原生执行。现有parametric checker未登记Store原语，176有限合同不扩展该协议。将具体Store的empty/grow封装在普通collection helper；泛型函数仍真正拥有、取出/替换Store字段并传递完整State<F>，动态读和分配错误检查仍真实执行。直接在泛型函数体调用Store原语仍列为下一步缺口，不能将此改写标成原输入已通过。
