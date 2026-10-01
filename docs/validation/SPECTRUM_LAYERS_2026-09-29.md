# 0.14.1 频谱图层开关

DRY / WET 图例左侧分别增加蓝色 / 白色色块，点击色块或文字即可切换该图层。显示时色块为实心，隐藏时变成暗色空心框，文字同步变暗。两个图层均隐藏时保留坐标、网格与分频控件。

这两个开关只控制可视化，不改变音频参数、音量或共鸣状态。`ui_show_dry` / `ui_show_wet` 为保存到工程的界面偏好，默认均显示，旧状态缺失字段时也恢复显示。冻结时可以切换，重新显示会恢复保留的整段历史；运行时即使隐藏也继续采集该图层。

UI 侧保存两路已映射的亮度，共 512 × 130 × 2 个 f32，约 520 KiB，另有行间断标记。普通帧沿用增量行上传；开关变化时一次性重着色约 260 KiB 的完整纹理，复用既有配色，不重复运行 FFT 或对数映射。底部实时亮边使用同一纹理，因此同时隐藏/恢复。音频线程路径未改动。

## 验证

- 45 项插件测试通过；包含实际 GUI 色块点击、Freeze 下切换不发送音频参数手势、显示偏好序列化/恢复，以及旧状态默认值。日志：`target/spectrum-layers-tests.log`。
- GPU 预览通过，RTX 4060 Laptop / Vulkan。最小 980×700 窗口下检查双层、仅干声、仅湿声、全部隐藏、重新恢复；历史重新恢复的频谱区域与切换前逐像素一致，各单层和隐藏状态的图像不同。截图与像素检查：`target/spectrum-layers-preview/`；日志：`target/spectrum-layers-ui.log`。
- 插件全部目标的 Clippy `-D warnings` 通过，日志：`target/spectrum-layers-clippy.log`。
- 最终 CLAP 制品检查：36 passed / 0 failed / 0 warnings / 8 skipped，日志：`target/spectrum-layers-validator.log`。
- 模拟宿主中三轮窗口创建、改变尺寸、显示、隐藏、销毁通过；独立音频线程处理 2161 个块，初次激活无额外重启。日志：`target/spectrum-layers-native.log`。格式和 diff 空白检查通过。

本次不以模拟宿主或离屏 GPU 检查代替真实音频宿主工程验收。

## 制品

`target/artifacts/0.14.1/my_spectral_resonator.clap`，13,809,152 字节。

SHA256：`4BCAA8ACF30ABA51535DC8FE02DA06577C0D763F2178A4C77BA73A0E88EF959B`。

常用加载路径为 `target/bundled/my_spectral_resonator.clap`；发布与 0.14.0 备份信息见 `target/spectrum-layers-preview/publication.json`。需要重新加载已有实例。Release 构建仅有 vendored nice-plug 原有的两个 unused-variable 警告。
