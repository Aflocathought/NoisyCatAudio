"""Plot measured DSP excitation curves, colored by equal-tempered piano key."""
import argparse
import csv
import json
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib import colors as mpl_colors, font_manager
import numpy as np


NOTES = range(48, 55)
NAMES = ["C3", "C♯3", "D3", "D♯3", "E3", "F3", "F♯3"]
COLORS = ["#e84c60", "#e98a24", "#b99f10", "#26a66a", "#19a3b4", "#4377d5", "#9254ce"]
HALF_AMPLITUDE = 10 ** (-6 / 20)


def frequency(note):
    return 440 * 2 ** ((note - 69) / 12)


def curve(rows, size, note, mode="fast"):
    samples = sorted(
        ((float(r["input_hz"]), float(r["wet_rms"])) for r in rows
         if int(r["size"]) == size and int(r["note"]) == note and r["mode"] == mode)
    )
    data = np.array(samples)
    target = frequency(note)
    # Normalize to the measured response at exactly this key's own frequency,
    # not the global peak, not the combined energy of the seven colored curves.
    center = np.flatnonzero(np.isclose(data[:, 0], target, atol=1e-9, rtol=0))
    assert len(center) == 1
    return data[:, 0], data[:, 1] / data[center[0], 1]


def width(x, y, center, level):
    """Interpolate the crossings enclosing the target, ignoring distant lobes."""
    at = int(np.argmin(abs(x - center)))
    left = right = at
    while left > 0 and y[left] >= level:
        left -= 1
    while right < len(x) - 1 and y[right] >= level:
        right += 1
    assert y[left] < level and y[right] < level
    low = x[left] + (level - y[left]) / (y[left + 1] - y[left]) * (x[left + 1] - x[left])
    high = x[right - 1] + (level - y[right - 1]) / (y[right] - y[right - 1]) * (x[right] - x[right - 1])
    return [float(low), float(high)]


def plot_two_octaves(rows, args):
    """Keep the original graph intact and render a separately measured wide view."""
    notes = list(range(48, 73))
    pitch_names = ["C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B"]
    names = {n: f"{pitch_names[n % 12]}{n // 12 - 1}" for n in notes}
    colors = {n: mpl_colors.hsv_to_rgb((i / 24 * 0.8, 0.76, 0.80)) for i, n in enumerate(notes)}
    # 231 even-Hz points plus 25 key centers; A3=220 and A4=440 are duplicates.
    assert len(rows) == 2 * 25 * 254 + 207, "Incomplete two-octave sweep"
    metrics = {"note_range": [48, 72], "frequency_range_hz": [frequency(48), frequency(72)],
               "canvas_pixels": [4480, 1600], "sizes": {}}
    fig = plt.figure(figsize=(28, 10), facecolor="#fafbfe")
    grid = fig.add_gridspec(2, 2, width_ratios=[6.5, 1], hspace=0.37, wspace=0.15)
    fig.subplots_adjust(left=0.035, right=0.98, top=0.84, bottom=0.18)
    for row, size in enumerate([2048, 4096]):
        ax = fig.add_subplot(grid[row, 0])
        metrics["sizes"][str(size)] = {}
        for note in notes:
            x, y = curve(rows, size, note)
            assert len(x) == 254 and np.all(np.isfinite(y))
            ax.fill_between(x, y * 100, color=colors[note], alpha=0.13, linewidth=0)
            ax.plot(x, y * 100, color=colors[note], lw=2.2 if note in [48, 60, 72] else 1.55)
            # Alternate label height for black keys, preserving their true Hz positions.
            label_y = 116 if "♯" in names[note] else 105
            ax.text(frequency(note), label_y, names[note], color=colors[note], fontsize=10.5,
                    ha="center", va="bottom", weight="bold")
            low, high = width(x, y, frequency(note), HALF_AMPLITUDE)
            metrics["sizes"][str(size)][names[note]] = {"note": note, "target_hz": frequency(note),
                "minus6db_hz": [low, high], "width_hz": high - low}
        for note in [48, 60, 72]:
            ax.axvline(frequency(note), color="#29354b", linestyle="--", lw=1, alpha=0.4)
        ax.axhline(HALF_AMPLITUDE * 100, color="#8b98ab", linestyle=":", lw=1.1)
        ax.text(103, HALF_AMPLITUDE * 100 + 2, "−6 dB", fontsize=9, color="#657188")
        ax.set(xlim=(100, 560), ylim=(0, 127), yticks=[0, 25, 50, 75, 100],
               xticks=np.arange(100, 561, 20), xlabel="输入正弦的频率（Hz，线性刻度）",
               ylabel="湿声幅度 / 同频输入响应（%）")
        ax.tick_params(labelsize=10)
        ax.set_title(f"{'①' if row == 0 else '③'}  {size} 点 / hop 512     基础延迟 {size / 48:.2f} ms     "
                     f"{'从低音区的宽交叠，逐渐过渡到更容易区分的相邻琴键' if size == 2048 else '当前 4096 点对照：相同输入频率范围，相同纵轴'}",
                     loc="left", fontsize=14, pad=13, weight="bold", color="#17243a")
        ax.grid(axis="y", alpha=0.14)
        ax.spines[["top", "right"]].set_visible(False)

    bars = fig.add_subplot(grid[0, 1])
    examples = [50, 62, 69]
    positions = np.arange(3)
    metrics["semitone_examples"] = {}
    for size, shift, color in [(2048, -0.17, "#238e9c"), (4096, 0.17, "#9ba8bb")]:
        values = []
        for note in examples:
            x, y = curve(rows, size, note)
            at = np.flatnonzero(np.isclose(x, frequency(note + 1), rtol=0, atol=1e-9))
            assert len(at) == 1
            values.append(float(y[at[0]] * 100))
        metrics["semitone_examples"][str(size)] = dict(zip([names[n] for n in examples], values))
        patches = bars.barh(positions + shift, values, height=0.29, color=color, label=f"{size} 点")
        bars.bar_label(patches, labels=[f"{v:.1f}%" for v in values], fontsize=10, padding=3)
    bars.set_yticks(positions, [f"{names[n]}\n{frequency(n):.1f} Hz" for n in examples], fontsize=10)
    bars.invert_yaxis()
    bars.set(xlim=(0, 115), xticks=[0, 50, 100], xlabel="相对同频输入的响应（%）")
    bars.legend(loc="lower right", fontsize=10)
    bars.spines[["top", "right", "left"]].set_visible(False)
    bars.tick_params(axis="y", length=0)
    bars.set_title("②  输入比目标高一个半音\n频率越高，抑制通常越强", loc="left", fontsize=12, pad=13, weight="bold", color="#17243a")

    compare = fig.add_subplot(grid[1, 1])
    x, fast = curve(rows, 2048, 50)
    compare.plot(x, fast * 100, color=colors[50], lw=2, label="Independent 0 ms")
    nx, natural = curve(rows, 2048, 50, "natural")
    compare.plot(nx, natural * 100, color="#28354b", lw=1.8, label="Natural")
    compare.fill_between(nx, natural * 100, color="#28354b", alpha=0.09)
    compare.set(xlim=(135, 160), ylim=(0, 112), yticks=[0, 50, 100], xlabel="输入频率（Hz）", ylabel="相对响应（%）")
    compare.set_title("④  模式仍然很重要\n2048 点、D3、Decay 2 s", loc="left", fontsize=12, pad=13, weight="bold", color="#17243a")
    compare.legend(fontsize=9, loc="lower right")
    compare.grid(axis="y", alpha=0.14)
    compare.spines[["top", "right"]].set_visible(False)

    fig.suptitle("连续两个八度的共鸣交叠：C3 → C4 → C5", fontsize=24, weight="bold", x=0.035, ha="left", y=0.965, color="#17243a")
    fig.text(0.035, 0.911, "25 个琴键中心，覆盖 24 个半音间隔｜MIDI 48–72｜130.81–523.25 Hz｜红色到紫色逐键变化；三个 C 的虚线标出八度。",
             fontsize=13, color="#536078")
    fig.text(0.035, 0.093, "实际 DSP 扫频｜48 kHz｜Independent Attack 0 ms｜Decay 2 s｜单泛音、无调制、Wet Alignment 0｜每个输入频率持续 2 s，测最后 0.5 s RMS。",
             fontsize=12, color="#29354b")
    fig.text(0.035, 0.049, "彩色曲线按各自的同频输入响应归一化，分别测量后叠绘；填色不代表能量相加，−6 dB 约为一半幅度。主图网格为 2 Hz，并额外测量每个琴键的精确频率。\n"
             "横轴保持线性：随着音高上升，相邻半音之间的 Hz 间隔变大。图示为快速激励场景；右下 Natural 长尾对照说明，不能把宽曲线推广到全部 Attack/Decay 设置。",
             fontsize=11, color="#657188", linespacing=1.7)
    fig.savefig(args.directory / "rainbow-two-octaves.png", dpi=160, facecolor=fig.get_facecolor())
    fig.savefig(args.directory / "rainbow-two-octaves.svg", facecolor=fig.get_facecolor())
    (args.directory / "metrics.json").write_text(json.dumps(metrics, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps(metrics["semitone_examples"], ensure_ascii=True, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--font", type=Path, default=Path("C:/Windows/Fonts/msyh.ttc"))
    parser.add_argument("--two-octaves", action="store_true", help="plot C3 through C5 on a twice-wide canvas")
    args = parser.parse_args()
    font_manager.fontManager.addfont(str(args.font))
    plt.rcParams.update({
        "font.family": [font_manager.FontProperties(fname=str(args.font)).get_name(), "DejaVu Sans"],
        "axes.unicode_minus": False, "font.size": 10, "svg.fonttype": "path",
    })
    with (args.directory / "sweep.csv").open(encoding="utf-8", newline="") as file:
        rows = list(csv.DictReader(file))
    if args.two_octaves:
        plot_two_octaves(rows, args)
        return
    # The Natural grid already contains D3, so its seven exact key centers add six rows.
    assert len(rows) == 2 * 7 * 128 + 207, "Incomplete sweep; wait for measurement to finish"
    metrics = {}
    fig = plt.figure(figsize=(14, 10), facecolor="#fafbfe")
    grid = fig.add_gridspec(2, 2, width_ratios=[3.1, 1], hspace=0.37, wspace=0.27)
    fig.subplots_adjust(left=0.07, right=0.97, top=0.85, bottom=0.19)
    axes = []
    for row, size in enumerate([2048, 4096]):
        ax = fig.add_subplot(grid[row, 0])
        axes.append(ax)
        for note, name, color in zip(NOTES, NAMES, COLORS):
            x, y = curve(rows, size, note)
            ax.fill_between(x, y * 100, color=color, alpha=0.11, linewidth=0)
            ax.plot(x, y * 100, color=color, lw=2.2 if note == 50 else 1.6)
            ax.text(frequency(note), 104, name, ha="center", va="bottom", color=color, fontsize=10, weight="bold")
        ax.axvline(frequency(50), color="#29354b", linestyle="--", lw=1.2, alpha=0.8)
        ax.axhline(HALF_AMPLITUDE * 100, color="#aab1be", linestyle=":", lw=1)
        x, y = curve(rows, size, 50)
        low, high = width(x, y, frequency(50), HALF_AMPLITUDE)
        metrics[str(size)] = {"d3_minus6db_hz": [low, high], "d3_width_hz": high - low, "neighbors_at_d3": {}}
        for note, name in zip(NOTES, NAMES):
            cx, cy = curve(rows, size, note)
            response = float(cy[np.flatnonzero(np.isclose(cx, frequency(50), rtol=0, atol=1e-9))[0]])
            metrics[str(size)]["neighbors_at_d3"][name] = response
        ax.annotate("", xy=(low, 43), xytext=(high, 43), arrowprops={"arrowstyle": "<->", "color": "#29354b", "lw": 1.5})
        ax.text(101.5, 91, f"D3 的 −6 dB 范围\n{low:.1f}–{high:.1f} Hz\n宽约 {high - low:.1f} Hz", fontsize=10,
                ha="left", va="top", color="#29354b", bbox={"facecolor": "white", "edgecolor": "none", "alpha": 0.92, "pad": 4})
        ax.set(xlim=(100, 220), ylim=(0, 117), yticks=[0, 25, 50, 75, 100],
               xlabel="输入正弦的频率（Hz）", ylabel="湿声幅度 / 同频输入响应（%）")
        ax.set_title(f"{'①' if row == 0 else '③'}  {size} 点 / hop 512：{'相邻琴键大范围交叠' if row == 0 else '当前窗口对照，曲线更窄'}",
                     loc="left", fontsize=13, pad=13, weight="bold", color="#17243a")
        ax.grid(axis="y", alpha=0.14)
        ax.spines[["top", "right"]].set_visible(False)

    bars = fig.add_subplot(grid[0, 1])
    responses = list(metrics["2048"]["neighbors_at_d3"].values())
    bars.barh(np.arange(7), np.array(responses) * 100, color=COLORS, height=0.65, alpha=0.9)
    bars.set_yticks(np.arange(7), [f"{name}  {frequency(note):.1f}" for note, name in zip(NOTES, NAMES)], fontsize=9)
    bars.invert_yaxis()
    for i, response in enumerate(responses):
        bars.text(response * 100 + 2, i, f"{response * 100:.0f}%", va="center", fontsize=9, color="#29354b")
    bars.set(xlim=(0, 123), xticks=[0, 50, 100], xlabel="各琴键的相对响应（%）")
    bars.axvline(50, color="#aab1be", linestyle=":", lw=1)
    bars.spines[["top", "right", "left"]].set_visible(False)
    bars.tick_params(axis="y", length=0)
    bars.set_title("②  固定输入 D3\n146.83 Hz 同时落入哪些曲线？", loc="left", fontsize=11, pad=13, weight="bold", color="#17243a")

    compare = fig.add_subplot(grid[1, 1])
    x, fast = curve(rows, 2048, 50)
    compare.plot(x, fast * 100, color=COLORS[2], lw=2, label="Independent 0 ms")
    nx, natural = curve(rows, 2048, 50, "natural")
    compare.plot(nx, natural * 100, color="#28354b", lw=1.8, label="Natural")
    compare.fill_between(nx, natural * 100, color="#28354b", alpha=0.09)
    low, high = width(nx, natural, frequency(50), HALF_AMPLITUDE)
    metrics["natural_2048"] = {"d3_minus6db_hz": [low, high], "d3_width_hz": high - low}
    compare.set(xlim=(135, 160), ylim=(0, 112), yticks=[0, 50, 100], xlabel="输入频率（Hz）", ylabel="相对响应（%）")
    compare.set_title("④  模式也会改变选择性\n同为 2048 点、D3、Decay 2 s", loc="left", fontsize=11, pad=13, weight="bold", color="#17243a")
    compare.legend(fontsize=8, loc="lower right")
    compare.grid(axis="y", alpha=0.14)
    compare.spines[["top", "right"]].set_visible(False)

    fig.suptitle("2048 点在低音区：同一输入，会激发多少个相邻琴键？", fontsize=20, weight="bold", x=0.07, ha="left", y=0.965, color="#17243a")
    fig.text(0.07, 0.911, "七个十二平均律琴键 C3–F♯3（MIDI 48–54，130.81–185.00 Hz）；每种颜色代表一个目标共鸣频率。",
             fontsize=11, color="#536078")
    fig.text(0.07, 0.096, "实际 DSP 扫频｜48 kHz｜Independent Attack 0 ms｜Decay 2 s｜单泛音、无调制、Wet Alignment 0",
             fontsize=10, color="#29354b")
    fig.text(0.07, 0.056, "各琴键单独测量后叠绘；每条曲线按本琴键同频输入的响应归一化，填色不代表实际能量相加。−6 dB ≈ 一半幅度。\n"
             "每频率输入 2 s，取最后 0.5 s RMS。①③为快速激励场景；④说明 Natural 长尾的选择性更强，不能把宽曲线推广到所有模式。",
             fontsize=9, color="#657188", linespacing=1.7)
    fig.savefig(args.directory / "rainbow-overlap.png", dpi=160, facecolor=fig.get_facecolor())
    fig.savefig(args.directory / "rainbow-overlap.svg", facecolor=fig.get_facecolor())
    (args.directory / "metrics.json").write_text(json.dumps(metrics, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps(metrics, ensure_ascii=True, indent=2))


if __name__ == "__main__":
    main()
