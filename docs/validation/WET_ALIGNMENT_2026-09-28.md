# 0.11.2 连续湿声预延迟与单位脉冲验证

用户希望尝试开关两端之间的半窗位置，并要求用类似狄拉克函数的信号验证。使用离散单位脉冲 `x[n0]=1`、其余样本为零，直接渲染引擎的实际干/湿输出。结论是：半窗确实精确平移湿声 2048 样本，但不能称为普遍的音头对齐。原始响应已经有窗口前响；长 Decay 还会使主峰建立得更晚。

## 参数与 DSP

- `align_wet` 保留宿主参数 ID，改为浮点 0–1 个窗口，新实例默认 0.5。Resonance 页使用同样的紧凑旋钮，旁边显示窗口比例和实际毫秒数。
- 4096 点窗口下，0.5 = 2048 samples，在 48/44.1/96 kHz 下分别为 42.67/46.44/21.33 ms。其他比例按最近整数样本选取延迟；稳定时保留湿声原始波形。
- 延迟位于 Post Unison 后，预分配 4097 帧立体声环，约 32 KiB。半窗读取真正的中间位置，不是把原始和整窗信号各混一半。
- 改变参数时在两个固定读点之间淡变 20 ms，避免突然换点或移动读头导致的移调。快速自动化先完成当前淡变，再跟随最新值，最坏约两次淡变时间后到达最终值。淡变期间可能有短暂叠音/相消。
- 首次处理和 reset 后直接选择目标读点；reset 清空环，调节参数不清除共鸣尾音。音频线程无新增分配、锁、文件操作或 FFT。
- 干声与宿主报告的 4096 样本基础延迟不变。主输出、辅助 Wet 和白色频谱都使用同一延后湿声；尾长继续保守包含一整窗，以覆盖任意参数值及切换。
- 旧版保存的布尔 `false/true` 分别迁移为浮点 `0/1`；缺失值补为 0.5；新版浮点值原样保留。旧实例如果此前开启开关，需手动设为 **0.5 windows** 才是半窗。

## 单位脉冲结果

配置：48 kHz，FFT 4096 / hop 512，440 Hz 基音，8 泛音，HF/LF damping=0，Wet Level=1，Middle Mix=50%，低/高分频为 1/12000 Hz，Motion Off、Spectral Unison 单路。脉冲置于 `8192 + offset`，offset 为 0、127、255、383、511，覆盖五个 hop 相位。时间零点是输入位置加 4096，实测干声主峰也在此处。

每种设置独立渲染，不是在图中事后平移曲线。原始采样检查证实全部 60 次渲染中，半窗/整窗湿声分别等于原始湿声精确延后 2048/4096 样本，干声逐样本不变。

下表时间均相对干声主峰，负号表示提前。起声阈值取湿声原始采样峰值的 0.001，即 −60 dB；它是统一测量阈值，不是人耳可闻阈值。主峰是绝对采样值的最大值，不是 RMS 曲线的峰值。

| Decay / Attack | Wet alignment | −60 dB 起声位置（ms） | 主峰位置（ms） |
| --- | --- | ---: | ---: |
| 2 s / Natural | 0 | −65.94 至 −65.92 | +34.08 |
| 2 s / Natural | 0.5 | −23.27 至 −23.25 | +76.75 |
| 2 s / Natural | 1 | +19.40 至 +19.42 | +119.42 |
| 2 s / Independent 0 ms | 0 | −70.50 至 −68.25 | +20.46 |
| 2 s / Independent 0 ms | 0.5 | −27.83 至 −25.58 | +63.13 |
| 2 s / Independent 0 ms | 1 | +14.83 至 +17.08 | +105.79 |
| 5 ms / Natural 或 Independent 0 ms | 0 | −70.52 至 −68.27 | 0.00 |
| 5 ms / Natural 或 Independent 0 ms | 0.5 | −27.85 至 −25.60 | +42.67 |
| 5 ms / Natural 或 Independent 0 ms | 1 | +14.81 至 +17.06 | +85.33 |

短 Decay 用例尤其说明：原始主峰已经对齐，仍然存在前响。半窗减少干声前的响应长度，但同时移动主峰。它是可试听的折中，不能同时完成“去掉前响”和“保持主峰位置”。不应把半窗时仍存在的提前响应误判为延迟环失效。

图像使用 1 ms RMS 便于观察，量化指标由未平滑采样计算。图像/CSV/JSON 位于：

- `target/alignment-0.11.2/impulse-response.png`、`.svg`、`.json`：2 s Decay。
- `target/alignment-0.11.2/impulse-short-response.png`、`.svg`、`.json`：5 ms Decay。
- `target/alignment-0.11.2/impulse/` 与 `impulse-short/`：各 30 个 CSV，包含输入、实际干声及湿声。

这些数据针对上述脉冲与设置，不是对所有音色、Post Unison 可变延迟或真实音频宿主听感的结论。

## 验证与制品

- Workspace 测试含实际 GPU 绘制：44 DSP + 38 插件 = 82 项通过，`target/alignment-tests.log`。
- 随后强化固定读点淡变的跳变幅度检查：2 项延迟测试通过；在已有参数自动化回归加入 0/0.5/1/0.37/0，分块不变性、立体声与无分配检查通过。见 `delay-tests.log`、`automation-tests.log`。
- 全目标/功能 Clippy `-D warnings`、格式与 diff 空白检查通过。release 保留 vendored nice-plug 原有两个 unused-variable 提示。
- 实际 CLAP 状态加载/保存验证旧 bool 的两端迁移及 0.5/0.37123 精度，再次加载不变：`abi-migration.log`。
- CLAP validator：36 passed、0 failed、0 warnings、8 skipped，`validator.log`。
- 默认半窗、Attack 0、Decay 5 ms，Internal/MIDI × Spectral/Post × 48/44.1/96 kHz 的 12 个模拟 Bounce 配置通过，含尾音排空和重启。见 `bounce/`。
- 实际测试窗口三次打开/缩放/关闭通过，音频处理 2175 块。Space 普通/重复消息、文本捕获及编辑后返回宿主通过，宿主收到 9 条预期空格消息。见 `keyboard.log`。
- GPU 预览 `resonance.png` 已目视检查，半窗与 42.67 ms 正常显示。

以上未单独注明的日志均位于 `target/alignment-0.11.2/`。实际音频宿主加载、听感和 Bounce 仍需宿主内验收。

版本副本：`target/artifacts/0.11.2/my_spectral_resonator.clap`，13,750,784 字节，SHA256 `6A3E02AE34D2FBC15F906BC95EFBBE2528CC535AD3E4FB92FC357CFCC2F39ED5`。稳定文件更新及旧版备份信息见 `target/alignment-0.11.2/publication.json`。

## 复现

```powershell
cargo run -p spectral-dsp --example probe_wet_timing --release --locked --offline -- --alignment 0.5 --decay-seconds 0.005 --csv-dir target/impulse-half
python scripts/probe-clap-bounce.py target/artifacts/0.11.2/my_spectral_resonator.clap --attack-ms 0 --decay-seconds 0.005 --wet-alignment 0.5 --require-tail-drain --output target/alignment-0.11.2/bounce
```

`scripts/plot-wet-impulse.py` 读取 `zero/half/full` 三组目录并验证原始样本后生成图像。绘图需要 NumPy 和 Matplotlib；本次仅安装在被 Git 忽略的 `target/plot-deps`，不改变插件依赖或系统 Python。
