# Local Windows frame-clock patch (0.10.0)

Source: crates.io baseview 0.3.4; original metadata, license and
`.cargo_vcs_info.json` are retained. Platforms other than Windows are unchanged.

The upstream Win32 redraw poll uses SetTimer with a 15 ms interval. It quantizes
egui repaint deadlines and cannot support a 90/120 FPS editor. A per-window
high-resolution waitable timer now posts a custom message, coalesced to at most
one outstanding wakeup. UI and GPU operations stay on the host window thread.

The helper waits without spinning, sleeps longer while hidden, and is joined
before the HWND can be destroyed/recycled. No global `timeBeginPeriod` is used.
If timer creation, helper creation or timer operation fails, the original
15 ms timer remains the fallback. Windows Security bindings are enabled for
the timer creation signature. The audio thread does not use this helper.

`scripts/probe-clap-editor.py` exercises real hosted windows, resizing and
destruction with an independent audio worker. Renderer pacing is implemented
separately in the egui-baseview patch.

## Space forwarding (0.11.1)

The Win32 WH_GETMESSAGE hook previously removed every keyboard message for
the child even when the UI wanted to return the key to its host. Space now
uses a side-effect-free `WindowHandler::captures_key` preflight before any
DOWN/CHAR/UP decoding. This matters because a DOWN may be deferred until CHAR;
checking only the final decoded event is too late for a host accelerator.

When Space belongs to the host, the original message and flags remain in the
host queue. No duplicate messages or simulated host key presses are posted.
If the host subsequently dispatches that message to the child, the window
does not also feed it to egui. Other native keys retain their existing path.
The hook releases its window-registry read guard before querying the handler.
