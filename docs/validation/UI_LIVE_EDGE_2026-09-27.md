# 0.10.2 底部实时频谱亮边

日期：2026-09-27。用户截图圈出翻转后出现在频谱最上沿的细亮线，希望在底部对应位置显示。

## 原因与改动

旧绘制将 128 行环形纹理的完整周期映射到图内，顶部与底部恰好采样同一个滚动游标。最新一行可通过线性过滤混入顶部，产生一条不属于旧历史的亮线。

显示纹理现在容纳 130 行，可见时间窗口仍为 128 行，即约 4.27 秒。顶部从旧数据的像素中心起采样，额外两行隔开滚动插值、过滤范围与最新数据，消除回卷到顶部的亮线。保持两段 UV 绘制，不复制整个历史纹理；新增 GPU 存储约 4 KiB。

底部两逻辑像素固定采样最新一行的像素中心，横轴与瀑布图完全相同。蓝/白亮度分别来自当前干/湿声频谱；静音频段保持暗色，Freeze 同时冻结此亮边。没有添加模糊、人工峰值或新的 FFT，音频 DSP、参数、延迟与后端帧率调度均未改变。

## 验证

- 编辑器相关 8 项测试通过，其中 GPU 离屏测试使用真实生产绘制路径；`target/ui-0.10.2/ui-tests.log`。
- 新增四种 GPU 像素回归场景：所有旧行暗色、仅最新行白色，覆盖游标起点、半行插值、环形跨界和中部位置。实际读取图顶前两行像素，确认最新白行不会出现在顶部；同时确认底沿仍显示白色亮边。诊断图 `target/ui-preview/edge-*.png` 为人工回归输入，不是实测声音。
- 常规 `target/ui-preview/tab-0.png` 等预览仍使用 DSP 扫频/噪声，已检查底部亮边与频率对齐。GPU 为 NVIDIA GeForce RTX 4060 Laptop GPU / Vulkan / 595.97。
- 本轮只改显示，没有重复全部 DSP/Bounce 测试或四档性能基准。实际 Bitwig 窗口需重新加载后确认。

- 格式化、全目标/全功能 Clippy `-D warnings` 通过；release 仍保留原 vendor/nice-plug 的两个 unused-variable 提示。
- 最终 CLAP validator：36 passed、0 failed、0 warnings、8 skipped，日志 `target/ui-0.10.2/validator.log`。
- 60 FPS 档三次打开、缩放、隐藏、关闭重开通过，尺寸 1120×780 → 1260×860 → 980×700，独立音频线程处理 2427 块。日志 `target/ui-0.10.2/lifecycle.log`。

## 制品

版本副本 `target/artifacts/0.10.2/my_spectral_resonator.clap` 与稳定文件 `target/bundled/my_spectral_resonator.clap` 均为 13,737,472 字节，SHA256 `C2B025C5E414EB52D2B8BE7CE7A833880204D4DA81BEEFC57A848CD9FEC2AB2D`。

发布前核对稳定文件为 0.10.1，并以备份方式原子替换；新文件与旧备份哈希已核对。备份路径、时间与哈希保存在 `target/ui-0.10.2/publication.json`。重新加载插件后，标题栏应显示 0.10.2。
