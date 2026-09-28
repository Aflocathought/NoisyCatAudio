# 0.13.1–0.13.3 分频引导线与输出布局

## 0.13.3 使用旋钮侧边空间

用户进一步说明希望曲线占用旋钮顶部以下的一部分高度，而非扩大上方整条空白。上方额外留白从 54 缩至 18 像素；弯曲起点从控制行上方 5 像素下移到控制行顶部以下 14 像素，利用控件侧边空隙完成 S 形过渡。端点仍与旋钮圆形底边对齐，保留五次 smoothstep 和两个增益值的两位小数显示。

下方旋钮、参数页及页脚坐标保持不变，减少的 36 像素还给频谱。本次仅调整 UI 坐标，未修改参数或音频处理。

现有编辑器测试 8 项及 GPU 预览通过，已检查默认布局、Routing 和最小窗口下分频最小/最大值，曲线不遮住标签或数值。Clippy、格式与空白检查通过；最终制品 CLAP validator 为 36 passed / 0 failed / 0 warnings / 8 skipped，模拟宿主窗口生命周期、数值输入和空格消息检查通过。日志为 `target/guide-inset-{tests,ui,clippy,validator,keyboard}.log`，截图和发布元数据位于 `target/guide-inset-preview/`。真实 Bitwig 未验证。

制品为 `target/artifacts/0.13.3/my_spectral_resonator.clap`，稳定路径仍为 `target/bundled/my_spectral_resonator.clap`；哈希、大小和旧版备份见 `target/guide-inset-preview/publication.json`。

## 0.13.2 调整

引导线预留区域由 26 增至 54 像素，端点从旋钮中心高度下移至可见圆形底边。曲线由三次改为五次 smoothstep `6t⁵−15t⁴+10t³`，以 48 段绘制；横向位移在两端的一阶、二阶导数均为零，平滑连接竖直线段。弯曲部分仍完全位于控件上方，不覆盖频谱历史或数值。

布局同步为增加的 28 像素预留高度：频谱略微缩短，下方旋钮、参数页和页脚的位置保留。扩大窗口时新增高度仍用于频谱。

Wet level 与 Routing 中的 Output gain 固定显示两位小数，例如 `12.04 dB`、`0.00 dB`，静音保留 `−∞ dB`，接近零时避免显示 `-0.00`。仅格式化非编辑状态的字符串；进入编辑仍使用未舍入的实际值，输入和保存不受两位小数限制。

0.13.2 验证记录：现有编辑器 8 项测试、单独 GPU 预览、Clippy、格式与空白检查通过；已目视检查默认布局、Routing 和最小窗口下分频两端的曲线、端点与小数位。最终制品 CLAP validator 为 36 passed / 0 failed / 0 warnings / 8 skipped，模拟宿主窗口生命周期、精确输入和空格消息检查通过。日志为 `target/guide-spacing-{tests,ui,clippy,validator,keyboard}.log`，图片和发布元数据位于 `target/guide-spacing-preview/`。未验证真实 Bitwig，也未改动 DSP。

0.13.2 制品为 `target/artifacts/0.13.2/my_spectral_resonator.clap`，稳定路径仍为 `target/bundled/my_spectral_resonator.clap`；哈希、大小和旧版备份见 `target/guide-spacing-preview/publication.json`。以下保留 0.13.1 的设计与验证记录。

## 0.13.1 初版

Low / Mid 固定在频谱下方最左侧，Mid / High 固定在最右侧。Middle dry/wet 控件本身对准整个频谱的中央，Wet level 紧挨其右侧。Output gain 移入 Routing，与 Main output 相邻：它调整最终输出电平，放在该页比控制音色的 Resonance 页更符合信号链含义。

## 连接线与交互

两侧控件内侧各有细线连接频谱底部对应的分频位置。末端以三次 smoothstep `t²(3−2t)` 构成 S 形平滑过渡，两端保持竖直切线。弯曲部分位于频谱与控件之间的窄区域；频谱历史内继续显示准确频率位置的垂直线，避免将弯曲部分误认为不同时间的分频位置。

连接线不参与鼠标命中测试，保留频谱分频线的水平拖动、Shift 细调、旋钮拖动与精确数值输入。线条平时低亮度，悬停对应控件或悬停/拖动频谱分频线时强调。上下布局在切换参数页时保持固定；增加窗口高度仍全部用于频谱。

只调整 UI 布局和绘制，不更改参数 ID、范围、保存值、自动化、DSP 或 FFT 重启逻辑。隐藏的 Output gain 继续生效。每条连接线只在 UI 线程绘制固定数量的线段，不进入音频回调。

## 验证

- 现有编辑器测试 8 项通过，覆盖三个窗口尺寸和四个参数页、窗口增高时频谱占比、分频拖动手势、精确数值输入、慢速整数旋钮、隐藏参数保留、FFT 设置重启与帧率统计。日志 `target/crossover-guides-tests.log`。
- GPU 预览测试单独通过，使用 RTX 4060 Laptop / Vulkan 和生产 egui/wgpu 绘制路径。增加 980×700 下分频最小/最大值的预览配置，检查曲线不会覆盖控件或数值；默认与增高窗口也已检查。日志 `target/crossover-guides-ui.log`，PNG 位于 `target/crossover-guides-preview/`。
- Workspace/all-target/all-feature Clippy `-D warnings`、格式和 diff 空白检查通过。release 构建仍有 vendored nice-plug 原有两个 unused-variable 提示。
- 最终 CLAP 制品验证：36 passed、0 failed、0 warnings、8 skipped。日志 `target/crossover-guides-validator.log`。
- 模拟宿主的真实 CLAP 编辑器创建、显示、隐藏及销毁通过；针对新数值行位置更新测试点击坐标，数值输入到 300 Hz 成功，9 条预期空格消息返回测试宿主，输入时正确捕获。日志 `target/crossover-guides-keyboard.log`。

未操作真实 Bitwig。本次是 UI 调整，未重复 FFT/DSP 的离线 Bounce 矩阵；其证据见 [0.13.0 记录](FFT_SETTINGS_2026-09-28.md)，不将模拟宿主检查视为真实 Bitwig 验收。

## 制品

版本副本为 `target/artifacts/0.13.1/my_spectral_resonator.clap`，稳定加载路径为 `target/bundled/my_spectral_resonator.clap`。SHA256、文件大小及旧版备份路径记录在 `target/crossover-guides-preview/publication.json`。重新加载现有实例才会载入新版本。
