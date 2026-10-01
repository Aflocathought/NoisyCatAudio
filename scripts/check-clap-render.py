"""Exercise the built CLAP's render transition, lifecycle and audio ABI.

This bounded fake host checks the exported library rather than calling Rust DSP
directly. It is a regression check, not a substitute for host Bounce testing.
Run in a subprocess so the calling shell can enforce a timeout.
"""

import argparse
import ctypes as c
import hashlib
import json
import math
from pathlib import Path
import struct
import time


class Version(c.Structure):
    _fields_ = [(name, c.c_uint32) for name in ("major", "minor", "revision")]


class Entry(c.Structure):
    _fields_ = [("version", Version)] + [
        (name, c.c_void_p) for name in ("init", "deinit", "get_factory")
    ]


class Host(c.Structure):
    _fields_ = [("version", Version), ("data", c.c_void_p)] + [
        (name, c.c_char_p) for name in ("name", "vendor", "url", "host_version")
    ] + [(name, c.c_void_p) for name in (
        "get_extension", "request_restart", "request_process", "request_callback"
    )]


class Plugin(c.Structure):
    _fields_ = [(name, c.c_void_p) for name in (
        "descriptor", "data", "init", "destroy", "activate", "deactivate",
        "start_processing", "stop_processing", "reset", "process",
        "get_extension", "on_main_thread"
    )]


class Factory(c.Structure):
    _fields_ = [(name, c.c_void_p) for name in ("count", "descriptor", "create")]


class Render(c.Structure):
    _fields_ = [("hard_realtime", c.c_void_p), ("set", c.c_void_p)]


class State(c.Structure):
    _fields_ = [("save", c.c_void_p), ("load", c.c_void_p)]


class Stream(c.Structure):
    _fields_ = [("context", c.c_void_p), ("callback", c.c_void_p)]


class Audio(c.Structure):
    _fields_ = [
        ("data32", c.POINTER(c.POINTER(c.c_float))), ("data64", c.c_void_p),
        ("channels", c.c_uint32), ("latency", c.c_uint32), ("constant", c.c_uint64)
    ]


class Process(c.Structure):
    _fields_ = [
        ("steady_time", c.c_int64), ("frames", c.c_uint32),
        ("transport", c.c_void_p), ("inputs", c.POINTER(Audio)),
        ("outputs", c.POINTER(Audio)), ("input_count", c.c_uint32),
        ("output_count", c.c_uint32), ("in_events", c.c_void_p),
        ("out_events", c.c_void_p)
    ]


def call(address, result, arguments, *values):
    return c.CFUNCTYPE(result, *arguments)(address)(*values)


def check(path, require_no_restart, unison_mode):
    library = c.CDLL(str(path))
    entry = Entry.in_dll(library, "clap_entry")
    assert call(entry.init, c.c_bool, [c.c_char_p], str(path).encode())
    factory_ptr = call(entry.get_factory, c.c_void_p, [c.c_char_p], b"clap.plugin-factory")
    factory = c.cast(factory_ptr, c.POINTER(Factory)).contents
    counts = {"restart": 0, "callback": 0, "process": 0}

    def requested(key):
        def increment(_host):
            counts[key] += 1
        return c.CFUNCTYPE(None, c.c_void_p)(increment)

    extension = c.CFUNCTYPE(c.c_void_p, c.c_void_p, c.c_char_p)(lambda *_: None)
    callbacks = [extension, requested("restart"), requested("process"), requested("callback")]
    # Keep callbacks and host alive until destroy; never reenter the plugin from
    # a host request. CLAP requests are serviced after the current call returns.
    host = Host(Version(1, 2, 0), None, b"Render regression host", b"Project",
                b"https://example.invalid", b"1", *[c.cast(cb, c.c_void_p) for cb in callbacks])
    ptr = call(factory.create, c.c_void_p, [c.c_void_p, c.c_void_p, c.c_char_p],
               factory_ptr, c.byref(host), b"org.spectral-resonator.dev")
    assert ptr
    plugin = c.cast(ptr, c.POINTER(Plugin)).contents
    active = processing = False
    try:
        assert call(plugin.init, c.c_bool, [c.c_void_p], ptr)
        render_ptr = call(plugin.get_extension, c.c_void_p, [c.c_void_p, c.c_char_p], ptr, b"clap.render")
        render = c.cast(render_ptr, c.POINTER(Render)).contents
        assert not call(render.hard_realtime, c.c_bool, [c.c_void_p], ptr)
        state_ptr = call(plugin.get_extension, c.c_void_p, [c.c_void_p, c.c_char_p], ptr, b"clap.state")
        state = c.cast(state_ptr, c.POINTER(State)).contents
        # Load through the public CLAP state stream, including its framework
        # length prefix. Use the same stress configuration for both algorithms.
        preset = json.dumps({"version": "0.8.0", "params": {
            "unison_mode": {"string": unison_mode}, "unison_voices": {"i32": 8},
            "mod_mode": {"string": "granular"}, "harmonics": {"i32": 256},
            "root_note": {"i32": 33}, "pitch_source": {"string": "internal"},
        }, "fields": {}}).encode()
        encoded = len(preset).to_bytes(8, "little") + preset
        cursor = 0

        @c.CFUNCTYPE(c.c_int64, c.c_void_p, c.c_void_p, c.c_uint64)
        def read_state(_stream, destination, size):
            nonlocal cursor
            chunk = encoded[cursor:cursor + size]
            c.memmove(destination, chunk, len(chunk))
            cursor += len(chunk)
            return len(chunk)

        stream = Stream(None, c.cast(read_state, c.c_void_p))
        assert call(state.load, c.c_bool, [c.c_void_p, c.c_void_p], ptr, c.byref(stream))

        def snapshot():
            chunks = []

            @c.CFUNCTYPE(c.c_int64, c.c_void_p, c.c_void_p, c.c_uint64)
            def write_state(_stream, source, size):
                chunks.append(c.string_at(source, size))
                return size

            stream = Stream(None, c.cast(write_state, c.c_void_p))
            assert call(state.save, c.c_bool, [c.c_void_p, c.c_void_p], ptr, c.byref(stream))
            saved = b"".join(chunks)
            assert int.from_bytes(saved[:8], "little") == len(saved) - 8
            return json.loads(saved[8:])

        saved_before = snapshot()
        assert saved_before["params"]["unison_mode"]["string"] == unison_mode

        def set_mode(mode):
            before = counts["restart"]
            assert call(render.set, c.c_bool, [c.c_void_p, c.c_int32], ptr, mode)
            return counts["restart"] - before

        assert set_mode(0) == 0  # Inactive, already realtime.
        assert call(plugin.activate, c.c_bool, [c.c_void_p, c.c_double, c.c_uint32, c.c_uint32],
                    ptr, 48000.0, 1, 1024)
        active = True
        call(plugin.on_main_thread, None, [c.c_void_p], ptr)
        transition_restarts = []
        peak_seconds = 0.0
        references = []
        for mode in [0, 1, 0, 1]:
            transition_restarts.append(set_mode(mode))
            # Do not service restarts here: this verifies that this DSP, which
            # ignores render mode, can switch without reactivation or host waits.
            assert call(plugin.start_processing, c.c_bool, [c.c_void_p], ptr)
            processing = True
            output = []
            position = 0
            for index in range(72):
                frames = [1, 64, 256, 1024][index % 4]
                left = (c.c_float * frames)(*[
                    0.1 * math.sin((position + i) * 2.0 * math.pi * 440.0 / 48000.0)
                    if position + i < 12000 else 0.0 for i in range(frames)
                ])
                right = (c.c_float * frames)()
                pointers = (c.POINTER(c.c_float) * 2)(left, right)
                audio = Audio(pointers, None, 2, 0, 0)
                process = Process(position, frames, None, c.pointer(audio), c.pointer(audio), 1, 1, None, None)
                started = time.perf_counter()
                status = call(plugin.process, c.c_int32, [c.c_void_p, c.c_void_p], ptr, c.byref(process))
                peak_seconds = max(peak_seconds, time.perf_counter() - started)
                assert status != 0
                assert all(math.isfinite(value) for value in left)
                assert all(value == 0.0 for value in right), "Stereo crossfeed"
                output.extend(left)
                position += frames
            call(plugin.stop_processing, None, [c.c_void_p], ptr)
            processing = False
            assert snapshot() == saved_before, "Bounce lifecycle altered the preset"
            references.append(output)
        assert max(map(abs, references[0])) > 0.01
        assert all(audio == references[0] for audio in references[1:]), "Mode changed rendered audio"
        result = {"plugin": str(path), "unison_mode": unison_mode,
                  "transition_restart_requests": transition_restarts,
                  "audio_equal_across_modes": True, "samples_per_render": len(references[0]),
                  "audio_sha256": hashlib.sha256(struct.pack(
                      f"<{len(references[0])}f", *references[0])).hexdigest(),
                  "state_preserved": True,
                  "peak_abi_call_ms": peak_seconds * 1000.0}
        print(json.dumps(result, indent=2), flush=True)
        if require_no_restart:
            assert sum(transition_restarts) == 0, "Render mode must not request reactivation for this DSP"
    finally:
        if processing:
            call(plugin.stop_processing, None, [c.c_void_p], ptr)
        if active:
            call(plugin.deactivate, None, [c.c_void_p], ptr)
        call(plugin.destroy, None, [c.c_void_p], ptr)
        call(entry.deinit, None, [])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("plugin", type=Path)
    parser.add_argument("--require-no-restart", action="store_true")
    options = parser.parse_args()
    for algorithm in ("spectral", "post"):
        check(options.plugin.resolve(strict=True), options.require_no_restart, algorithm)
