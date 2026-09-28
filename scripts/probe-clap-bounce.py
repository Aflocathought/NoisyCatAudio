"""Bounded two-thread CLAP host: transport, snapshots, stop/restart and tail drain.

Only the child process loads the plugin. The parent enforces a hard timeout and
kills only its own test child on a hang, retaining the last completed ABI phase.
No real DAW project or process is modified. Outputs are diagnostic JSONL.
"""
import argparse
from array import array
from concurrent.futures import ThreadPoolExecutor
import ctypes as c
import importlib.util
import hashlib
import json
import math
from pathlib import Path
import subprocess
import sys
import threading
import time
import wave

# Loading the ABI helper should not leave generated files beside source code.
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("clap_abi", Path(__file__).with_name("check-clap-render.py"))
abi = importlib.util.module_from_spec(spec)
spec.loader.exec_module(abi)
call = abi.call


class OneCallback(c.Structure):
    _fields_ = [("get", c.c_void_p)]


class ThreadCheck(c.Structure):
    _fields_ = [("main", c.c_void_p), ("audio", c.c_void_p)]


class HostParams(c.Structure):
    _fields_ = [(name, c.c_void_p) for name in ("rescan", "clear", "request_flush")]


class Header(c.Structure):
    _fields_ = [("size", c.c_uint32), ("time", c.c_uint32), ("space", c.c_uint16),
                ("type", c.c_uint16), ("flags", c.c_uint32)]


class Transport(c.Structure):
    _fields_ = [("header", Header), ("flags", c.c_uint32),
                ("beats", c.c_int64), ("seconds", c.c_int64),
                ("tempo", c.c_double), ("tempo_inc", c.c_double),
                ("loop_beats_start", c.c_int64), ("loop_beats_end", c.c_int64),
                ("loop_seconds_start", c.c_int64), ("loop_seconds_end", c.c_int64),
                ("bar_start", c.c_int64), ("bar", c.c_int32),
                ("numerator", c.c_uint16), ("denominator", c.c_uint16)]


class Note(c.Structure):
    _fields_ = [("header", Header), ("id", c.c_int32), ("port", c.c_int16),
                ("channel", c.c_int16), ("key", c.c_int16), ("velocity", c.c_double)]


class Events(c.Structure):
    _fields_ = [("ctx", c.c_void_p), ("size", c.c_void_p), ("get", c.c_void_p)]


def log(phase, **details):
    print(json.dumps({"phase": phase, **details}), flush=True)


class Probe:
    def __init__(self, path, source, mode, require_tail_drain=False, output_dir=None, host_extensions=None,
                 attack_ms=None, decay_seconds=0.05, decay_model="damping", wet_alignment=None,
                 reshape=False, fft_size=None):
        self.library = c.CDLL(str(path))
        self.entry = abi.Entry.in_dll(self.library, "clap_entry")
        assert call(self.entry.init, c.c_bool, [c.c_char_p], str(path).encode())
        factory_ptr = call(self.entry.get_factory, c.c_void_p, [c.c_char_p], b"clap.plugin-factory")
        factory = c.cast(factory_ptr, c.POINTER(abi.Factory)).contents
        self.main_id = threading.get_ident()
        self.audio_id = None
        self.callbacks = []
        self.callback_pending = threading.Event()
        self.restart_pending = threading.Event()
        self.requests = {"restart": 0, "callback": 0, "latency": 0, "rescan": 0, "process": 0, "tail": 0}
        self.callback_errors = []
        self.notified_tail_samples = 0
        self.require_tail_drain = require_tail_drain
        self.output_dir = output_dir
        self.fft_size = fft_size
        self.extensions = {}

        def callback(result, args, fn):
            function = c.CFUNCTYPE(result, *args)(fn)
            self.callbacks.append(function)
            return c.cast(function, c.c_void_p)

        def request(key, event=None):
            def received(*_):
                self.requests[key] += 1
                if event:
                    event.set()
            return received

        self.extensions[b"clap.thread-check"] = ThreadCheck(
            callback(c.c_bool, [c.c_void_p], lambda _: threading.get_ident() == self.main_id),
            callback(c.c_bool, [c.c_void_p], lambda _: threading.get_ident() == self.audio_id))
        self.extensions[b"clap.latency"] = OneCallback(callback(None, [c.c_void_p], request("latency")))

        def tail_changed(_):
            self.requests["tail"] += 1
            if threading.get_ident() != self.audio_id:
                self.callback_errors.append("tail.changed was not on the audio thread")
            # This reentrant read is explicitly allowed by the tail extension.
            self.notified_tail_samples = call(self.tail.get, c.c_uint32, [c.c_void_p], self.ptr)

        self.extensions[b"clap.tail"] = OneCallback(callback(None, [c.c_void_p], tail_changed))
        self.extensions[b"clap.params"] = HostParams(
            callback(None, [c.c_void_p, c.c_uint32], request("rescan")),
            callback(None, [c.c_void_p, c.c_uint32, c.c_uint32], lambda *_: None),
            callback(None, [c.c_void_p], lambda _: None))
        self.extensions.update(host_extensions or {})
        self.host = abi.Host(abi.Version(1, 2, 0), None, b"Threaded Bounce probe", b"Project",
            b"https://example.invalid", b"1",
            callback(c.c_void_p, [c.c_void_p, c.c_char_p],
                     lambda _, name: c.addressof(self.extensions[name]) if name in self.extensions else None),
            callback(None, [c.c_void_p], request("restart", self.restart_pending)),
            callback(None, [c.c_void_p], request("process")),
            callback(None, [c.c_void_p], request("callback", self.callback_pending)))
        self.ptr = call(factory.create, c.c_void_p, [c.c_void_p, c.c_void_p, c.c_char_p],
                        factory_ptr, c.byref(self.host), b"org.spectral-resonator.dev")
        assert self.ptr
        self.plugin = c.cast(self.ptr, c.POINTER(abi.Plugin)).contents
        assert call(self.plugin.init, c.c_bool, [c.c_void_p], self.ptr)
        self.state = self.extension(b"clap.state", abi.State)
        self.render = self.extension(b"clap.render", abi.Render)
        self.tail = self.extension(b"clap.tail", OneCallback)
        self.latency = self.extension(b"clap.latency", OneCallback)
        self.active = False
        self.processing = False
        self.source = source
        self.mode = mode
        self.rate = 48000
        self.block = 256
        self.steady_time = 0
        self.worker = ThreadPoolExecutor(max_workers=1, thread_name_prefix="CLAP audio")
        self.worker.submit(self.set_audio_id).result()
        self.load({"version": "0.8.1", "params": {
            "pitch_source": {"string": source}, "unison_mode": {"string": mode},
            "harmonics": {"i32": 256}, "unison_voices": {"i32": 8},
            "mod_mode": {"string": "granular"}, "root_note": {"i32": 33},
            "decay_t60": {"f32": decay_seconds}, "decay_mode": {"string": decay_model},
            **({"fft_size": {"string": str(fft_size)}} if fft_size is not None else {}),
            **({"attack_mode": {"string": "reshape" if reshape else "independent"}, "attack_ms": {"f32": attack_ms}}
               if attack_ms is not None else {}),
            **({"align_wet": {"f32": wet_alignment}} if wet_alignment is not None else {}),
            **{f"decay_point_seconds_{i}": {"f32": decay_seconds} for i in range(1, 7)}
        }, "fields": {}})
        # Check actual deserialization, not only the test's intended settings.
        if fft_size is not None:
            assert self.snapshot()["params"]["fft_size"]["string"] == str(fft_size)
        if wet_alignment is not None:
            assert abs(self.snapshot()["params"]["align_wet"]["f32"] - wet_alignment) < 1e-6
        if attack_ms is not None:
            saved_params = self.snapshot()["params"]
            assert saved_params["attack_mode"]["string"] == ("reshape" if reshape else "independent")
            assert abs(saved_params["attack_ms"]["f32"] - attack_ms) < 1e-4
            assert abs(saved_params["decay_t60"]["f32"] - decay_seconds) < 1e-5

    def set_audio_id(self):
        self.audio_id = threading.get_ident()

    def extension(self, name, shape):
        ptr = call(self.plugin.get_extension, c.c_void_p, [c.c_void_p, c.c_char_p], self.ptr, name)
        assert ptr, name
        return c.cast(ptr, c.POINTER(shape)).contents

    def pump(self):
        assert threading.get_ident() == self.main_id
        if self.callback_pending.is_set():
            self.callback_pending.clear()
            call(self.plugin.on_main_thread, None, [c.c_void_p], self.ptr)

    def run_audio(self, function, snapshots=False):
        future = self.worker.submit(function)
        # Main-thread reads are intentionally concurrent with audio. Lifecycle
        # mutations remain ordered: no deactivate/reset races with process().
        while not future.done():
            self.pump()
            if snapshots:
                self.snapshot()
                call(self.latency.get, c.c_uint32, [c.c_void_p], self.ptr)
                call(self.tail.get, c.c_uint32, [c.c_void_p], self.ptr)
            time.sleep(0.001)
        self.pump()
        return future.result()

    def load(self, state):
        encoded = json.dumps(state).encode()
        blob = len(encoded).to_bytes(8, "little") + encoded
        cursor = 0

        @c.CFUNCTYPE(c.c_int64, c.c_void_p, c.c_void_p, c.c_uint64)
        def read(_stream, destination, size):
            nonlocal cursor
            chunk = blob[cursor:cursor + min(size, 127)]
            c.memmove(destination, chunk, len(chunk))
            cursor += len(chunk)
            return len(chunk)

        stream = abi.Stream(None, c.cast(read, c.c_void_p))
        assert call(self.state.load, c.c_bool, [c.c_void_p, c.c_void_p], self.ptr, c.byref(stream))

    def snapshot(self):
        chunks = []

        @c.CFUNCTYPE(c.c_int64, c.c_void_p, c.c_void_p, c.c_uint64)
        def write(_stream, source, size):
            size = min(size, 127)
            chunks.append(c.string_at(source, size))
            return size

        stream = abi.Stream(None, c.cast(write, c.c_void_p))
        assert call(self.state.save, c.c_bool, [c.c_void_p, c.c_void_p], self.ptr, c.byref(stream))
        blob = b"".join(chunks)
        assert int.from_bytes(blob[:8], "little") == len(blob) - 8
        return json.loads(blob[8:])

    def activate(self):
        log("activate.begin", rate=self.rate, block=self.block)
        assert call(self.plugin.activate, c.c_bool,
                    [c.c_void_p, c.c_double, c.c_uint32, c.c_uint32],
                    self.ptr, self.rate, 1, self.block)
        self.active = True
        self.pump()
        if self.fft_size is not None:
            assert call(self.latency.get, c.c_uint32, [c.c_void_p], self.ptr) == self.fft_size
        log("activate.end", latency=call(self.latency.get, c.c_uint32, [c.c_void_p], self.ptr),
            restart=self.restart_pending.is_set())

    def start(self):
        assert self.run_audio(lambda: call(self.plugin.start_processing, c.c_bool, [c.c_void_p], self.ptr))
        self.processing = True

    def stop(self):
        log("stop.begin")
        self.run_audio(lambda: call(self.plugin.stop_processing, None, [c.c_void_p], self.ptr))
        self.processing = False
        log("stop.end")

    def deactivate(self):
        call(self.plugin.deactivate, None, [c.c_void_p], self.ptr)
        self.active = False

    def render_audio(self, seconds, excitation=True, drain=False, capture_path=None):
        left = (c.c_float * self.block)()
        right = (c.c_float * self.block)()
        pointers = (c.POINTER(c.c_float) * 2)(left, right)
        audio = abi.Audio(pointers, None, 2, 0, 0)
        transport = Transport()
        transport.header = Header(c.sizeof(Transport), 0, 0, 9, 0)
        transport.tempo = 120.0
        transport.numerator = transport.denominator = 4
        events = []

        @c.CFUNCTYPE(c.c_uint32, c.c_void_p)
        def count(_):
            return len(events)

        @c.CFUNCTYPE(c.c_void_p, c.c_void_p, c.c_uint32)
        def event_get(_, index):
            return c.addressof(events[index]) if index < len(events) else None

        event_list = Events(None, c.cast(count, c.c_void_p), c.cast(event_get, c.c_void_p))
        process = abi.Process(0, self.block, c.addressof(transport), c.pointer(audio),
                              c.pointer(audio), 1, 1, c.addressof(event_list), None)
        statuses = {}
        max_ms = peak = last_peak = 0.0
        first_quiet_status = None
        recorded = array("f") if capture_path else None
        frames_total = int(seconds * self.rate)
        silent_input_samples = 0
        for position in range(0, frames_total, self.block):
            frames = min(self.block, frames_total - position)
            process.frames = frames
            process.steady_time = self.steady_time
            self.steady_time += frames
            # Include playing/recording flags, then stop the transport. Feed a
            # real transport event at a nonzero offset to exercise block splits.
            transport.flags = 15 | (48 if excitation and position < self.rate // 2 else 0)
            transport.seconds = int(position / self.rate * (1 << 31))
            transport.beats = transport.seconds * 2
            events.clear()
            if position == 0 and self.source == "midi":
                for note_id in range(16):
                    events.append(Note(Header(c.sizeof(Note), 0, 0, 0 if excitation else 1, 0),
                                       note_id, 0, 0, 33 + note_id % 8, 1.0 if excitation else 0.0))
            if position == self.block * 2 and frames > 64:
                change = Transport.from_buffer_copy(transport)
                change.header.time = 64
                events.append(change)
            for i in range(frames):
                left[i] = (0.1 * math.sin((position + i) * 0.0576)
                           if excitation and position + i < self.rate // 2 else 0.0)
                right[i] = 0.0
            if any(left[i] != 0.0 for i in range(frames)) or events:
                silent_input_samples = 0
            else:
                silent_input_samples += frames
            started = time.perf_counter()
            status = call(self.plugin.process, c.c_int32, [c.c_void_p, c.c_void_p], self.ptr, c.byref(process))
            max_ms = max(max_ms, (time.perf_counter() - started) * 1000)
            assert status != 0
            statuses[status] = statuses.get(status, 0) + 1
            last_peak = max(abs(left[i]) for i in range(frames))
            assert math.isfinite(last_peak)
            assert all(right[i] == 0.0 for i in range(frames)), "Stereo crossfeed"
            peak = max(peak, last_peak)
            if recorded is not None:
                for i in range(frames):
                    recorded.extend((left[i], right[i]))
            # Model CLAP scheduling, not an arbitrary wall-clock timeout. An
            # unconditional CONTINUE keeps the graph awake even for exact zeros.
            # TAIL permits suspension after silent input spans tail.get().
            can_suspend = status == 4 or (status == 2 and last_peak < 1e-9) or (
                status == 3 and silent_input_samples >= self.notified_tail_samples)
            if first_quiet_status is None and can_suspend:
                first_quiet_status = (position + frames) / self.rate
            if drain and first_quiet_status is not None:
                break
        result = {"statuses": statuses, "peak": peak, "last_peak": last_peak,
                  "first_quiet_status_seconds": first_quiet_status, "max_call_ms": max_ms}
        if recorded is not None:
            result["audio_sha256"] = hashlib.sha256(recorded.tobytes()).hexdigest()
            result["frames_recorded"] = len(recorded) // 2
            # Save a listenable stereo fixture; hashes above refer to original
            # float samples, so PCM conversion cannot hide audio differences.
            pcm = array("h", (round(max(-1.0, min(1.0, value)) * 32767) for value in recorded))
            capture_path.parent.mkdir(parents=True, exist_ok=True)
            with wave.open(str(capture_path), "wb") as output:
                output.setnchannels(2)
                output.setsampwidth(2)
                output.setframerate(self.rate)
                output.writeframes(pcm.tobytes())
            result["wav"] = str(capture_path)
        return result

    def run(self):
        log("case.begin", source=self.source, mode=self.mode)
        self.activate()
        # Service startup latency restarts just as a host would, after activate.
        if self.restart_pending.is_set():
            self.restart_pending.clear()
            self.deactivate()
            self.activate()
        saved = self.snapshot()
        drains = []
        for cycle in range(3):
            self.start()
            log("realtime.begin", cycle=cycle)
            rendered = self.run_audio(lambda: self.render_audio(1.0), snapshots=True)
            log("realtime.end", **rendered)
            log("tail-drain.begin")
            drained = self.run_audio(lambda: self.render_audio(3.0, excitation=False, drain=True), snapshots=True)
            drains.append(drained["first_quiet_status_seconds"])
            log("tail-drain.end", reported_tail_samples=call(self.tail.get, c.c_uint32, [c.c_void_p], self.ptr), **drained)
            # Even if a plugin keeps asking to run, stop it explicitly and see
            # whether its audio thread acknowledges. Both observations matter.
            self.stop()
            assert self.snapshot() == saved
            self.load(saved)
            log("offline-switch.begin")
            assert call(self.render.set, c.c_bool, [c.c_void_p, c.c_int32], self.ptr, 1)
            self.deactivate()
            self.rate, self.block = [(48000, 1024), (44100, 512), (96000, 2048)][cycle]
            self.activate()
            self.restart_pending.clear()
            self.start()
            log("offline.begin")
            capture = self.output_dir / f"{self.source}-{self.mode}-{cycle}-{self.rate}.wav"
            rendered = self.run_audio(lambda: self.render_audio(1.0, capture_path=capture), snapshots=True)
            log("offline.end", **rendered)
            self.stop()
            self.run_audio(lambda: call(self.plugin.reset, None, [c.c_void_p], self.ptr))
            self.deactivate()
            assert call(self.render.set, c.c_bool, [c.c_void_p, c.c_int32], self.ptr, 0)
            self.rate, self.block = 48000, 256
            self.activate()
        log("case.end", source=self.source, mode=self.mode, requests=self.requests)
        assert not self.callback_errors, self.callback_errors
        if self.require_tail_drain:
            assert self.requests["tail"] > 0, "Host tail cache never notified"
            assert all(value is not None for value in drains), "Host cannot finish silent tail drain"

    def close(self):
        if self.processing:
            self.stop()
        if self.active:
            self.deactivate()
        self.worker.shutdown(wait=True)
        call(self.plugin.destroy, None, [c.c_void_p], self.ptr)
        call(self.entry.deinit, None, [])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("plugin", type=Path)
    parser.add_argument("--output", type=Path, default=Path("target/bounce-probe"))
    parser.add_argument("--child", action="store_true")
    parser.add_argument("--require-tail-drain", action="store_true")
    parser.add_argument("--source", choices=["internal", "midi"], default="internal")
    parser.add_argument("--mode", choices=["spectral", "post"], default="spectral")
    parser.add_argument("--attack-ms", type=float, help="Enable Independent attack (0..2000 ms)")
    parser.add_argument("--decay-seconds", type=float, default=0.05)
    parser.add_argument("--decay-model", choices=["damping", "curve"], default="damping")
    parser.add_argument("--wet-alignment", type=float, help="Wet delay in windows (0..1)")
    parser.add_argument("--reshape", action="store_true", help="Use wet transient shaping with --attack-ms")
    parser.add_argument("--fft-size", type=int, choices=[1024, 2048, 3072, 4096])
    args = parser.parse_args()
    if args.attack_ms is not None and not 0 <= args.attack_ms <= 2000:
        parser.error("--attack-ms must be 0..2000")
    if args.reshape and args.attack_ms is None:
        parser.error("--reshape requires --attack-ms")
    if not 0.005 <= args.decay_seconds <= 12:
        parser.error("--decay-seconds must be 0.005..12")
    if args.wet_alignment is not None and not 0 <= args.wet_alignment <= 1:
        parser.error("--wet-alignment must be 0..1")
    if args.child:
        probe = Probe(args.plugin.resolve(strict=True), args.source, args.mode,
                      args.require_tail_drain, args.output, attack_ms=args.attack_ms,
                      decay_seconds=args.decay_seconds, decay_model=args.decay_model,
                      wet_alignment=args.wet_alignment, reshape=args.reshape, fft_size=args.fft_size)
        try:
            probe.run()
        finally:
            probe.close()
        return
    args.output.mkdir(parents=True, exist_ok=True)
    results = []
    for source in ("internal", "midi"):
        for mode in ("spectral", "post"):
            path = args.output / f"{source}-{mode}.jsonl"
            command = [sys.executable, __file__, str(args.plugin.resolve(strict=True)),
                       "--child", "--source", source, "--mode", mode,
                       "--output", str(args.output), "--decay-seconds", str(args.decay_seconds),
                       "--decay-model", args.decay_model]
            if args.fft_size is not None:
                command.extend(["--fft-size", str(args.fft_size)])
            if args.attack_ms is not None:
                command.extend(["--attack-ms", str(args.attack_ms)])
            if args.wet_alignment is not None:
                command.extend(["--wet-alignment", str(args.wet_alignment)])
            if args.reshape:
                command.append("--reshape")
            if args.require_tail_drain:
                command.append("--require-tail-drain")
            with path.open("w", encoding="utf-8") as output:
                try:
                    result = subprocess.run(command, stdout=output, stderr=subprocess.STDOUT, timeout=45)
                    status = "passed" if result.returncode == 0 else f"failed: {result.returncode}"
                except subprocess.TimeoutExpired:
                    status = "HANG: 45 second watchdog killed test child"
            results.append({"source": source, "mode": mode, "status": status, "log": str(path)})
            log("child-result", **results[-1])
    (args.output / "results.json").write_text(json.dumps(results, indent=2), encoding="utf-8")
    if any(result["status"] != "passed" for result in results):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
