# 0.13.0 FFT 设置与窗口切换验证

Settings 增加 `FFT size`，保存到工程/预设。用户可在低延迟与低频分辨率之间选择，默认保持 4096。此前提出的后续音头 Attack 行为本次不调整。

## 使用及延迟

| FFT 样本数 | hop 样本数 | 48 kHz 基础延迟 |
| --- | ---: | ---: |
| 1024 | 256 | 21.33 ms |
| 2048 | 512 | 42.67 ms |
| 3072 | 512 | 64.00 ms |
| 4096（默认） | 512 | 85.33 ms |

界面根据实际采样率换算毫秒，并显示活动窗口及等待宿主重启的状态。`fft_size` 为不可自动化的枚举参数，序列化 ID 为 `1024` / `2048` / `3072` / `4096`；旧工程缺失该字段时明确补入 4096，不继承当前实例先前的选择。其他隐藏参数保持原值。

选择修改后请求宿主重新激活。宿主执行之前继续使用旧引擎和旧延迟，不在音频回调中重新规划 FFT 或分配缓冲。重新激活会清空共鸣尾音；正在播放时切换可能有短暂中断。宿主尚未处理请求时界面显示等待提示。

表中是宿主报告的基础延迟。Wet alignment 仍按活动窗口的比例延后湿声，Post Unison 也可能增加湿声延迟；切换 FFT 不会自动更改这些参数。1024 使用四重重叠以减小合成波动，hop 比其余档位短，因此较短窗口不等于更低 CPU，尤其是多声部/多泛音场景。

## 实现

- `fft.rs` 集中保存窗口/hop 对应关系及活动窗口、采样率和重启状态。GUI 和音频回调观察已提交的参数，共用原子标志合并重启请求。
- nice-plug 的 GUI setter 会排队处理参数，因此请求重启发生在观察到参数实际更新之后；不在 setter 刚返回时使用尚未提交的值重建引擎。
- 3072 使用 RustFFT 混合基数路径。引擎/STFT 接受可被 hop 整除的偶数长度，调制模板跨 DC 的索引在非二次幂长度下改用欧几里得取模；二次幂保留原位掩码路径。参考合成的共轭索引也兼容 3072。
- 宿主延迟、尾长、Wet alignment 毫秒值、音头参考及分析器干声延迟跟随实际活动窗口。分析器延迟环预分配最大 4096 样本，显示用 4096/16384 点分析窗独立保留。
- 修正 vendored nice-plug 的激活通知顺序：先销毁延迟通知上下文，再发布 `is_activated`。此前首次激活或 FFT 重启中的延迟声明会再次请求重启；修正后每次有效 FFT 切换只请求一次。已经激活时改变延迟仍沿用框架的重启路径。

## 验证结果

最终候选制品和代码通过以下检查：

- Workspace：47 DSP + 44 插件 = 91 项通过，GPU 测试单独运行。日志 `target/fft-settings-tests.log`。
- 新回归覆盖四档窗口的真实双声道干声延迟、音频回调零分配、待重启期间保持旧引擎、GUI 重启合并、旧状态迁移、分析器干声对齐，以及 3072 下复音 Granular 和左右隔离。已有调制标量参考对比也加入 1024/2048/3072。
- Workspace/all-target Clippy `-D warnings` 通过，日志 `target/fft-settings-clippy.log`；格式及 diff 空白检查通过。release 构建仍有 vendored nice-plug 原有两个 unused-variable 提示。
- GPU Settings 预览通过，在 RTX 4060 Laptop / Vulkan 上生成 `target/ui-preview/fft-settings.png`，已检查布局。日志 `target/fft-settings-ui.log`。预览使用合成帧间隔，其 FPS 数字不代表实际宿主帧率。
- CLAP validator：36 passed、0 failed、0 warnings、8 skipped，日志 `target/fft-settings-validator.log`。
- 实际导出 CLAP 的参数切换测试通过：4096 → 2048 → 3072 → 1024 → 4096。每次只请求一次重启，初次激活/重新激活不额外请求；重启前保持旧延迟，重启后正确报告新延迟。覆盖参数非自动化属性、48/96 kHz 状态恢复、缺失 FFT 字段的旧工程。日志 `target/fft-settings-probe/switches.jsonl`。
- 四档 FFT 各测试 Internal/MIDI × Spectral/Post × 48/44.1/96 kHz，共 48 个离线渲染配置通过。模拟独立主线程/音频线程、播放到离线模式切换、状态快照、重激活、有限尾音排空及 WAV 输出。所有激活均报告对应 FFT 延迟且不额外重启。结果位于 `target/fft-settings-bounce-{1024,2048,3072,4096}/`。
- 默认 4096 的音频兼容性另使用与 0.12.0 基线相同的参数：Independent、Attack 0 ms、Decay 5 ms、Wet alignment 0.5。12 组浮点音频 SHA256 和 PCM WAV SHA256 全部一致。结果 `target/fft-settings-probe/bounce-legacy/`，对比 `target/fft-settings-probe/legacy-comparison.json`。前述四档矩阵使用默认 Natural / Decay 50 ms，不能直接与这组旧基线比较。

这些是源码、GPU 离屏和模拟宿主证据。本次未在真实音频宿主中验证菜单切换、实时负载或 Bounce，不据此宣称此前音频宿主挂起已解决。需要重新加载旧实例才能使用新制品。

## 制品与复现

Windows x86-64 CLAP：`target/artifacts/0.13.0/my_spectral_resonator.clap`，13,763,072 字节。

SHA256：`D131F120060D2D689EFD3AA1BFE7FF0B6117C29A8F42D074F3B72E599B019A16`。

稳定路径为 `target/bundled/my_spectral_resonator.clap`，发布与旧版备份记录见 `target/fft-settings-probe/publication.json`。

项目使用 Rust 1.95.0，设置项目本地 `CARGO_HOME=target/cargo-home` 后可用缓存离线执行：

```powershell
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings
cargo test -p spectral-resonator-plugin render_gpu_preview --locked --offline -- --ignored --nocapture
python scripts/probe-clap-fft.py target/artifacts/0.13.0/my_spectral_resonator.clap --output target/fft-settings-probe
python scripts/probe-clap-bounce.py target/artifacts/0.13.0/my_spectral_resonator.clap --fft-size 3072 --require-tail-drain --output target/fft-settings-bounce-3072
python scripts/probe-clap-bounce.py target/artifacts/0.13.0/my_spectral_resonator.clap --fft-size 4096 --attack-ms 0 --decay-seconds 0.005 --wet-alignment 0.5 --require-tail-drain --output target/fft-settings-probe/bounce-legacy
```
