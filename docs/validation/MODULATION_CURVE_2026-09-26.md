# 0.7.0：Chorus、Wander、Granular、Unison 与频率衰减曲线

日期：2026-09-26。基于 0.6.1 的 8 声部、1024 泛音上限（新实例默认 256）、Mid Mix 与自然尾音增加音色控制。用户最终确认选择 Granular；本次没有添加手工逐泛音 Partial 编辑器。修改位于未提交的共享工作区，没有创建 Git 提交。

## 已交付行为

| 宿主参数 | 范围 / 初始值 | 行为 |
| --- | --- | --- |
| Modulation Mode | Off / Chorus / Wander / Granular，默认 Off | 改变湿声各泛音的幅度与合成频率 |
| Mod Rate | 0–10 Hz，默认 0.5 | 周期/漂移速率；Granular 下为每泛音、每副本的颗粒触发密度 |
| Mod Amount | 0–100%，默认 50% | 幅度调制深度，并缩放音高调制 |
| Pitch Mod | 0–12 半音，默认 0.1 | Chorus/Wander 双向偏移；Granular 仅正向偏移 |
| Grain Decay | 5–500 ms，默认 80 | Granular 指数包络的时间常数 |
| Unison | 1–4，默认 1 | 每 MIDI 音符的湿声重合成副本数量 |
| Unison Detune | 0–50 音分，默认 7 | 对称分布的最外侧副本失谐量 |
| Decay Mode | Damping / Curve，默认 Damping | 传统阻尼与直接频率/T60 曲线之间切换 |
| Decay Point 1–6 Frequency | 20–20000 Hz | 默认 80、250、1000、4000、10000、20000 Hz |
| Decay Point 1–6 T60 | 0.05–12 秒，默认均为 2 | 指定该绝对频率衰减到 −60 dB 所需时间 |

Chorus 使用逐泛音三角波。Wander 在独立的随机目标间连续漂移。Granular 对各泛音独立、不规则地触发指数衰减包络；这是频谱域颗粒处理，不是时域音频切片。Unison 副本平均混合，各有独立相位和调制源，底层激励和共鸣状态共享。左右声道音频状态始终独立，共用调制轨迹，没有新增声像展开。

曲线在对数频率与对数时间上插值，范围外使用最近端点；节点可以跨越顺序，同频时原编号较大的节点生效。例如 250 Hz/4 秒、1000 Hz/1 秒之间，500 Hz 的 T60 为 2 秒。Curve 模式取代全局 Decay T60 和 HF/LF Damp，以原始泛音 `h*f0` 查询，输出音高调制不改变查表频率。DSP 仍将用户指定秒数转换为反馈半径，但不用用户通过阻尼参数反推时间。

默认 Damping、Off、Unison=1 保留原声音路径。加载旧状态时补入这些默认值，已有新参数保持保存值。六个节点的稳定 ID 为 `decay_point_hz_1..6` 和 `decay_point_seconds_1..6`；旧参数 ID 不变。节点浮点参数平滑 50 ms，调制参数平滑 30 ms，DSP 另有约 20 ms 的效果切换、副本淡变和轨迹平滑。已有尾音继续演化，不因切换模式清空。

界面仍为宿主参数面板，可编辑和自动化六组节点；可拖动曲线图尚未实现。`Grain Decay` 是指数时间常数，一个时间常数后包络剩约 36.8%，不是 T60 或硬性颗粒长度；输出还受 20 ms 平滑、H=512 和 N=4096 限制。Chorus 的 Rate=0 关闭音高摆动、冻结幅度分布；Granular 的 Rate=0 停止新颗粒并继续衰减现有包络。

## 参考与实现边界

本次核对 [Ableton Live 12 官方手册](https://www.ableton.com/en/live-manual/12/live-audio-effect-reference/#spectral-resonator)：模式为 None、Chorus、Wander、Granular；Partial 指单个泛音。手册描述 Chorus 的三角波、Wander 的随机锯齿源、Granular 的随机指数包络和 Unison 的失谐副本，但不公开完整内部 DSP。

本实现采用自己的频谱重合成模型。Wander 使用随机目标间线性插值，Unison 共用原始激励后分别失谐合成，不等价于多个独立失谐输入滤波器。没有进行 Live 音频 A/B，不承诺逐样本或听感一致。Harmonics 仍是每 MIDI 声部的上限，没有引入 Live 在复音间分摊总泛音数的规则。

## 源码与 CLAP 验证

Rust/Cargo 1.92.0、nice-plug 0.4.2、RustFFT 6.4.1、clap-validator 0.4.1；未新增依赖。

- `cargo fmt --all -- --check`、Clippy 全目标 `-D warnings`、release 构建通过。
- `cargo test --workspace --locked --offline`：**47 项通过，0 失败**，其中 DSP 26 项、插件 21 项。
- 动态频率核与直接分数频率 Hann 重建相比，测试频率的最大相对频谱 RMS 为 **2.305421e-5**；Unison ±20 音分的两个目标频率均检测到显著能量，并保留原共鸣状态。
- 44.1/48/96 kHz 下，不同音符中同为 440 Hz 的泛音获得相同曲线 T60；0.8 秒目标的状态衰减比相对误差低于 `1e-4`。节点变化不会重置尾音。
- 曲线测试覆盖节点排序、重合节点两侧插值、端点延伸、非法值与正时间范围。Granular 测试覆盖不规则触发、副本独立性、有限增益与上移时的 Nyquist 保护。
- 三种调制均改变音频；左声道与独立单声道计算相等，无输入右声道保持零。关闭效果、Unison=1 并冲刷过渡后，输出恢复原声音路径。
- 实际 `Plugin::process` 覆盖 Chorus→Wander→Granular→Off→Granular、Unison 2/3/4/1/2、曲线节点修改、Damping/Curve 切换及 note-off。固定 128 与变化块长输出逐样本相等；callback、Panic 和 reset 的堆分配/重分配/释放计数为 **0**。Mid Mix=0 时仍恢复延迟干声，尾音在模式切换后保留。
- 检查实际宿主参数表 ID 唯一、迁移键匹配、旧状态缺省与新状态保留。首轮 CLAP 检查发现非线性节点参数文本往返出现末位变化，已将频率显示精度设为 0.1 Hz、T60 为 0.001 秒，并增加每节点 1101 个采样位置的文本往返回归检查。
- 修复后版本制品与稳定路径分别通过 CLAP：**36 通过、0 失败、0 警告、8 跳过**。日志为 `target/modulation-artifact-validator.txt`、`target/modulation-stable-validator.txt`。
- 既有立体声、复音、密集抢占、踏板、最大泛音、音高、T60、Mid Mix 和延迟测试继续通过。

## 离线负载

本机 AMD Ryzen 9 7945HX，Windows x86-64，release，立体声、N=4096/H=512、128 样本块。每场景处理 8 秒合成输入，48 kHz 为 3000 块，192 kHz 为 12000 块。计时包括事件分派、曲线自动化快照和 DSP，不包括初始化/文件写入，也不包括 nice-plug 参数适配、宿主或声卡调度。CPU 型号通过本机注册表核实。

四个场景分别为八个持续低音、每 12000 样本更换八音和弦、每 64 样本触发新音的密集抢占、八音持续发声同时连续修改曲线。均使用 Curve，静态节点时间为 3、2.5、2、1、0.4、0.2 秒；调制 Rate=3 Hz、Amount=70%、Pitch Mod=0.2 半音、Detune=12 音分、Grain Decay=80 ms。自动化场景让第 3 点在 0.5–2.5 秒变化。

下表给出四个场景各自测量后取的最差 p99 和最大块耗时，单位 **ms**。它们不是所有场景混合后的统计量。

| 采样率 / 泛音 | 模式 / Unison | 最差 p99 | 最大块耗时 | 每块预算 |
| --- | --- | ---: | ---: | ---: |
| 48 kHz / 64 | Granular / 2 | 0.6964 | 1.0066 | 2.6667 |
| 48 kHz / 256 | Off / 1 | 0.8726 | 1.3768 | 2.6667 |
| 48 kHz / 256 | Chorus / 2 | 2.2800 | 2.7846 | 2.6667 |
| 48 kHz / 256 | Wander / 4 | 4.1817 | 5.0713 | 2.6667 |
| 48 kHz / 256 | Granular / 4 | 3.9610 | 5.0910 | 2.6667 |
| 192 kHz / 1024 | Granular / 4 | 16.1723 | 20.0322 | 0.6667 |

**48 kHz、64 泛音、2 路 Unison 是本次较轻的实测起点**。256 泛音/2 路已有部分峰值超时；8 个低音配 256 泛音/4 路在 48 kHz 也超过预算，192 kHz/1024/4 更不能称为稳定实时配置。没有自动缩小用户参数或丢音来掩盖成本。增加宿主缓冲、降低泛音或 Unison、减少同时发声数可减轻压力，具体仍需宿主验证。

各组 p50 约 1.6–3.7 µs，但 FFT 和重建集中在每四个块中的一个，不能据此判断实时余量。初始化约 27–30 ms（48 kHz）与 102 ms（192 kHz）。所有输出有限；没有进行 Bitwig 10 分钟运行、多实例或音频 underrun 验收。

完整原始耗时保存在 `target/modulation-{light,off,chorus,wander,granular,stress}-benchmark.txt`。重现：

```powershell
$env:CARGO_HOME = Join-Path (Get-Location) 'target/cargo-home'
$env:RUSTUP_TOOLCHAIN = '1.92.0'
cargo build -p spectral-dsp --example render_m4 --release --locked --offline
target/release/examples/render_m4.exe target/fixtures/modulation/granular-64-48k 64 48000 granular 2 curve
target/release/examples/render_m4.exe target/fixtures/modulation/off-256-48k 256 48000 off 1 curve
target/release/examples/render_m4.exe target/fixtures/modulation/chorus-256-48k 256 48000 chorus 2 curve
target/release/examples/render_m4.exe target/fixtures/modulation/wander-256-48k 256 48000 wander 4 curve
target/release/examples/render_m4.exe target/fixtures/modulation/granular-256-48k 256 48000 granular 4 curve
target/release/examples/render_m4.exe target/fixtures/modulation/granular-1024-192k 1024 192000 granular 4 curve
```

每组同时生成 `input-stereo.wav`、`polyphonic-stereo.wav`。WAV 为两组四音和弦及尾音的合成试听素材，性能测试另用低音高负载场景。文件输出峰值分别约为 0.5961、0.5774、0.5965、0.5936、0.5971、0.5375，文件增益均为 1；尚未进行主观试听验收。

## 制品与下一步宿主检查

Windows x86-64 CLAP **0.7.0**，**1,708,544 字节**。两个路径内容一致：

```text
target/bundled/my_spectral_resonator.clap
target/artifacts/0.7.0/my_spectral_resonator.clap
SHA-256: 6AF8A49F8A0CAACA2005DFA281C1AEF8C1EBD866003CFE724890317FA0DBDDC7
```

以原子替换更新稳定路径，没有关闭 Bitwig。原 0.6.1 文件保留为 `target/bundled/8bb4b437f287474aac933c06a760e1a6.previous`，SHA-256 为 `C920ED4A9DB1ECA32329EF827878593343629DE1E195D5C3BF2D35ECB4A5F05E`。运行中的实例仍需重新加载才能使用新版。

宿主待验收：确认扫描版本 0.7.0 与双声道；从 48 kHz/64 泛音/Unison=2 开始听三种模式；在长尾音中切换模式并调曲线节点；保存、重开工程确认所有节点及模式恢复；观察自动化、窗口/轨道切换、多实例和持续播放的峰值负载。源码与 CLAP 通过不代表这些宿主行为或与 Live 的听感已验收。
