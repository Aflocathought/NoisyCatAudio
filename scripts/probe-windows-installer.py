"""Inspect only a freshly built test installer's own Win32 wizard.

Requires Pillow for PNG capture. No desktop-wide screenshots or DAW interaction.
Run after test-windows-installer.ps1; the test product must be uninstalled.
"""
import ctypes as c
from ctypes import wintypes as w
import atexit
import json
from pathlib import Path
import subprocess
import sys
import time

from PIL import Image

exe = Path(sys.argv[1]).resolve()
test_base = Path(__file__).resolve().parents[1] / "target" / "installer-tests"
assert exe.is_relative_to(test_base), "Only isolated test installers are allowed"
run_root = test_base / exe.relative_to(test_base).parts[0]
assert len(run_root.name) == 32 and all(ch in "0123456789abcdef" for ch in run_root.name)
out = run_root / "wizard-preview"
out.mkdir(exist_ok=True)

user, kernel, gdi = c.WinDLL("user32"), c.WinDLL("kernel32"), c.WinDLL("gdi32")
callback = c.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
user.EnumWindows.argtypes = [callback, w.LPARAM]
user.EnumChildWindows.argtypes = [w.HWND, callback, w.LPARAM]
user.GetWindowThreadProcessId.argtypes = [w.HWND, c.POINTER(w.DWORD)]
user.GetWindowTextW.argtypes = [w.HWND, w.LPWSTR, c.c_int]
user.GetClassNameW.argtypes = [w.HWND, w.LPWSTR, c.c_int]
user.GetWindowRect.argtypes = [w.HWND, c.POINTER(w.RECT)]
user.SendMessageW.argtypes = [w.HWND, w.UINT, w.WPARAM, w.LPARAM]
user.IsWindowVisible.argtypes = [w.HWND]
user.GetDC.argtypes, user.GetDC.restype = [w.HWND], w.HDC
user.ReleaseDC.argtypes = [w.HWND, w.HDC]
user.PrintWindow.argtypes = [w.HWND, w.HDC, w.UINT]
gdi.CreateCompatibleDC.argtypes, gdi.CreateCompatibleDC.restype = [w.HDC], w.HDC
gdi.CreateCompatibleBitmap.argtypes, gdi.CreateCompatibleBitmap.restype = [w.HDC, c.c_int, c.c_int], w.HBITMAP
gdi.SelectObject.argtypes, gdi.SelectObject.restype = [w.HDC, w.HGDIOBJ], w.HGDIOBJ
gdi.GetDIBits.argtypes = [w.HDC, w.HBITMAP, w.UINT, w.UINT, c.c_void_p, c.c_void_p, w.UINT]
gdi.DeleteObject.argtypes = [w.HGDIOBJ]
gdi.DeleteDC.argtypes = [w.HDC]
kernel.CreateToolhelp32Snapshot.argtypes, kernel.CreateToolhelp32Snapshot.restype = [w.DWORD, w.DWORD], w.HANDLE
kernel.CloseHandle.argtypes = [w.HANDLE]


class Process(c.Structure):
    _fields_ = [("size", w.DWORD), ("usage", w.DWORD), ("pid", w.DWORD),
                ("heap", c.c_size_t), ("module", w.DWORD), ("threads", w.DWORD),
                ("parent", w.DWORD), ("priority", w.LONG), ("flags", w.DWORD),
                ("exe", w.WCHAR * 260)]


kernel.Process32FirstW.argtypes = [w.HANDLE, c.POINTER(Process)]
kernel.Process32NextW.argtypes = [w.HANDLE, c.POINTER(Process)]


def descendants(pid):
    snapshot = kernel.CreateToolhelp32Snapshot(2, 0)
    entry = Process()
    entry.size = c.sizeof(entry)
    pairs = []
    try:
        more = kernel.Process32FirstW(snapshot, c.byref(entry))
        while more:
            pairs.append((entry.pid, entry.parent))
            more = kernel.Process32NextW(snapshot, c.byref(entry))
    finally:
        kernel.CloseHandle(snapshot)
    ids = {pid}
    for _ in range(4):
        ids.update(child for child, parent in pairs if parent in ids)
    return ids


def label(hwnd, class_name=False):
    text = c.create_unicode_buffer(2048)
    (user.GetClassNameW if class_name else user.GetWindowTextW)(hwnd, text, len(text))
    return text.value


def children(hwnd):
    controls = []
    user.EnumChildWindows(hwnd, callback(lambda h, _: controls.append((h, label(h), label(h, True))) or True), 0)
    return controls


def capture(hwnd, name):
    rect = w.RECT()
    user.GetWindowRect(hwnd, c.byref(rect))
    width, height = rect.right - rect.left, rect.bottom - rect.top
    dc = user.GetDC(hwnd)
    memory = gdi.CreateCompatibleDC(dc)
    bitmap = gdi.CreateCompatibleBitmap(dc, width, height)
    previous = gdi.SelectObject(memory, bitmap)
    try:
        assert user.PrintWindow(hwnd, memory, 2)
        gdi.SelectObject(memory, previous)
        # BITMAPINFOHEADER: negative height requests a top-down BGRA image.
        import struct
        info = c.create_string_buffer(struct.pack("<IiiHHIIiiII", 40, width, -height, 1, 32, 0, 0, 0, 0, 0, 0))
        pixels = c.create_string_buffer(width * height * 4)
        assert gdi.GetDIBits(dc, bitmap, 0, height, pixels, info, 0) == height
        Image.frombytes("RGB", (width, height), pixels.raw, "raw", "BGRX").save(out / f"{name}.png")
    finally:
        gdi.DeleteObject(bitmap)
        gdi.DeleteDC(memory)
        user.ReleaseDC(hwnd, dc)
    return [{"text": text, "class": cls} for _, text, cls in children(hwnd)]


def click(hwnd, word):
    found = [h for h, text, cls in children(hwnd) if cls == "TNewButton" and word in text]
    assert len(found) == 1, (word, children(hwnd))
    user.SendMessageW(found[0], 0x00F5, 0, 0)
    time.sleep(0.25)


startup = subprocess.STARTUPINFO()
startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
startup.wShowWindow = 0
process = subprocess.Popen([str(exe), "/CURRENTUSER", "/LANG=chinesesimp", "/SUPPRESSMSGBOXES", "/NORESTART",
                            f"/CLAPDIR={run_root / '可视检查 CLAP'}", f"/LOG={out / 'setup.log'}"], startupinfo=startup)


def cleanup():
    # Only this probe's isolated installer process tree may be stopped. Keep
    # a failed visual assertion from leaving a registered test installation.
    if process.poll() is None:
        subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                       capture_output=True, timeout=10, creationflags=subprocess.CREATE_NO_WINDOW)
        process.wait(timeout=10)
    uninstall = run_root / "support" / "unins000.exe"
    if uninstall.exists():
        result = subprocess.run([str(uninstall), "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"],
                                startupinfo=startup, timeout=30)
        assert result.returncode == 0, "Test cleanup failed"


atexit.register(cleanup)
hwnd = None
for _ in range(100):
    ids, windows = descendants(process.pid), []

    def collect(h, _):
        pid = w.DWORD()
        user.GetWindowThreadProcessId(h, c.byref(pid))
        if pid.value in ids and label(h, True) == "TWizardForm":
            windows.append(h)
        return True

    user.EnumWindows(callback(collect), 0)
    if windows:
        hwnd = windows[0]
        break
    time.sleep(0.1)
assert hwnd, "Test wizard did not open"
time.sleep(0.5)
observed = {"folder": capture(hwnd, "folder")}
click(hwnd, "下一步")
observed["ready"] = capture(hwnd, "ready")
click(hwnd, "安装")
for _ in range(150):
    if any("完成" in text and cls == "TNewButton" for _, text, cls in children(hwnd)):
        break
    time.sleep(0.1)
else:
    raise RuntimeError("Installation did not reach the finish page")
observed["finished"] = capture(hwnd, "finished")
click(hwnd, "完成")
assert process.wait(timeout=15) == 0
(out / "controls.json").write_text(json.dumps(observed, ensure_ascii=False, indent=2), encoding="utf-8")
cleanup()
atexit.unregister(cleanup)
assert not any("{cm:" in item["text"] for page in observed.values() for item in page), "Unexpanded message constant"
assert any("重新扫描" in item["text"] for item in observed["finished"]), "Missing host scan guidance"
print(out)
