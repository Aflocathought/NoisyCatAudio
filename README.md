# Spectral Resonator

面向 Bitwig Studio 的 Rust 频谱共鸣效果器，当前为 **0.9.1 开发版**。egui/wgpu 独立编辑器提供蓝色干声与白色 Unison 后湿声的频谱瀑布图、可拖动分频线、精细旋钮和数值输入、分类参数页。0.9.1 加快时间滚动、提高低频显示分辨率，拉大窗口时优先扩大频谱。支持 16 声部 MIDI 复音、最多 512 泛音（默认 256）、Chorus / Wander / Granular、1–8 路 Unison，以及六节点频率衰减曲线。Unison 可在 Spectral 与 Post 两种算法间切换。默认双声道，左右独立处理，兼容单声道。

Windows x86-64 CLAP：`target/bundled/my_spectral_resonator.clap`，版本副本位于 `target/artifacts/0.9.1/`。显示改进见 [0.9.1 记录](docs/validation/UI_REFINEMENT_2026-09-27.md)，UI 与路由说明见 [0.9.0 记录](docs/validation/UI_2026-09-27.md)。保留 0.8.2 的有限尾音与 Bounce 修复；真实 Bitwig Bounce 是否恢复仍待用户确认，详见 [0.8.2 调查](docs/validation/BOUNCE_TAIL_2026-09-27.md)。

## 界面与宿主效果链

点击 Bitwig 设备的 **Plug-in Interface** 打开 GPU 编辑器。设备栏可使用 CLAP Remote Controls 的 Perform / Motion / Resonance 三页。公开接口没有提供 Bitwig 原生任意画布组件；CLAP 的迷你曲线草案也不足以实现交互瀑布图。

蓝色代表原始输入，补偿 4096 样本基础延迟；白色代表 Unison 后、包含 Wet Level / Mid Mix 和 Output Gain 的湿声。左右声道按功率合并显示，防止反相信号相消。频率轴为 20 Hz 至 min(20 kHz, Nyquist)，亮度范围约 -84 至 0 dBFS。历史约 4.3 秒，新信号在上方，每秒一条时间刻度；相同图高下滚动速度为 0.9.0 的两倍。拖动分频线或旋钮可改频率，Shift 拖动细调，点击数值可键入，旋钮双击复位。Freeze view 只冻结显示。

低频显示使用 16384 点分析窗，400 Hz 以下完整应用，400–1000 Hz 平滑衔接至原来的 4096 点短窗。48 kHz 下低频频点间隔由 11.72 Hz 缩小至 2.93 Hz，但其时间窗口也由 85.3 ms 加长到 341.3 ms，所以低音的时间轮廓更宽；1 kHz 以上保留短窗响应。这里仅改变可视化，不增加声音延迟。拉高窗口时新增高度全部分配给频谱，控制区维持紧凑布局。

**Main Output** 默认为 Mixed；选择 Wet only 后，主输出只剩 Unison 后湿声，可直接接 Bitwig 效果器。若宿主选择可选的 **Stereo + Wet** 布局，则额外输出 **Wet / Post Unison** 可用于独立效果链。要分别处理后相加，将主输出设为 Dry contribution；Mixed 与额外湿声同时相加会重复湿声。默认 Stereo / Mono 布局保持不变，宿主若不提供布局选择，可使用 Wet only 加 FX Layer 的并行干声路径，但该路径保留的是完整干声，不等同于插件内部经过分频的干声贡献。

图中白色信号取自本插件内部，不能显示 Bitwig 后续效果器的输出。窗口关闭或隐藏时停止可视化采集；显示 FFT 和 GPU 上传均不在音频线程上运行。

## 在 Bitwig 中使用

1. 重新加载插件，确认加载版本为 0.9.1。在立体声音频轨道上新建实例，确认双声道配置。避免同时扫描具有相同插件 ID 的历史副本。已加载的旧实例需要重新加载，单独扫描文件不保证替换内存中的旧版。
2. `Pitch Source = Internal` 时，`Root Note` 选择 MIDI 33–93，即 55–1760 Hz 的基音；音频输入持续激励共鸣。
3. `Pitch Source = Midi` 时，在插件前用 Note Receiver 等宿主路由接收音符。最多 16 个声部同时共振，松键只停止该音符的新激励，已有尾音继续按 Decay 衰减；加入新音符不会清空旧共鸣。支持重复同音、踏板 CC64、All Notes Off、All Sound Off 和 CLAP Choke。
4. `Harmonics` 范围为 **1–512，新实例默认 256**；`Decay Mode = Damping` 沿用全局 `Decay T60` 和高低频阻尼。切换到 `Curve` 后，设置六组 `Decay Point n Frequency / T60`，直接指定各频率衰减到 −60 dB 的秒数，取代全局 Decay 和阻尼。`Excitation` 调整进入湿声的激励增益。
5. `Low / Mid`、`Mid / High` 控制分频点。`Mid Mix` 只混合中频：0% 为干声，100% 为共鸣湿声，默认 100%。`Wet Level` 独立控制湿声增益，默认 4、范围 0–16。Mix=0% 时全频干声经原有延迟和 Output Gain 输出；Wet Level=0 则仅静音湿声，在 Mix=100% 时中频仍被压低。Panic 开启时淡出并保持湿声静音，关闭后可重新演奏。
6. `Modulation Mode` 可选 Off、Chorus、Wander、Granular。Chorus 为逐泛音周期调制；Wander 为逐泛音随机漂移；Granular 为逐泛音不规则触发的指数衰减包络。`Mod Rate` 控制速率或颗粒密度，`Mod Amount` 控制深度，`Pitch Mod` 控制音高调制范围，`Grain Decay` 控制颗粒包络的时间常数。
7. `Unison` 设置 1–8 路副本，默认 1，`Unison Detune` 设置最外侧副本的正负失谐音分（0–50）。`Unison Mode = Spectral` 保留原来的逐泛音副本和独立调制；选择 `Post (Audio)` 后，先完成一份湿声，再用双读头变速延迟生成整体失谐副本，适合比较较轻的计算负载。Post 下每个泛音仍有 Chorus / Wander / Granular，但生成的 Unison 副本共享这份调制结果。Unison=1 或 Detune=0 时 Post 移调旁路。
8. `Voice Spread` 控制 16 个 MIDI 声部的自动 Pan，范围 0–100%，默认 0%。0% 保留原始立体声，调大后各音符按左右交替的八个位置循环分布；可以先试 30%–60%。16 个声部可以共享位置，沿用旧版空间排列。松键后的尾音保留位置，增减音符不会重排其他声部。Spectral 副本跟随所属音符；Post 处理已经完成此声场分配的湿声，左右不串音。

Spread 只改变共鸣湿声的左右平衡，干声不受影响。它保留独立 L/R，不把两路折成单声道再声像定位：偏左时保持左路、衰减右路，偏右反之，所以纯左输入不会被搬到右路。100% 时外侧音符可完全偏向一侧，总电平可能下降；0% 不会把原立体声缩成单声道。单声道实例忽略 Spread。Internal 模式的音符也采用同一分配规则，单个持续音符不会自动来回移动。

曲线节点初始为 80、250、1000、4000、10000、20000 Hz，各 2 秒。频率可在 20–20000 Hz 内移动，时间范围 0.05–12 秒；节点之间按对数频率和对数时间插值，范围外保持端点时间。节点自动按频率排序；同频节点使用编号较大的节点。曲线按原始泛音的绝对频率查询，Unison / Pitch Mod 不会让衰减时间随调制来回变化。目前通过宿主参数面板编辑节点，尚无可拖动的曲线图。

例如把 250 Hz 设为 4 秒、1000 Hz 设为 1 秒，中间 500 Hz 的 T60 为 2 秒。插件内部仍将秒数换算为稳定的反馈系数，但用户直接控制频率与时间，不必用 Damping 反推衰减。模式与数量切换平滑过渡并保留底层共鸣状态。

旧参数 ID 保持不变；`M2 Wet Level` 只改显示名称为 `Wet Level`。旧工程保存的 Wet Level 可能仍为 1，默认值不会覆盖已有参数。旧预设补齐 Internal、32 泛音、无额外阻尼等缺失参数；缺失的 `mid_mix` 补为 100%，保存过的 Mix 则照原值恢复。0.7.0 新效果默认 Off、Unison=1、Decay Mode=Damping，旧工程缺失这些参数时也恢复为此组合。缺失的 `voice_spread` 补为 0%。0.8.0 新参数 `unison_mode` 缺失时补为 Spectral；保存的 Post 会恢复。按用户要求降低容量后，旧工程超过 512 的 Harmonics 保存值会明确钳到 512，其余有效值保留。0.5.0 更换过声音引擎，不保证更早版本的音色逐样本一致。

## 当前边界

固定 N=4096、H=512，报告延迟仍为 4096 样本：44.1 kHz 下约 92.88 ms，48 kHz 下约 85.33 ms。事件按宿主样本偏移接收，音高和频谱控制在 hop 边界生效；长窗本身也会混合时间信息。每个短音符分别按 hop 内 Gate 的占比激励，同一个 hop 内可同时保留多个音符。

16 声部上限包含正在衰减的尾音。满额时优先淡出能量最低的已松键声部，否则淡出最早按下的声部；约 20 ms 淡出按 hop 量化，替代音符等待淡出完成后进入，因此只在抢占时增加这段启动等待。极密集输入使所有声部都正在淡出时，最新音符替换最早的待启动音符，不重新截断正在播放的淡出。各声部直接相加，和弦可能更响，可用 Wet Level / Output Gain 留出余量。

Harmonics 是数量上限，实际数量还受基音和采样率限制，频率达到 `0.49 × 采样率` 的泛音不参与计算。模板在初始化时按每个 MIDI 音符预计算，收到 MIDI 后选用相应模板，不需要在回调中分配。55 Hz 基音在 48 kHz 下最多 427 个、96 kHz 下受新版容量限制为 512 个；150 Hz 在 48 kHz 下最多 156 个，而不是无条件计算 256/512 个。150×256 实际为 38,400 Hz，156×150 才是 23,400 Hz。中频分频与阻尼仍会抑制高次泛音；分频下限与共鸣基音是不同控制，当前 MIDI 范围仍为 33–93。

Post 是小幅移调实验算法，带来约 0.33–40.33 ms 的可变湿声读头延迟（48 kHz），并可能产生轻微幅度起伏、旁带和高频变化；它不是 Spectral 的等音色替代。干声保持原来的 4096 samples 延迟，湿声延迟属于效果本身；算法切换和副本数量变化会短淡变，已有共鸣状态不清空。两种模式都使用每声道一次 FFT/IFFT；两路实信号打包在复数 IFFT 的实部/虚部输出，避免为分离干湿增加变换次数。

0.8.0 的三轮离线 A/B 在 48 kHz / 256 samples、16 声部、Harmonics=256、Unison=8、Granular 下，55 Hz 起的持续和弦平均块耗时中位数由 Spectral 的 2.17 ms 降至 Post 的 0.63 ms；155.6 Hz 起则由 1.00 ms 降至 0.37 ms。全部 Post 配置的观测最大单块为 3.36 ms，低于本次 5.33 ms 预算。这是有音色差别的算法比较，不能作为 Bitwig 任意工程的实时保证；完整 p99、峰值及测试条件见 [0.8.0 记录](docs/validation/POST_UNISON_2026-09-27.md)。

0.7.3 在本机 48 kHz / 256 samples 的三轮离线测量中，16 声部 × 8 路 Unison × 64 泛音的最坏场景 p99 为 2.50 ms，全部测量最大单块为 3.65 ms。提高到 256 泛音后，持续和弦 p99 的三轮中位数约 5.16 ms，换和弦/抢占/曲线自动化也出现超过 5.33 ms 预算的处理块，不能保证该组合的实时余量。保持 16 × 8 × 256、改用 512 samples 后，最坏场景 p99 为 6.52 ms，最大 8.58 ms，低于当时的 10.67 ms 预算。这里的 p99 是约 99% 块耗时不超过的值，不是最大值；完整峰值、配置与边界见 0.7.3 验证记录。未自动减少用户选择的泛音或副本数，Bitwig 实际工程仍需试听。

此前 0.7.1 将共轭频谱镜像从每个泛音的内循环合并为每声道一次，保留全部 65 点核、泛音数、Unison 数、精度与延迟。三轮交替 A/B 中，48 kHz/256 泛音/Granular×4 的平均处理耗时下降约 46%–50%，Off×1 下降约 14%–23%。当时按 256 样本块测量，四组压力场景最大单块约 2.32 ms，预算为 5.33 ms。这是旧版八声部池的离线结果，不代表新版十六声部满载；详见 0.7.1 性能记录。

已提供自定义 GPU 编辑器及干声/湿声瀑布图，宿主参数面板仍可使用。MPE、Pitch Bend、连续滑音、VST3 和正式安装器尚未实现。平滑换音是尾音重叠与声部淡入淡出，不改变音符的固定音高。MIDI 模式需要音频作为激励。超出 MIDI 33–93 的音符直接忽略；接近 Nyquist 的泛音渐弱并排除。

新效果参考 [Live 官方手册](https://www.ableton.com/en/live-manual/12/live-audio-effect-reference/#spectral-resonator) 的功能语义，采用自己的实现，不保证相同音色。Spectral Unison 共用原始泛音的激励与衰减状态，再以不同频率、独立相位重建；Post 是独立的整体湿声移调方案，不能据此推断 Live 的内部算法。L/R 共用调制/读取轨迹，但音频状态独立、没有串音；Voice Spread 在声部输出处调节左右平衡。Granular 作用于频谱泛音，不是时域采样切片器。

`Grain Decay` 为指数包络的时间常数（一个时间常数后剩约 36.8%），不是 T60 或硬性颗粒长度；实际包络另受约 20 ms 平滑、hop 与分析窗限制。Chorus 的 Rate=0 时关闭音高摆动、固定各泛音的幅度分布；Granular 的 Rate=0 时停止产生新颗粒，已有颗粒继续衰减。高复音、高泛音和多路 Unison 会叠加 CPU 成本，详见本版负载记录。

## 文档与证据

- [项目大纲](docs/PROJECT_OUTLINE.md)：产品范围、里程碑与退出条件。
- [实现设计](docs/IMPLEMENTATION.md)：算法、生命周期、实时约束与后续设计。
- [0.5.0 验证记录](docs/validation/M3_2026-09-26.md)：数值结果、MIDI/内存检查、CPU 测量和宿主验收步骤。
- [0.5.1 Mid Mix 验证](docs/validation/MID_MIX_2026-09-26.md)：中频干湿比例、旧状态兼容与构建结果。
- [0.6.0 复音验证](docs/validation/M4_POLYPHONY_2026-09-26.md)：8 声部、自然尾音、抢占、事件压力及 CPU 测量。
- [0.6.1 泛音扩展验证](docs/validation/HARMONICS_1024_2026-09-26.md)：1024 上限、256 默认值、Nyquist 裁剪与最大负载。
- [0.7.0 调制与曲线验证](docs/validation/MODULATION_CURVE_2026-09-26.md)：Chorus、Wander、Granular、Unison、六节点 T60 曲线及性能边界。
- [0.7.1 性能验证](docs/validation/PERFORMANCE_2026-09-26.md)：分段热点、合成优化、新旧音频差异、三轮 A/B 与 256 样本块测量。
- [0.7.2 声场验证](docs/validation/VOICE_SPREAD_2026-09-26.md)：八位置自动 Pan、尾音保持、平滑控制和立体声兼容。
- [0.7.3 Unison / 复音验证](docs/validation/UNISON8_POLYPHONY16_2026-09-27.md)：8 路 Unison、16 MIDI 声部、零分配、三轮性能测量和内存预算。
- [0.8.0 后处理 Unison 验证](docs/validation/POST_UNISON_2026-09-27.md)：512 上限、双算法切换、移调算法、干湿分离、音频回归和三轮 A/B 性能。
- [0.8.1 Bounce 兼容修复候选](docs/validation/BOUNCE_TRANSITION_2026-09-27.md)：宿主日志、CLAP 模式切换重启请求、最小框架补丁和制品级回归。
- [0.8.2 有限尾音与模拟 Bounce](docs/validation/BOUNCE_TAIL_2026-09-27.md)：双线程宿主模拟、有限等待、尾音通知、12 次离线录制与音频一致性。
- 历史记录：[M1](docs/validation/M1_2026-09-26.md)、[M2](docs/validation/M2_2026-09-26.md)、[0.4.x 分频](docs/validation/M3_CROSSOVER_2026-09-26.md)。

## 构建与验证

项目固定 Rust 1.95.0、nice-plug 0.4.2、nice-plug-egui 0.5.1、egui 0.36.1、wgpu 30、RustFFT 6.4.1，使用 Cargo.lock。UI 依赖要求 Rust 1.95；缓存位于仓库内 `target/cargo-home`，无需改全局 Cargo 配置。

```powershell
# 本机已经准备好的离线缓存；新机器可使用正常可用的 Cargo home。
$env:CARGO_HOME = Join-Path (Get-Location) 'target/cargo-home'
$env:RUSTUP_TOOLCHAIN = '1.95.0'
cargo fmt --all -- --check
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
./scripts/bundle-clap.ps1 -Offline
./scripts/validate-clap.ps1
```

Bitwig 占用稳定文件时，使用 `./scripts/bundle-clap.ps1 -ArtifactOnly -Offline` 生成版本副本，再执行 `./scripts/validate-clap.ps1 -PluginPath target/artifacts/0.9.1/my_spectral_resonator.clap`。打包脚本总是先保留版本副本。新机器首次下载依赖时去掉 `-Offline` / `--offline`。框架补丁分别记录于 [CLAP 补丁](vendor/nice-plug/PATCHES.md) 和 [编辑器补丁](vendor/nice-plug-egui/PATCHES.md)。

验证脚本优先使用 `target/tools/bin/clap-validator.exe`，否则从 PATH 查找。`scripts/probe-clap-editor.py PLUGIN` 在独立测试窗口中检查 GPU 编辑器的创建、缩放、显示和关闭重开，音频只写内存，不送扬声器。`cargo test -p spectral-resonator-plugin render_gpu_preview -- --ignored --nocapture` 使用同一 UI 和 GPU 渲染器生成 `target/ui-preview/*.ppm`，属于离屏渲染验证，不代表 Bitwig 宿主验收。

运行 `cargo run -p spectral-dsp --example render_m4 --release --locked --offline` 可生成 `target/fixtures/m4/` 下的立体声输入、和弦共鸣输出 WAV，并打印 8 声部固定和弦、和弦更换与密集抢占时的离线块耗时。历史 `render_m3` 示例保留。这些是合成测试素材，不能替代 Bitwig 实际演奏。

`render_m4` 默认使用 64 泛音、48 kHz。位置参数依次为输出目录、泛音上限、采样率、模式（off/chorus/wander/granular）、Unison 数、衰减方式（damping/curve）、块长（默认 128）、每组音符数（1–16，默认 8）。例如 `cargo run -p spectral-dsp --example render_m4 --release --locked --offline -- target/fixtures/granular-256-48k 256 48000 granular 8 curve 256 16`。四组场景包括持续和弦、换和弦、密集抢占、持续和弦并自动化衰减曲线；输出实际峰值声部数。尾音也占槽，8 个新音符加旧尾音可能达到 16 声部，不能与旧版 8 槽池的换和弦负载直接比较。试听 WAV 仍保留两组四音和弦，便于复现历史音频。

分段计时使用 `cargo run -p spectral-dsp --example profile_hotspots --features profiling --release --locked --offline`。`profiling` 仅用于离线诊断，常规 CLAP 构建不启用，音频路径中不包含时钟调用。

0.8.0 在 `render_m4` 的每组音符数之后增加两个位置参数：Unison 算法（`spectral`/`post`，默认 spectral）与压力场景最低 MIDI 音符（33–82，默认 33）。例如 `./target/release/examples/render_m4.exe target/fixtures/post-ab 256 48000 granular 8 curve 256 16 post 51` 用约 155.6 Hz 起的音符做后处理压力测试。两组四音试听 WAV 保持固定音符不变，便于算法 A/B；最低音符参数只改变计时场景。
