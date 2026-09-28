"""Plot real wet-only reshape renders; requires NumPy and Matplotlib."""
import argparse
import json
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    metrics = []
    fig, axes = plt.subplots(2, 1, figsize=(10, 7), layout="constrained")
    settings = [("baseline", "Independent / 0 ms", "#d59132"),
                ("sharp", "Reshape / 0 ms", "#1b9e8a"),
                ("medium", "Reshape / 10 ms", "#2781ce"),
                ("soft", "Reshape / 100 ms", "#976bd1")]
    base_peak = None
    for setting, label, color in settings:
        cases = []
        for offset in [0, 127, 255, 383, 511]:
            data = np.loadtxt(args.input / setting / f"fast-offset-{offset}.csv", delimiter=",", skiprows=1)
            baseline = np.loadtxt(args.input / "baseline" / f"fast-offset-{offset}.csv", delimiter=",", skiprows=1)
            np.testing.assert_array_equal(data[:, 3], baseline[:, 3])
            t, wet = data[:, 1], data[:, 4]
            peak = np.max(np.abs(wet))
            if setting != "baseline":
                assert np.count_nonzero(wet[t < 0]) == 0
            cases.append({"setting": setting, "hop_offset": offset,
                          "onset_ms": float(t[np.flatnonzero(np.abs(wet) >= peak * 0.001)[0]]),
                          "peak_ms": float(t[np.argmax(np.abs(wet))]), "peak": float(peak)})
            if offset == 0:
                if base_peak is None:
                    base_peak = peak
                # Raw absolute samples make the zero pre-onset region explicit;
                # all traces share one scale, so the emphasis remains visible.
                axes[0].plot(t, np.abs(wet) / base_peak, color=color, linewidth=1.0, alpha=0.85, label=label)
                rms = np.sqrt(np.convolve(wet * wet, np.ones(48) / 48, mode="same"))
                axes[1].plot(t, 20 * np.log10(np.maximum(rms / base_peak, 1e-6)), color=color, linewidth=1.5)
        metrics.extend(cases)
        print(f"{label}: onset {min(m['onset_ms'] for m in cases):.3f}..{max(m['onset_ms'] for m in cases):.3f} ms; "
              f"peak {min(m['peak_ms'] for m in cases):.3f}..{max(m['peak_ms'] for m in cases):.3f} ms")
    for ax in axes:
        ax.axvline(0, color="#444444", linestyle="--", linewidth=1.2)
        ax.grid(alpha=0.2)
        ax.spines[["top", "right"]].set_visible(False)
    axes[0].set_xlim(-15, 35)
    axes[0].set_ylabel("Absolute wet / baseline peak")
    axes[0].set_title("Onset detail: raw samples, common gain scale", loc="left")
    axes[0].legend(fontsize=9, loc="upper right")
    axes[1].set_xlim(-85, 220)
    axes[1].set_ylim(-65, 10)
    axes[1].set_ylabel("Wet level (dB / baseline peak)")
    axes[1].set_xlabel("Time relative to the dry transient (ms)")
    axes[1].set_title("Envelope overview (1 ms RMS)", loc="left")
    fig.suptitle("Resonant wet transient reshaping: no dry attack mixed in", fontsize=14, weight="bold")
    fig.supxlabel("48 kHz | FFT 4096 / hop 512 | 440 Hz, 8 partials | T60 2 s\n"
                   "Alignment 0 | Spectral Unison 1 | Reshape emphasis +6 dB | Plot hop offset 0", fontsize=9)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(args.output.with_suffix(".png"), dpi=150)
    fig.savefig(args.output.with_suffix(".svg"))
    args.output.with_suffix(".json").write_text(json.dumps(metrics, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
