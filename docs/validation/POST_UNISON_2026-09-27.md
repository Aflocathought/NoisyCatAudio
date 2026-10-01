# 0.8.0：512 泛音上限与后处理 Unison A/B

日期：2026-09-27。按用户要求将 Harmonics 最大值从 1024 降到 512，默认仍为 256；用户选择保留两种 Unison 算法，新增 `Unison Mode` 参数切换。已构建并更新 0.8.0 CLAP，保留旧版备份。Post 在本次离线压力测试中显著降低耗时，但改变了副本调制、延迟与声音细节；尚未进行音频宿主听感验收。

## 基频与实际泛音数量

泛音频率是 `h*f0`。引擎原本就只生成满足 `h*f0 < 0.49*fs` 的模板，初始化时为 MIDI 33–93 预计算，收到事件后选择相应模板。新版本实际启用上限为：

```text
min(用户 Harmonics, 512, ceil(0.49*fs/f0)-1)
```

严格小于号解释了整除时减一；不是每个音符无条件生成 512 个谐波。以下为 48 kHz、安全上界 23,520 Hz 的数学示例，含基音：

| 基频 Hz | 安全范围内数量 | 最高分量 Hz |
| ---: | ---: | ---: |
| 150 | 156 | 23,400 |
| 3,000 | 7 | 21,000 |
| 6,000 | 3 | 18,000 |
| 10,000 | 2 | 20,000 |
| 12,000 | 1 | 12,000 |

3/7/15 等按八度区间估计的数量只能作为该区间的上限，区间内还需按实际频率计算。当前产品基音范围仍为 55–1760 Hz，表中高频例子不表示已扩展可接收 MIDI 范围。150×256=38,400 Hz，150×156=23,400 Hz。中频分频下限决定输出哪些频段，与共鸣基音是不同控制。

降低最大值会减少预分配和高采样率下的极端容量；在 48 kHz、150 Hz 基音处，原本就只有 156 个有效分量，因此不会因 1024→512 再减半实际合成工作。

## 两种算法与状态兼容

`unison_mode` 使用稳定枚举值 `spectral` / `post`，显示为 Spectral / Post (Audio)。新实例默认 Spectral；旧状态缺失此参数也补为 Spectral，保存过的 Post 则保持。旧参数 ID 不变；保存的 Harmonics 1–512 保留，513–1024 明确钳到 512。Unison 仍为 1–8，Detune 仍为 0–50 cents，默认 1 路 / 7 cents。

Spectral 保留每个 MIDI 声部、每个泛音的多路合成与独立调制。Post 将泛音层 Unison 数设为 1，保留每个泛音自己的 Chorus/Wander/Granular，再对总湿声生成失谐副本。Post 的副本共享同一份已经调制过的湿声，不再各自重新生成逐泛音随机轨迹，因此两种模式不等音色。

MIDI 仍为 16 声部，Voice Spread 仍在声部层调节左右平衡。Post 共用左右的读取位置，但 L/R 读取各自的音频，不折成单声道，也没有交叉馈送。未新增 Unison 副本 Pan 参数。切换算法、改变路数和 Detune 都保留共鸣状态，使用约 20 ms 平滑/淡变；干湿参数与声部抢占规则保持原意。Unison=1 或 Detune=0 时，Post 移调旁路。

## 微移调算法

实现见 `crates/spectral-dsp/src/post_unison.rs`。采用预分配环形缓冲、双读头和窗函数交叉淡化，适用本插件的 ±50 cents 小幅失谐。

1. 对第 u 路，将音分 `c_u` 均匀分布到 `[-Detune,+Detune]`，换算读取速率 `r_u=2^(c_u/1200)`。例如 +7 cents 约为 1.004052 倍速；-7 cents 约为 0.995965 倍速。
2. 写指针每个样本前进 1。读取的是 `x[n-d[n]]`，故读取速度为 `1-Δd`。令 `Δd=1-r_u`，即可让延迟缓慢变短产生升调，或缓慢变长产生降调。延迟在有限范围循环，不能无限追赶写指针。
3. 两个读头的循环相位相差 0.5，权重采用互补 Hann 窗 `w` 与 `1-w`。一个读头回绕时，它的权重为零，由另一个接续输出，避免硬性跳接。
4. 小数位置用 16 taps、257 行分数位置表的归一化 Blackman 窗 sinc 插值，分数行之间再线性插值。窗与 sinc 表在初始化阶段构造；callback 不计算三角函数表、不分配内存。
5. 每路读取速率连续平滑，副本增益短淡变，按增益总和归一化。Post 总混合比例也淡变。旁路时持续写入湿声历史，使再次开启时缓冲已有内容。

读头扫描跨度固定为 40 ms，最小延迟 16 samples；48 kHz 下读取位置相对原湿声约落后 0.33–40.33 ms，另有插值核支撑范围。该可变延迟属于湿声效果，不改变干声的固定 4096 samples 宿主延迟。插件报告的尾音时长额外保留该缓冲及核所需样本。

Post 输入的湿声频谱在 `0.45*fs` 到 `0.47*fs` 间渐弱；最大 +50 cents 时，`0.47*fs * 2^(50/1200) < 0.484*fs`，避免最高分量越过 Nyquist。Spectral 继续使用原有逐泛音频率保护。窗函数重叠与有限插值会引入旁带、幅度起伏和高频变化；这不是无损、恒延迟、任意移调的重采样器，也不宣称是 其他产品 的算法。

## 干湿分离仍只用一次 IFFT

后处理只应作用于共鸣湿声，不能改变低高频干声或 Mix=0 的对齐。每个声道分别计算干声频谱 D 与湿声频谱 W，两者均为共轭对称频谱，对应实信号。将 `D+iW` 输入一次复数 IFFT，结果的实部就是干声、虚部就是湿声，再分别 overlap-add。湿声经过 Post 后才与干声相加。

因此每声道每 hop 仍为一次 FFT、一次 IFFT；新增两个湿声输出环与一条双声道后处理延迟线，而非新增多组 FFT。此重排会有浮点求和次序差异，已单独做音频回归。

## 测试与音频证据

Rust 1.92.0、nice-plug 0.4.2、RustFFT 6.4.1，Windows x86-64。`cargo fmt --all -- --check`、全目标全特性 Clippy（`-D warnings`）和 release 构建通过。**58 项源码测试通过：DSP 36、插件 22**。版本副本与稳定路径 CLAP 分别为 **36 passed、0 failed、0 warnings、8 skipped**。

- 最大 16 声部 × 512 泛音 × 8 Unison，在 192 kHz 下分别测试 Spectral/Post 的真实 callback 与 reset：alloc/realloc/dealloc 合计均为 0，输出有限，右侧零输入保持零。
- 固定块和可变块下同时自动化算法、Unison 路数、Chorus/Wander/Granular、曲线及 Spread，输出逐样本一致；Panic/reset 正确，尾音保留。
- Post 的真实引擎输出改变湿声，底层共鸣状态与一份原声共鸣逐样本保持一致；Mid Mix=0 仍恢复原固定延迟干声，纯左输入不进入右侧。
- 三个采样率（44.1/48/96 kHz）的 1 kHz、±50 cents 测试跨过多次读头回绕，两路目标频率均有输出，原载频低，未出现突跳。
- 48 kHz 下对 150/440/997 Hz、±50 cents 测试做加窗 FFT 峰值估计，六个目标的最大绝对误差为 **1.1542 cents**。150 Hz 恰与读头跨度对齐，440/997 Hz 补充非对齐案例；有限窗移调确实有邻近旁带，这个误差只代表本组测试，不能外推为全频段精度保证。
- 22,880 Hz 的高频状态在 Post 保护后，相对参考 RMS 约 `3.36e-8`，验证不会照搬可向 Nyquist 外移出的最高分量。
- 0.7.3 与 0.8.0 Spectral 的同参数 Granular×8、48 kHz 双声道和弦 WAV：768,000 个标量样本，峰值差 `1.192092896e-7`，相对 RMS 差 `1.087584484e-7`。低于已有回归门槛（1e-5 / 1e-6）；不是 Post 与旧算法等音色的证明。

日志：`target/perf-0.8.0/tests.txt`、`artifact-validator.txt`、`stable-validator.txt`。未在音频宿主内实际听取或进行长时间运行验收。

## 性能 A/B

CPU：AMD Ryzen 9 7945HX。统一 release 构建，不启用 profiling。48 kHz、256 samples，块预算 **5.3333 ms**；16 MIDI 声部、Harmonics=256、Unison=8、Detune=12 cents、Curve、Spread=0%。Granular 的 Rate=3 Hz、Amount=70%、Pitch Mod=0.2 st。

每个配置三轮交替 A/B，第二轮反转算法顺序。每轮四个八秒场景：持续和弦、每 12,000 samples 换和弦、每 64 samples 新音符密集抢占、持续和弦并自动化衰减曲线。每场景 1500 块，前五秒有衰减噪声激励、后三秒为零输入，保留全部计时峰值。初始化、文件写入和输入生成不计入，事件派发计入；测量期间未并发运行编译、测试或验证器。

持续和弦用两个独立身份的八音组：低音组 MIDI 33–40（55–82.4 Hz，均启用 256 分量），较高组 MIDI 51–58（155.6–233.1 Hz，按频率自动裁剪到约 100–151 分量/声部）。换和弦和密集场景的音高范围会进一步扩展。表中“平均”和“持续和弦 p99”取三轮中位数；“最坏场景 p99”取所有三轮四场景中最高 p99，绝对峰值不剔除。

| 最低音 / 调制 | Unison 算法 | 持续和弦平均 ms | 持续和弦 p99 ms | 最坏场景 p99 ms | 绝对峰值 ms |
| --- | --- | ---: | ---: | ---: | ---: |
| 55 Hz / Granular | Spectral | 2.1734 | 5.6100 | 6.9706 | 12.8616 |
| 55 Hz / Granular | Post | 0.6296 | 1.5538 | 2.2164 | 3.0036 |
| 155.6 Hz / Granular | Spectral | 0.9984 | 2.5822 | 3.1425 | 5.5435 |
| 155.6 Hz / Granular | Post | 0.3689 | 0.8487 | 1.2414 | 3.3630 |
| 55 Hz / Off | Spectral | 2.0858 | 5.1930 | 6.0403 | 8.5002 |
| 55 Hz / Off | Post | 0.4938 | 1.1155 | 1.8173 | 3.2052 |

持续和弦的平均耗时中位数分别降低约 **71.0%**（低音 Granular）、**63.0%**（155.6 Hz 起 Granular）、**76.3%**（低音 Off，仅 Unison）。Post 的三个配置本次全部观测块均低于预算，最高为 3.3630 ms；仍不能据此承诺任意宿主工程、采样率、多实例或长期调度都不会超时。Spectral 的最坏峰值包含操作系统调度影响，没有将它全部归因于算法。

这是一组有音色差异的两种设计比较。Granular/Post 只生成一份逐泛音调制，CPU 降低同时伴随不再为八路副本独立产生调制轨迹；Off 对照则单独展示取消重复泛音失谐合成的收益。改变最低音的结果也验证了按实际基频削减泛音已经在工作。

复现：

```powershell
$env:CARGO_HOME = Join-Path (Get-Location) 'target/cargo-home'
$env:RUSTUP_TOOLCHAIN = '1.92.0'
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo build -p spectral-dsp --example render_m4 --release --locked --offline
./target/release/examples/render_m4.exe target/fixtures/spectral-ab 256 48000 granular 8 curve 256 16 spectral 51
./target/release/examples/render_m4.exe target/fixtures/post-ab 256 48000 granular 8 curve 256 16 post 51
```

原始 18 次运行/72 场景日志、CSV/JSON 与试听 WAV 在 `target/perf-0.8.0/`。试听文件仍使用同样的两组四音和弦，且未触发文件防削波缩放（gain=1），可直接对比 `r1-root33-granular-spectral/polyphonic-stereo.wav` 与 `r1-root33-granular-post/polyphonic-stereo.wav`；它们不是十六音压力场景的录音。

## 内存与制品

调制 lane 池从 `16×1024×8×72 = 9,437,184 B` 降为 `16×512×8×72 = 4,718,592 B`（4.5 MiB）；Voice 池从 329,472 B 降为 165,632 B。48 kHz 下后处理缓冲/查表及两个湿声输出环合计约 68 KiB，另有少量结构元数据。这是固定缓冲预算，不是进程工作集。保留 Spectral 算法意味着两种模式都预留完整 lane 池，切换时不重新分配。

Windows x86-64 CLAP **0.8.0**，**1,722,368 字节**：

```text
target/artifacts/0.8.0/my_spectral_resonator.clap
target/bundled/my_spectral_resonator.clap
SHA-256: B65A9727084B90C77DC29E6101E1C50F9D65BD950F2AE6D0FBF3336F9BDC538C
```

稳定文件已原子替换、核对哈希并重新验证。旧 0.7.3 备份为 `target/bundled/f22b8bf5733e42078b456c2cbce2b3a1.previous`，SHA-256 为 `4122AE6D756D7E167A8D4308DE6572E07CA10CF55396B876E5C31C62C622BB03`。未关闭音频宿主，已有加载实例需要重新加载并确认版本 0.8.0。

试听时保持同一素材与参数，切换 Unison Mode，比较尾音、起音迟滞、失谐起伏与音量。独立的逐泛音调制、后处理湿声延迟和高频响应存在差别，不把较低 CPU 等同于更好的听感或 其他产品 复刻。
