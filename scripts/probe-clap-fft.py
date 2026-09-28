"""Bounded CLAP test for live FFT parameter changes and host reactivation.

The actual DLL runs in a child process. Only this test host is controlled;
there is no interaction with a real DAW, project, or installed plugin instance.
"""
import argparse
import ctypes as c
import importlib.util
import json
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("bounce_probe", Path(__file__).with_name("probe-clap-bounce.py"))
bounce = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bounce)
call = bounce.call


class Params(c.Structure):
    _fields_ = [(name, c.c_void_p) for name in ("count", "info", "value", "to_text", "from_text", "flush")]


class ParamInfo(c.Structure):
    _fields_ = [("id", c.c_uint32), ("flags", c.c_uint32), ("cookie", c.c_void_p),
                ("name", c.c_char * 256), ("module", c.c_char * 1024),
                ("minimum", c.c_double), ("maximum", c.c_double), ("default", c.c_double)]


class ParamValue(c.Structure):
    _fields_ = [("header", bounce.Header), ("id", c.c_uint32), ("cookie", c.c_void_p),
                ("note_id", c.c_int32), ("port", c.c_int16), ("channel", c.c_int16),
                ("key", c.c_int16), ("value", c.c_double)]


def run(path, output):
    probe = bounce.Probe(path, "internal", "spectral", output_dir=output, fft_size=4096)
    try:
        params = probe.extension(b"clap.params", Params)
        info = ParamInfo()
        found = False
        for index in range(call(params.count, c.c_uint32, [c.c_void_p], probe.ptr)):
            assert call(params.info, c.c_bool, [c.c_void_p, c.c_uint32, c.c_void_p], probe.ptr, index, c.byref(info))
            if info.name == b"FFT Size":
                found = True
                break
        assert found and not info.flags & (1 << 5), "FFT must be saved but non-automatable"
        assert (info.minimum, info.maximum, info.default) == (0, 3, 3)

        def flush_fft(size):
            event = ParamValue(bounce.Header(c.sizeof(ParamValue), 0, 0, 5, 0), info.id, info.cookie,
                               -1, -1, -1, -1, [1024, 2048, 3072, 4096].index(size))

            @c.CFUNCTYPE(c.c_uint32, c.c_void_p)
            def count(_):
                return 1

            @c.CFUNCTYPE(c.c_void_p, c.c_void_p, c.c_uint32)
            def get(_, index):
                return c.addressof(event) if index == 0 else None

            events = bounce.Events(None, c.cast(count, c.c_void_p), c.cast(get, c.c_void_p))
            probe.run_audio(lambda: call(params.flush, None, [c.c_void_p, c.c_void_p, c.c_void_p],
                                         probe.ptr, c.byref(events), None))

        probe.activate()
        assert not probe.restart_pending.is_set(), "Initial activation requested an unnecessary restart"
        probe.start()
        for selected in [2048, 3072, 1024, 4096]:
            previous = probe.fft_size
            requests = probe.requests["restart"]
            flush_fft(selected)
            assert probe.snapshot()["params"]["fft_size"]["string"] == str(selected)
            probe.run_audio(lambda: probe.render_audio(0.06))
            assert probe.requests["restart"] == requests + 1
            assert call(probe.latency.get, c.c_uint32, [c.c_void_p], probe.ptr) == previous
            # Continue while the host delays honoring the restart: no loop,
            # premature latency declaration or audio-thread engine rebuild.
            probe.run_audio(lambda: probe.render_audio(0.04))
            assert probe.requests["restart"] == requests + 1
            probe.stop()
            probe.deactivate()
            probe.restart_pending.clear()
            probe.fft_size = selected
            probe.activate()
            assert not probe.restart_pending.is_set(), "Reactivation created a restart loop"
            saved = probe.snapshot()
            assert saved["params"]["fft_size"]["string"] == str(selected)
            probe.start()
            rendered = probe.run_audio(lambda: probe.render_audio(0.12))
            assert rendered["peak"] > 0 and not probe.restart_pending.is_set()
            # Restore each selection into the same instance while inactive,
            # as when reopening an existing project at another sample rate.
            probe.stop()
            probe.deactivate()
            probe.load(saved)
            probe.rate = 96_000 if probe.rate == 48_000 else 48_000
            probe.activate()
            assert probe.snapshot() == saved
            assert not probe.restart_pending.is_set()
            probe.start()
            bounce.log("fft-switch.pass", previous=previous, selected=selected, rate=probe.rate,
                       latency=call(probe.latency.get, c.c_uint32, [c.c_void_p], probe.ptr))

        # Loading a preset without this new parameter restores the legacy 4096
        # choice even after this instance has previously used another window.
        probe.stop()
        probe.deactivate()
        saved = probe.snapshot()
        saved["params"]["fft_size"] = {"string": "1024"}
        probe.load(saved)
        del saved["params"]["fft_size"]
        probe.load(saved)
        assert probe.snapshot()["params"]["fft_size"]["string"] == "4096"
        assert not probe.callback_errors, probe.callback_errors
        bounce.log("fft-probe.pass", switches=4, requests=probe.requests)
    finally:
        probe.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("plugin", type=Path)
    parser.add_argument("--output", type=Path, default=Path("target/fft-settings-probe"))
    parser.add_argument("--child", action="store_true")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    if args.child:
        run(args.plugin.resolve(strict=True), args.output)
        return
    log = args.output / "switches.jsonl"
    with log.open("w", encoding="utf-8") as stream:
        result = subprocess.run([sys.executable, __file__, str(args.plugin.resolve(strict=True)),
                                 "--output", str(args.output), "--child"],
                                stdout=stream, stderr=subprocess.STDOUT, timeout=120)
    print(json.dumps({"exit_code": result.returncode, "log": str(log)}))
    raise SystemExit(result.returncode)


if __name__ == "__main__":
    main()
