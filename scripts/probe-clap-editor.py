"""Local CLAP GUI test host: GPU window + independent audio thread, no speakers.

Creates only its own Win32 parent. Exercises real exported gui create/parent/
resize/show/hide/destroy while audio runs, then reopens it. A parent process
enforces a timeout so a broken GUI cannot block the test runner indefinitely.
"""
import argparse
import faulthandler
import ctypes as c
from ctypes import wintypes as w
import importlib.util
import json
import math
from pathlib import Path
import subprocess
import sys
import threading
import time

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("bounce", Path(__file__).with_name("probe-clap-bounce.py"))
bounce = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bounce)
abi, call = bounce.abi, bounce.call


class Gui(c.Structure):
    _fields_ = [(name, c.c_void_p) for name in ("supported", "preferred", "create", "destroy", "scale", "size", "can_resize", "hints", "adjust", "set_size", "parent", "transient", "title", "show", "hide")]


class Window(c.Structure):
    _fields_ = [("api", c.c_char_p), ("handle", c.c_void_p)]


class HostGui(c.Structure):
    _fields_ = [(name, c.c_void_p) for name in ("hints", "resize", "show", "hide", "closed")]


def run(path, seconds):
    faulthandler.dump_traceback_later(seconds + 25, repeat=False)
    headless = bounce.Probe(path, "internal", "post")
    try:
        headless_gui = headless.extension(b"clap.gui", Gui)
        assert not call(headless_gui.create, c.c_bool, [c.c_void_p, c.c_char_p, c.c_bool], headless.ptr, b"win32", False)
    finally:
        headless.close()
    bounce.log("gui.no-host-callbacks", result="graceful refusal")
    user = c.WinDLL("user32", use_last_error=True)
    user.CreateWindowExW.argtypes = [w.DWORD, w.LPCWSTR, w.LPCWSTR, w.DWORD, c.c_int, c.c_int, c.c_int, c.c_int, w.HWND, w.HMENU, w.HINSTANCE, c.c_void_p]
    user.CreateWindowExW.restype = w.HWND
    user.DefWindowProcW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
    user.DefWindowProcW.restype = w.LPARAM
    user.ShowWindow.argtypes = [w.HWND, c.c_int]
    user.DestroyWindow.argtypes = [w.HWND]
    user.SetWindowPos.argtypes = [w.HWND, w.HWND, c.c_int, c.c_int, c.c_int, c.c_int, w.UINT]
    user.PeekMessageW.argtypes = [c.POINTER(w.MSG), w.HWND, w.UINT, w.UINT, w.UINT]
    user.TranslateMessage.argtypes = [c.POINTER(w.MSG)]
    user.DispatchMessageW.argtypes = [c.POINTER(w.MSG)]
    user.DispatchMessageW.restype = w.LPARAM
    # This is an application test window, not automation of a user's DAW.
    parent = user.CreateWindowExW(0, "STATIC", "Spectral Resonator / CLAP UI test", 0x00CF0000 | 0x02000000, 100, 100, 1140, 840, None, None, None, None)
    assert parent, c.get_last_error()
    gui_callbacks = [
        c.CFUNCTYPE(None, c.c_void_p)(lambda _: None),
        c.CFUNCTYPE(c.c_bool, c.c_void_p, c.c_uint32, c.c_uint32)(lambda _, width, height: bool(user.SetWindowPos(parent, None, 0, 0, width + 16, height + 39, 0x0002 | 0x0004))),
        c.CFUNCTYPE(c.c_bool, c.c_void_p)(lambda _: True),
        c.CFUNCTYPE(c.c_bool, c.c_void_p)(lambda _: True),
        c.CFUNCTYPE(None, c.c_void_p, c.c_bool)(lambda *_: None),
    ]
    host_gui = HostGui(*[c.cast(cb, c.c_void_p) for cb in gui_callbacks])
    probe = bounce.Probe(path, "internal", "post", host_extensions={b"clap.gui": host_gui})
    saved = probe.snapshot()
    saved["params"].update({"root_note": {"i32": 57}, "mod_mode": {"string": "chorus"}, "decay_t60": {"f32": 2.0}, "unison_voices": {"i32": 4}})
    probe.load(saved)
    gui = probe.extension(b"clap.gui", Gui)
    assert call(gui.supported, c.c_bool, [c.c_void_p, c.c_char_p, c.c_bool], probe.ptr, b"win32", False)
    probe.activate()
    probe.start()
    running = threading.Event()
    running.set()

    def audio():
        left, right = (c.c_float * 256)(), (c.c_float * 256)()
        ptrs = (c.POINTER(c.c_float) * 2)(left, right)
        buf = abi.Audio(ptrs, None, 2, 0, 0)
        process = abi.Process(0, 256, None, c.pointer(buf), c.pointer(buf), 1, 1, None, None)
        blocks = 0
        peak = 0.0
        seed = 42
        start = time.monotonic()
        while running.is_set():
            for i in range(256):
                t = (blocks * 256 + i) / 48000.0
                seed = (1664525 * seed + 1013904223) & 0xffffffff
                noise = (seed / 2147483648.0 - 1.0) * 0.025
                left[i] = noise + 0.10 * math.sin(math.tau * (180 * t + 70 * t * t))
                right[i] = noise + 0.08 * math.sin(math.tau * 440 * t)
            process.steady_time = blocks * 256
            status = call(probe.plugin.process, c.c_int, [c.c_void_p, c.c_void_p], probe.ptr, c.byref(process))
            assert status != 0
            peak = max(peak, max(abs(x) for x in left), max(abs(x) for x in right))
            assert math.isfinite(peak)
            blocks += 1
            time.sleep(max(0.0, start + blocks * 256 / 48000.0 - time.monotonic()))
        return {"blocks": blocks, "peak": peak}

    future = probe.worker.submit(audio)
    opened = False
    try:
        for cycle in range(3):
            assert call(gui.create, c.c_bool, [c.c_void_p, c.c_char_p, c.c_bool], probe.ptr, b"win32", False)
            opened = True
            window = Window(b"win32", parent)
            assert call(gui.parent, c.c_bool, [c.c_void_p, c.c_void_p], probe.ptr, c.byref(window))
            width, height = c.c_uint32(), c.c_uint32()
            assert call(gui.size, c.c_bool, [c.c_void_p, c.c_void_p, c.c_void_p], probe.ptr, c.byref(width), c.byref(height))
            assert width.value > 0 and height.value > 0
            assert call(gui.can_resize, c.c_bool, [c.c_void_p], probe.ptr)
            if cycle > 0:
                width.value, height.value = (1260, 860) if cycle == 1 else (700, 500)
                assert call(gui.adjust, c.c_bool, [c.c_void_p, c.c_void_p, c.c_void_p], probe.ptr, c.byref(width), c.byref(height))
                assert call(gui.set_size, c.c_bool, [c.c_void_p, c.c_uint32, c.c_uint32], probe.ptr, width.value, height.value)
            user.SetWindowPos(parent, None, 0, 0, width.value + 16, height.value + 39, 0x0002 | 0x0004)
            user.ShowWindow(parent, 5)
            assert call(gui.show, c.c_bool, [c.c_void_p], probe.ptr)
            bounce.log("gui.open", cycle=cycle, width=width.value, height=height.value)
            deadline = time.monotonic() + (seconds if cycle == 0 else 2.0)
            message = w.MSG()
            while time.monotonic() < deadline:
                # Redraw timers can keep the native message queue nonempty.
                # Bound each pump so host callbacks and the deadline still run.
                for _ in range(64):
                    if not user.PeekMessageW(c.byref(message), None, 0, 0, 1):
                        break
                    user.TranslateMessage(c.byref(message))
                    user.DispatchMessageW(c.byref(message))
                probe.pump()
                if future.done():
                    future.result()
                    raise AssertionError("Audio stopped unexpectedly")
                time.sleep(0.005)
            assert call(gui.hide, c.c_bool, [c.c_void_p], probe.ptr)
            assert call(gui.show, c.c_bool, [c.c_void_p], probe.ptr)
            assert call(gui.hide, c.c_bool, [c.c_void_p], probe.ptr)
            call(gui.destroy, None, [c.c_void_p], probe.ptr)
            opened = False
            bounce.log("gui.closed", cycle=cycle)
        bounce.log("gui.final-state", params=probe.snapshot()["params"])
    finally:
        if opened:
            call(gui.hide, c.c_bool, [c.c_void_p], probe.ptr)
            call(gui.destroy, None, [c.c_void_p], probe.ptr)
        running.clear()
        bounce.log("audio.end", **future.result(timeout=10))
        probe.close()
        user.DestroyWindow(parent)
    bounce.log("gui.pass", cycles=3)
    faulthandler.cancel_dump_traceback_later()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("plugin", type=Path)
    parser.add_argument("--seconds", type=float, default=10)
    parser.add_argument("--child", action="store_true")
    args = parser.parse_args()
    if args.child:
        run(args.plugin.resolve(), args.seconds)
    else:
        result = subprocess.run([sys.executable, __file__, str(args.plugin.resolve()), "--seconds", str(args.seconds), "--child"], timeout=args.seconds + 40)
        raise SystemExit(result.returncode)
