# 0.11.1 湿声音头位置与空格键

用户确认提前现象发生在插件内部 Middle dry/wet 混合，而非另一条并行轨。此次同时修正湿声时间位置和 Windows 编辑器吞空格的问题。

## 湿声提前的原因及测量

4096 点频谱分析看到一个瞬态后，将其共鸣重建到整个合成窗口。干声参考音头位于输入之后的第 4096 样本，而窗口重建的部分湿声能在这个参考点前出现。因此“干湿使用同样大小的输出环”不保证两者音头相同。0.11.0 的 Attack 控制建立速度，没有消除这种窗口前响。

`probe_wet_timing` 使用 48 kHz、440 Hz 基音、8 泛音、无 Damping、Wet=1 的单样本脉冲，在一个 hop 内五个相位测量。以每次输出峰值的 0.001（−60 dB）为起点阈值，与输入位置加 4096 的干声参考比较：

| 模式 | 原始湿声位置 | 开启对齐后 |
| --- | ---: | ---: |
| Natural | 提前约 65.92–65.94 ms | 晚约 19.40–19.42 ms |
| Independent / Attack 0 | 提前约 68.25–70.50 ms | 晚约 14.83–17.08 ms |

这些是指定脉冲的测量，不是对任意声音的感知提前量估计。测试也直接检查完整采样数据，而不只依赖阈值。

## 对齐改动及代价

新增宿主参数 `align_wet`，UI 位于 Resonance 页 `Align wet onset`，也加入 Envelope 远程页。插件新实例默认开启，旧预设缺失时补为开启；显式保存的关闭值保留。关闭可恢复原来的时间位置。

开启时在 Unison **之后**增加 4096 样本湿声预延迟，48/44.1/96 kHz 下分别约 85.33/92.88/42.67 ms。这是使整个湿声输出避开干声参考前方的保守修正，湿声主体和尾音也会整体后移；不是音头峰值自动对齐，也不是缩短 FFT 窗口。强 Attack 和极短 Decay 仍受窗口展宽限制。

左右使用独立数据；主输出、辅助 Wet 输出和白色频谱都取延后后的信号。干声及报告给宿主的 4096 样本延迟不变，效果尾长另外包含新增预延迟，避免离线排空提前结束。DSP 控制结构的默认对齐值保留 false 供旧引擎诊断使用；插件每帧明确传入宿主参数，默认 true。

预分配一个 4096×2 浮点环，约 32 KiB，无新 FFT 或音频线程分配。原始和延后两路一直运行，切换约 20 ms 淡变，期间可能听见两种时间位置；不清空共鸣。首次处理/重置后直接选择目标分支，避免淡变起点泄漏前响。重置清空新增环。

测试覆盖 44.1/48/96 kHz、不同 hop 相位、首次输入即脉冲、Natural/快速 Attack、Spectral/Post Unison：开启后湿声在干声参考前严格为零，原湿声逐样本平移 4096 后与新湿声相等，干声逐样本不变，右声道静音不串音。另有开关过渡、尾音保留和 reset 检查。

## 空格键

根因在 vendored baseview 的 Windows 消息钩子：它预先调用插件窗口处理键盘消息，然后无条件把消息替换为空消息。即使 egui 返回 Ignored，宿主也拿不到原来的 Space KEYDOWN。

现在先以无副作用的接口查询 Space 捕获策略，再决定是否解码。普通界面保留原 DOWN/CHAR/UP 和重复标志给宿主；即使宿主继续向子窗口分派该消息，也不会同时触发 egui 按钮。没有向音频宿主合成额外按键。只有数值编辑期间捕获 Space，输入提交/取消后恢复放行。其他原生按键路径保持原样。

`scripts/probe-clap-editor.py --keyboard` 在自己创建的宿主/子窗口测试真实 Windows 消息队列，不操作用户 DAW：

- 0.11.0 负例收到 0 条原始空格消息，复现吞键；`target/space-old-failure.log`。
- 0.11.1 普通状态、带重复标志的消息、完成输入后三段操作，共 9 条原始消息正确到达宿主队列。
- 数值输入时 Space 未泄漏给宿主，将 Low / Mid 从 250 输入为 300 并成功提交；随后 Space 再次放行。
- 测试宿主补齐了 CLAP 参数输出队列：nice-plug 只有在输出这些事件的音频边界才正式更新 GUI 提交值。该修改仅完善测试宿主。
- 三次打开/缩放/关闭通过，尺寸 1120×780、1260×860、980×700；音频线程共 2186 块。日志 `target/timing-0.11.1/keyboard.log`。

这证明测试宿主能收到原始快捷键消息，不等于已经在真实音频宿主中验证其快捷键设置及插件沙箱转发。

## 其他验证与制品

- 全部 workspace 源码/GPU 测试：42 DSP + 37 插件 = 79 项通过。`target/timing-final-tests.log`。随后强化的首次采样脉冲与切换两项测试也通过，`target/timing-0.11.1/startup-tests.log`。
- 音频回调仍通过无分配、块长变化、独立立体声、16 复音/8 Unison、主/辅助路由回归。声场测试按新增湿声延迟移动稳态测量区间，仍检查相同声道衰减性质。
- GPU 实际绘制预览 `target/timing-0.11.1/resonance.png`，新开关和其他控件布局已检查。
- 全目标/全功能 Clippy `-D warnings`、格式化与差异空白检查通过。release 编译保留 vendor/nice-plug 原有两个 unused-variable 提示。
- 版本制品 CLAP validator：36 passed，0 failed，0 warnings，8 skipped；`target/timing-validator.log`。
- 对齐开启、Attack 0、Decay 5 ms，Internal/MIDI × Spectral/Post × 48/44.1/96 kHz 共 12 个模拟离线配置通过，保存/恢复、尾音排空及停止/重启通过；`target/timing-0.11.1/bounce/`。真实音频宿主 Bounce 和主观听感仍待重载后确认。

制品：`target/artifacts/0.11.1/my_spectral_resonator.clap`，13,749,760 字节，SHA256 `E42AE6E3A10A1E0B0EE796CD5C10F86CCC96B4B64721CBDB223932303A18CBB1`。稳定路径更新及 0.11.0 备份信息见 `target/timing-0.11.1/publication.json`。

复现测量：

```powershell
cargo run -p spectral-dsp --example probe_wet_timing --release --locked --offline
cargo run -p spectral-dsp --example probe_wet_timing --release --locked --offline -- --aligned
python scripts/probe-clap-editor.py target/artifacts/0.11.1/my_spectral_resonator.clap --keyboard --seconds 3 --cycles 3
```
