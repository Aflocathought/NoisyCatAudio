# 0.12.0 共鸣湿声音头塑形

用户希望 Attack 直接重塑音头、改善瞬态，并明确选择只处理共鸣湿声，不混入原始音头。本版增加 `Attack response → Reshape wet`；旧 Natural、Independent 模式及默认选择保留。

## 使用

在 Resonance 页选择 `Reshape wet`，先试 Attack 0–3 ms、Transient emphasis 3–6 dB、Wet alignment 0。Spectral Unison 单路适合先判断起音差异，再逐渐增加 Unison 或选择 Post。

- `Attack / 90%`：控制时域开启斜坡和强调包络的上升速度，0–2000 ms。0 ms 内部保留 0.25 ms 的短斜坡，避免硬开关。
- `Transient emphasis`：音头短暂强调，0–12 dB，默认 6 dB。0 dB 仍保留静音后起音的开启塑形；已经开启的尾音上不会新增强调。
- 不自动修改 Wet alignment；原先 0.5/1 窗延迟会继续整体移动塑形后的声音。Post Unison 也保留自身可变延迟。
- 切回旧模式不重置隐藏数值。新增参数 ID 为 `attack_emphasis_db`，旧预设缺失时补 6 dB；新增模式序列化 ID 为 `reshape`。宿主增加 Transient 远程参数页。

## 实现与边界

当前 Independent Attack 只改变泛音的上升极点，之后仍经过长窗口合成。Reshape 在此基础上使用最快频谱激励，并在重建湿声的时域包络上处理音头。信号链为：

`FFT 共鸣 / IFFT → 音头塑形 → Post Unison → Wet alignment → 主/辅助 Wet 输出`

已有输入环在覆写前的槽位就是 `x[n-N]`，其时间位置与干声基础延迟一致。这个信号只用于控制增益，绝不加进湿声。无新 FFT、额外复音或音频缓冲；每声道只增加少量状态和每采样运算。宿主延迟仍为 4096 样本。

从静音开始时，湿声在对应输入到达前保持关闭。输入超过约 −140 dBFS 后按 Attack 线性开启，达到 90% 所需时间对应参数。开启后会保留到输入与未塑形湿声都低于极低阈值持续 120 ms，避免音符松开就切断共鸣。

逐声道相对峰值检测捕捉新音头：输入超过 −100 dBFS、比具有 20 ms 指数释放的峰值参考高 1.8 倍，且距离前次触发至少 30 ms。强调包络按 Attack 上升，完整上升后约保留 12 ms，然后按 60 ms 下降 60 dB 的速度退回单位增益。新触发不把包络或开启增益重置为零，因此不会反复截断旧尾音。模式切换约 20 ms 淡变，reset 清除全部状态，旧模式稳定旁路时精确返回原样本。

局限必须与结果一起理解：

- 塑形作用于每个声道已合成的湿声，不是逐个音符分离。密集音头叠在旧尾音上时，旧尾音也可能被短暂强调；不能保证去掉每个新音头的前响。
- 低于已有输入峰值的新音头、缓慢渐强、持续噪声或非常密集的敲击不一定触发强调。低电平输入仍可开启基础包络。
- 长 Attack 会让音头更软；强强调会抬高瞬态峰值。包络塑形会改变瞬态频谱，不能承诺完全不变的音色。
- Post Unison、Spectral 多副本相位叠加和频段设置会改变主峰位置。未缩短 FFT 分辨率或基础宿主延迟，不宣称所有配置主峰都自动对齐。

## 脉冲测量

48 kHz、N=4096/H=512，440 Hz、8 泛音、T60=2 s、无 damping，Middle Mix 50%，Wet Level 1，分频 1/12000 Hz，Motion Off、单路 Spectral Unison、Wet alignment=0。单位脉冲放在 `8192+offset`，offset=0/127/255/383/511。

时间相对干声主峰。起声阈值为各输出峰值的 −60 dB；它不是可闻阈值。强调为 +6 dB。

| 响应 | −60 dB 起声位置 | 绝对采样主峰位置 |
| --- | ---: | ---: |
| Independent / 0 ms | 提前 68.25–70.50 ms | 晚 20.458 ms |
| Reshape / 0 ms | 0.000 ms | 晚 11.354 ms |
| Reshape / 10 ms | 晚 0.021 ms | 晚 20.458 ms |
| Reshape / 100 ms | 晚 1.000 ms | 晚 111.354 ms |

另直接检查原始样本，所有新模式渲染在干声参考之前严格为零，干声逐样本不变。三采样率、两种 Decay、Spectral/Post 三路 Unison 的回归证明起音前无湿声、后段尾音在强调结束后与原快速分支逐样本一致，覆盖首次采样即脉冲及 reset。多路 Unison 的绝对主峰并不一定更早，例如 44.1 kHz、Spectral 三路的 2 s Decay 用例主峰仍在约 +64.76 ms，不能把单路表格结果推广到所有设置。

图像 `target/reshape-0.12.0/transient-response.png` 和 `.svg` 上方显示原始采样绝对值，下方显示 1 ms RMS，所有曲线使用同一幅度标尺。`transient-response.json` 保存五个相位的原始指标，CSV 位于 `impulse/`。

持续音和同一音符内重复 30 ms 音头的湿声试听位于 `target/reshape-0.12.0/audio/`，包含原始输入、Natural、Independent 和三种 Reshape Attack，固定输入/输出增益、未逐文件归一化。`measurements.csv` 的持续音 T90 从 10 ms RMS 测量窗的**起点**计时，窗口会读取其后 10 ms，不能用这个字段证明声音早于宿主延迟。

## 验证

- Workspace 含 GPU：47 DSP + 38 插件 = 85 项通过，`tests.log`。
- 后续加入首次采样即脉冲和“只有参考、没有湿声时输出必须为零”的断言，3 项塑形测试通过，`startup-tests.log`。
- 原音头和尾音的增益测试覆盖重复触发、左右独立、Attack 上升时间、精确旁路、reset；插件自动化测试纳入 Reshape、Emphasis、不同块长、MIDI/调制/Unison 切换，保持无音频线程分配。
- 隐藏参数保留和实际 CLAP 状态往返通过，`state-abi.log`。
- GPU 最小窗口 980×700 已目视检查，`reshape-ui.png`；预览参数设置修正后再次绘制通过，`gpu-final.log`。
- Clippy 全目标/功能 `-D warnings`、格式、diff 空白检查通过。release 仍有 vendored nice-plug 原有两个 unused-variable 提示。
- CLAP validator：36 passed、0 failed、0 warnings、8 skipped，`validator.log`。
- Reshape / Attack 0 / Decay 5 ms / Alignment 0 的 12 个模拟 Bounce 配置通过；原 Independent / Attack 0 / Decay 5 ms / Alignment 0.5 的 12 个配置也通过，涵盖 Internal/MIDI × Spectral/Post × 48/44.1/96 kHz。日志 `bounce-reshape/`、`bounce-legacy/`。
- 后一组 12 个 WAV 与 0.11.2 对应文件 SHA256 全部相同，`legacy-comparison.json`。
- 实际测试窗口打开/关闭及空格消息回归通过，886 音频块、9 条预期宿主空格消息，`keyboard.log`。

未特别说明的文件均位于 `target/reshape-0.12.0/`。这些验证不代替真实音频宿主的听感、实时负载或 Bounce 验收。

## 制品与复现

`target/artifacts/0.12.0/my_spectral_resonator.clap`，13,757,440 字节，SHA256 `E9D7A3FB9F6F2FCC3F32C4FE704B1FF9BFCB590074B46A196364392AC59C1549`。稳定路径和旧版备份信息见 `publication.json`。

```powershell
cargo run -p spectral-dsp --example probe_wet_timing --release --locked --offline -- --alignment 0 --reshape-attack-ms 0 --emphasis-db 6 --csv-dir target/reshape-probe
cargo run -p spectral-dsp --example render_envelope --release --locked --offline -- target/reshape-audio
python scripts/probe-clap-bounce.py target/artifacts/0.12.0/my_spectral_resonator.clap --reshape --attack-ms 0 --decay-seconds 0.005 --wet-alignment 0 --require-tail-drain --output target/reshape-bounce
```
