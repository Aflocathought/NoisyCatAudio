# 低音区相邻琴键的彩虹响应图

用户要求直接观察 2048 点在低音区的相邻琴键交叠范围。此次仅新增诊断程序和作图脚本，未修改生产 DSP 或发布插件。

## 方法

`crates/spectral-dsp/examples/sweep_low_keys.rs` 直接调用当前 `SpectralResonator`，分别使用 N/H=2048/512 和 4096/512。48 kHz、单声道、单泛音、Independent Attack 0 ms、Decay/T60 2 s、无 damping、Motion Off、单路 Spectral Unison、分频 1/12000 Hz、Wet Level 1、Mix 100%、Wet Alignment 0，关闭 Reshape。

目标为十二平均律 MIDI 48–54（以 C4=MIDI 60 命名即 C3–F#3，130.81–185.00 Hz）。每个目标单独扫频，输入范围 100–220 Hz、间隔 1 Hz，并加入七个琴键的精确频率，共 128 个输入频率。每次 reset 后输入幅度 0.1 的正弦 2 s，取最后 0.5 s 湿声 RMS。共 1792 次快速激励测量。

另测 2048/512 的 Natural D3，D3 上下 25 Hz 以 0.25 Hz 步长采样，并加入其他琴键中心，共 207 次，用来显示攻击模式的影响。

每条曲线除以该共鸣目标在同频输入下的 RMS，得到相对幅度。彩色面积是分别测量后的叠绘，不是七声部同时运行的混合能量；输入频率也不会自动开启对应 MIDI 声部。−6 dB 对应约 50.12% 幅度、25.12% 能量。主瓣阈值交点使用相邻实测点线性插值；图没有额外平滑。

## 结果

| D3 目标，146.832 Hz | −6 dB 输入频率范围 | 宽度 |
| --- | ---: | ---: |
| 2048/512，Independent 0 ms | 130.8–162.3 Hz | 31.5 Hz |
| 4096/512，Independent 0 ms | 138.5–155.1 Hz | 16.6 Hz |
| 2048/512，Natural | 145.88–147.79 Hz | 约 1.91 Hz |

固定输入 D3 时，2048 快速激励下 C3、C#3、D3、D#3、E3、F3、F#3 的相对响应分别为 50.9%、83.3%、100%、81.5%、43.2%、18.1%、6.7%。这些值分别相对各目标自己的同频响应，而非相对七个目标的总输出。

图刻意采用容易暴露交叠的快速激励模式，不能代表全部 Attack/Decay 配置。Natural 长尾明显更窄，也不等于实时响应同样迅速。以上为离线稳态单泛音测量，不是琴声录音、瞬态混合素材或 Bitwig 主观听感验收。

## 复现与检查

```powershell
# 使用项目已有的 Rust 1.95.0 / 离线 Cargo 环境
cargo run -p spectral-dsp --example sweep_low_keys --release --locked --offline -- target/low-key-overlap
# 需要 NumPy、Matplotlib；默认中文字体为 Windows Microsoft YaHei
python scripts/plot-low-key-overlap.py target/low-key-overlap
```

全部 1999 个测量已完成、输出均有限。诊断程序 Clippy `-D warnings`、工作区格式检查通过。D3 目标被 D#3 输入激励的两个配置，与之前 `evaluate_windows` 的独立测量差异均低于 0.001 dB。最终 PNG 已目视检查标注、字体与布局。

产物位于 `target/low-key-overlap/`：`sweep.csv`、`metrics.json`、`rainbow-overlap.png`、`rainbow-overlap.svg`。可用诊断程序及作图脚本重新生成，未执行提交。

## 连续两个八度扩展

按用户要求，另生成 C3–C5（MIDI 48–72，130.81–523.25 Hz）的 25 个琴键中心，覆盖 24 个连续半音间隔。图宽由 2240 加倍至 4480 像素，高度仍为 1600 像素，保留 2048/512 与 4096/512 的主图对照，以及 Natural D3 的参考曲线。

新主图的输入范围为 100–560 Hz，均匀间隔 2 Hz，再加入所有目标琴键的精确频率；A3=220 Hz、A4=440 Hz 去重后，每条曲线为 254 点。两种配置共 12,700 次快速激励测量，加 207 次 Natural 测量，共 12,907 次。每次测量的其余条件与前图一致，未从低音曲线平移推算高音曲线。

右侧另列出高一个半音输入对目标 D3、D4、A4 的相对响应：2048 点为 81.0%、44.7%、19.9%；4096 点为 45.9%、3.5%、0.3%。横轴保留线性 Hz 刻度，直观显示相邻半音的频率间隔随音高增加而变大。

```powershell
cargo run -p spectral-dsp --example sweep_low_keys --release --locked --offline -- target/two-octave-overlap --two-octaves
python scripts/plot-low-key-overlap.py target/two-octave-overlap --two-octaves
```

全部测量完成，所有曲线点数和有限值检查通过；更新后的诊断程序 Clippy、Rust 格式检查通过。新图已目视检查。产物为 `target/two-octave-overlap/rainbow-two-octaves.png`、同名 SVG，以及原始 `sweep.csv` 与 `metrics.json`。上一张图与原始测量保留。
