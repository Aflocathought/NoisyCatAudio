"""Compare the stereo float WAV files emitted by render_m4 (no dependencies)."""
import array
import math
from pathlib import Path
import struct
import sys


def read_fixture(path):
    data = Path(path).read_bytes()
    assert data[:4] == b"RIFF" and data[8:16] == b"WAVEfmt "
    assert struct.unpack_from("<HH", data, 20) == (3, 2), "expected float stereo"
    assert struct.unpack_from("<H", data, 34)[0] == 32
    assert data[36:40] == b"data", "expected render_m4's fixed WAV header"
    assert struct.unpack_from("<I", data, 40)[0] == len(data) - 44
    samples = array.array("f")
    samples.frombytes(data[44:])
    if sys.byteorder != "little":
        samples.byteswap()
    assert all(math.isfinite(x) for x in samples)
    return data[:44], samples


if __name__ == "__main__":
    old_header, old = read_fixture(sys.argv[1])
    new_header, new = read_fixture(sys.argv[2])
    assert old_header == new_header, "sample rate, channels, or length changed"
    differences = [a - b for a, b in zip(old, new)]
    peak_error = max(map(abs, differences))
    reference_energy = math.fsum(x * x for x in old)
    error_energy = math.fsum(x * x for x in differences)
    relative_rms = math.sqrt(error_energy / max(reference_energy, 1e-30))
    print(f"samples={len(old)}, peak_error={peak_error:.9e}, relative_rms={relative_rms:.9e}")
    # Keep the bound tighter than the original synthesis approximation gate.
    # Reordering mathematically equivalent sums need not be bit-identical.
    assert peak_error < 1e-5 and relative_rms < 1e-6
