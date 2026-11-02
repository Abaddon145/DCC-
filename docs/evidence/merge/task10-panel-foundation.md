# Task10a/b：三态面板共享层

基线d568d04。前端改动；无新依赖、schema或后端功能。本步实现共享状态/交互组件及复用浮层判断，尚未把学习区或知识库现有布局替换为该组件。

- 纯状态函数覆盖closed/peek/pinned、同侧tab、固定宽度、复位、专注快照和持久化。旧context缺字段沿用既有外观宽度；peek不写成固定展开，专注不会把原固定布局保存为全收起。
- WorkspacePanel保留窄入口栏，首次访问才挂载内容，后续切换/收起保留搜索与滚动。固定可左右拖宽、键盘调宽及双击/Home复位；小窗口仅CSS夹宽，不把临时显示尺寸回写状态。
- peek外点/Escape收起；输入/IME、已处理事件和可见对话框优先。关闭后焦点回到入口，两侧同时临时打开时Escape交给含焦点的一侧。固定面板不被外点关闭。
- 原生浮层可见性判断从NativeLearningSurface原样移至既有workspace工具，旧名称兼容导出。组件仅peek挂data-native-overlay，后续接入时继续由已有几何观察更新原生窗，不用重建播放器。

## 验证

- 状态桩RED六项全部失败，再GREEN六项，0.792秒；`task10-panel-state-red.log` / `task10-panel-state-green.log`。
- 组件桩RED五项失败；初次实现连同现有学习区/偏好32项通过。追加焦点恢复测试RED一项，再补两侧Escape优先级RED两项；共同修复后4文件33项通过，2.74秒；`task10-panel-focus-red.log` / `task10-panel-priority-red.log` / `task10-panel-final-targeted.log`。
- 生产类型检查捕获React键盘事件没有直接isComposing字段；改用nativeEvent.isComposing，再执行完整前端与构建，未绕过类型检查。
- 完整前端 **404项 / 77文件通过**，47.56秒；生产构建退出0、4.18秒，进程35300退出0；`task10-panel-foundation-full-ui.log` / `task10-panel-foundation-build.log`。既有chunk提示保留。
- 后端没有改变，不重复Rust回归；最近已验证的后端基线为归档阶段470通过、2既有忽略，见该阶段证据。本步不能宣称新的真实原生播放、屏幕尺寸或IME人工验收通过。
- 提交前diff/凭据检查排除无关Luca/outputs/tmp。日志均在忽略的release-artifacts/yingliu-merge。

后续10c必须接入真实调用方、偏好和导航过滤，并验证播放会话/编辑器不因面板操作重建。已核对learning尚不在后端workspace context白名单；接入时须显式注册独立上下文，不能借用video键覆盖现有媒体筛选。实际拖宽/原生遮挡/DPI/输入法留Task14，不把组件DOM模拟当真实桌面验证。

用户karpathy-guidelines/Ponytail保持有效；规则复用已有组件和工具，未创建全局面板服务。常规跨文件事实核查用GPT-6 Sol medium，未使用GPT-6 Astra。没有源用户数据、付费请求、推送、合并、发布或安装。
