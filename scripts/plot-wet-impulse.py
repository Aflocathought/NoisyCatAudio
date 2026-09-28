"""Plot probe_wet_timing CSVs; requires NumPy and Matplotlib (diagnostics only)."""
import argparse
import json
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path, help="directory containing zero/half/full CSV directories")
    parser.add_argument("output", type=Path, help="output prefix")
    parser.add_argument("--decay-seconds", type=float, default=2.0, help="decay used by the renderer (plot label)")
    args = parser.parse_args()
    delays = {"zero": 0, "half": 2048, "full": 4096}
    colors = {"zero": "#e7a34b", "half": "#1b9e8a", "full": "#976bd1"}
    names = {"zero": "0 windows", "half": "0.5 windows (+42.67 ms)", "full": "1 window (+85.33 ms)"}
    metrics = []
    fig, axes = plt.subplots(2, 1, figsize=(11, 7.5), sharex=True, layout="constrained")
    for ax, attack in zip(axes, ["natural", "fast"]):
        for offset in [0, 127, 255, 383, 511]:
            reference = np.loadtxt(args.input / "zero" / f"{attack}-offset-{offset}.csv", delimiter=",", skiprows=1)
            for setting, shift in delays.items():
                data = np.loadtxt(args.input / setting / f"{attack}-offset-{offset}.csv", delimiter=",", skiprows=1)
                # Compare independent renders, not an artificially shifted plot.
                # The raw samples must agree exactly, including both endpoints.
                np.testing.assert_array_equal(data[:, 3], reference[:, 3])
                expected = np.zeros(len(data))
                if shift:
                    expected[shift:] = reference[:-shift, 4]
                else:
                    expected[:] = reference[:, 4]
                np.testing.assert_array_equal(data[:, 4], expected)
                time = data[:, 1]
                wet = data[:, 4]
                peak = np.max(np.abs(wet))
                first = int(np.flatnonzero(np.abs(wet) >= peak * 0.001)[0])
                peak_at = int(np.argmax(np.abs(wet)))
                metrics.append({
                    "attack": attack, "hop_offset": offset, "setting": setting,
                    "delay_samples": shift, "onset_ms": float(time[first]),
                    "peak_ms": float(time[peak_at]),
                    "dry_peak_ms": float(time[np.argmax(np.abs(data[:, 3]))]),
                    "wet_peak_amplitude": float(peak),
                })
                if offset == 0:
                    # One millisecond RMS is only a display envelope. All timing
                    # statistics above use unfiltered raw samples, not this trace.
                    rms = np.sqrt(np.convolve(wet * wet, np.ones(48) / 48, mode="same"))
                    db = 20 * np.log10(np.maximum(rms / peak, 1e-6))
                    ax.plot(time, db, color=colors[setting], label=names[setting], linewidth=1.8)
        ax.axvline(0, color="#2781ce", linestyle="--", linewidth=1.5, label="Dry transient (0 ms)")
        ax.set_title("Natural attack" if attack == "natural" else "Independent attack: 0 ms", loc="left", weight="bold")
        ax.set_ylim(-65, 2)
        ax.set_xlim(-90, 220)
        ax.set_ylabel("Wet level / peak (dB)")
        ax.grid(alpha=0.2)
        ax.spines[["top", "right"]].set_visible(False)
    axes[0].legend(loc="lower right", fontsize=9)
    axes[1].set_xlabel("Time relative to the dry transient (ms)")
    fig.suptitle("Unit-impulse response: original, half-window and full-window wet delay", fontsize=14, weight="bold")
    fig.supxlabel(f"48 kHz | FFT 4096 / hop 512 | 440 Hz, 8 partials | T60 {args.decay_seconds:g} s | Mid Mix 50%\n"
                   "Plot: 1 ms RMS, hop offset 0. Metrics: raw samples across 5 hop phases.", fontsize=9)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(args.output.with_suffix(".png"), dpi=160)
    fig.savefig(args.output.with_suffix(".svg"))
    args.output.with_suffix(".json").write_text(json.dumps(metrics, indent=2), encoding="utf-8")
    for attack in ["natural", "fast"]:
        for setting in delays:
            cases = [m for m in metrics if m["attack"] == attack and m["setting"] == setting]
            print(f"{attack:7} {setting:4} onset_ms={min(m['onset_ms'] for m in cases):.3f}..{max(m['onset_ms'] for m in cases):.3f} "
                  f"peak_ms={min(m['peak_ms'] for m in cases):.3f}..{max(m['peak_ms'] for m in cases):.3f}")
    print("PASS: wet is exactly shifted; actual dry contribution is unchanged in all 30 renders.")


if __name__ == "__main__":
    main()
