"""Summarize evaluate_windows CSVs and plot the latency/quality/CPU tradeoff."""
import argparse
import csv
import json
import statistics
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np


CONFIGS = [(4096, 512), (2048, 256), (1024, 128), (2048, 512), (1024, 256), (1024, 512)]


def read_rows(root, name):
    with (root / name).open(newline="", encoding="utf-8") as source:
        return list(csv.DictReader(source))


def matching(rows, size, hop, **filters):
    return [r for r in rows if int(r["size"]) == size and int(r["hop"]) == hop
            and all(r[key] == str(value) for key, value in filters.items())]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    root = args.directory
    tails = read_rows(root, "tails.csv")
    impulses = read_rows(root, "impulses.csv")
    selectivity = read_rows(root, "selectivity.csv")
    performance = read_rows(root, "performance.csv")
    summaries = []
    for size, hop in CONFIGS:
        tail = matching(tails, size, hop)
        short = matching(impulses, size, hop, mode="short")
        item = {
            "size": size, "hop": hop, "latency_ms": size / 48,
            "bin_spacing_hz": 48000 / size,
            "max_abs_pitch_cents": max(abs(float(r["pitch_cents"])) for r in tail),
            "max_abs_t60_error_percent": max(abs(float(r["t60_error_percent"])) for r in tail),
            "worst_tail_residual_db": max(float(r["residual_db"]) for r in tail),
            "worst_tail_residual_note50_up_db": max(float(r["residual_db"]) for r in tail if int(r["note"]) >= 50),
            "short_impulse_width90_ms_mean": statistics.mean(float(r["width90_ms"]) for r in short),
            "short_impulse_onset_ms_range": [min(float(r["onset_ms"]) for r in short), max(float(r["onset_ms"]) for r in short)],
            "selectivity": [], "performance": {},
        }
        for note in [33, 50, 69]:
            for mode in ["natural", "fast", "short"]:
                response = matching(selectivity, size, hop, note=note, mode=mode)
                reference = next(float(r["wet_rms"]) for r in response if float(r["input_ratio"]) == 1)
                for row in response:
                    ratio = float(row["input_ratio"])
                    if ratio != 1:
                        item["selectivity"].append({
                            "note": note, "mode": mode, "input_ratio": ratio,
                            "relative_db": float(20 * np.log10(float(row["wet_rms"]) / reference)),
                        })
        for scenario in ["single", "16voice-8spectral", "16voice-8post"]:
            samples = matching(performance, size, hop, scenario=scenario)
            item["performance"][scenario] = {
                "median_wall_ms_per_500ms": statistics.median(float(r["wall_ms"]) for r in samples),
                "median_p99_block_ms": statistics.median(float(r["p99_block_ms"]) for r in samples),
                "max_block_ms": max(float(r["max_block_ms"]) for r in samples),
            }
        summaries.append(item)
    (root / "summary.json").write_text(json.dumps(summaries, indent=2), encoding="utf-8")

    fig, axes = plt.subplots(2, 1, figsize=(10.5, 8), layout="constrained")
    colors = ["#777f8e", "#188b91", "#d18b28"]
    for (size, hop), color in zip([(4096, 512), (2048, 512), (1024, 256)], colors):
        data = np.loadtxt(root / f"impulse-{size}-{hop}.csv", delimiter=",", skiprows=1)
        # Display a 1 ms RMS envelope, normalized independently for width only.
        # Statistics and saved WAVs use the original unnormalized samples.
        envelope = np.sqrt(np.convolve(data[:, 1] ** 2, np.ones(48) / 48, mode="same"))
        level = 20 * np.log10(np.maximum(envelope / envelope.max(), 1e-6))
        axes[0].plot(data[:, 0], level, color=color, linewidth=2, label=f"N={size}, H={hop}")
    axes[0].axvline(0, color="#444444", linestyle="--", linewidth=1)
    axes[0].set(xlim=(-85, 85), ylim=(-65, 2), xlabel="Time relative to delayed dry impulse (ms)",
                ylabel="Normalized envelope (dB)", title="Short decay: a smaller window reduces temporal spread")
    axes[0].legend(loc="lower right")

    positions = np.arange(len(CONFIGS))
    for shift, scenario, color, label in [
        (-0.18, "16voice-8spectral", "#188b91", "16 voices / 8 Spectral Unison"),
        (0.18, "16voice-8post", "#d18b28", "16 voices / 8 Post Unison"),
    ]:
        key = "median_wall_ms_per_500ms"
        baseline = summaries[0]["performance"][scenario][key]
        ratios = [s["performance"][scenario][key] / baseline for s in summaries]
        bars = axes[1].bar(positions + shift, ratios, width=0.34, color=color, label=label)
        axes[1].bar_label(bars, labels=[f"{v:.2f}x" for v in ratios], fontsize=9, padding=3)
    axes[1].axhline(1, color="#444444", linestyle="--", linewidth=1)
    axes[1].set_xticks(positions, [f"{n}/{h}\n{n // h} overlaps" for n, h in CONFIGS])
    axes[1].set(ylim=(0, 4.5), xlabel="FFT size / hop size", ylabel="Time / matching 4096 baseline",
                title="Heavy processing: keeping 8 overlaps increases CPU cost")
    axes[1].legend(loc="upper right", fontsize=9)
    for ax in axes:
        ax.grid(axis="y", alpha=0.2)
        ax.set_axisbelow(True)
        ax.spines[["top", "right"]].set_visible(False)
    fig.suptitle("Window-size evaluation at 48 kHz", fontsize=16, weight="bold")
    fig.supxlabel("Top: 440 Hz, 8 partials, T60 5 ms, Natural, alignment 0; each trace normalized.\n"
                  "Bottom: release offline DSP, Granular, median of 3 trials; not a Bitwig real-time guarantee.", fontsize=9)
    fig.savefig(root / "comparison.png", dpi=160)
    fig.savefig(root / "comparison.svg")
    for item in summaries:
        print(f"N={item['size']} H={item['hop']}: residual worst {item['worst_tail_residual_db']:.2f} dB, "
              f"note50+ {item['worst_tail_residual_note50_up_db']:.2f} dB; "
              f"performance {item['performance']}")


if __name__ == "__main__":
    main()
