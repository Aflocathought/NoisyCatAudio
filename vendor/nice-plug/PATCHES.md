# Local patch to nice-plug 0.4.2

Source: crates.io `nice-plug` 0.4.2, repository
<https://codeberg.org/RustAudio/nice-plug>. Original package metadata, README,
source and tests are retained. License: ISC, as declared by upstream Cargo.toml.
Upstream revision is recorded in `.cargo_vcs_info.json`.

Only two source files differ from the published crate:

- `src/wrapper/clap.rs` adds `CLAP_REACTIVATE_ON_RENDER_MODE_CHANGE`, default true.
- `src/wrapper/clap/wrapper.rs` guards the existing render-mode restart request
  with that opt-out. Both choices still update `current_process_mode`. It also
  maps `ProcessStatus::Tail` to `CLAP_PROCESS_TAIL` and notifies `clap_host_tail`
  when the reported tail changes, publishing the value before the callback.
  With the editor enabled (0.9.0), `gui.create` also returns false if the host
  supplies no `clap.gui` callbacks, instead of panicking on an Option unwrap.
  In 0.13.0, activation publishes `is_activated` only after dropping the deferred
  activation context. Latency is therefore announced while still inactive,
  preventing an extra restart on initial activation or an FFT-size change.
  Runtime latency changes made while already active retain the existing restart
  path. `scripts/probe-clap-fft.py` exercises actual parameter flushes and confirms
  exactly one restart per requested size, with none added by reactivation.

Specatral Resonator opts out because its activation/DSP never varies by process
mode. Sample-rate, buffer-size and channel-layout reactivation is unaffected.
Do not opt out for a plugin that needs reconfiguration when render mode changes.

The 0.8.1 mode-change opt-out alone did not resolve the reported host Bounce
hang. A two-thread fake host then reproduced indefinite CONTINUE responses even
after output was exactly zero, plus absent tail-change notifications. 0.8.2 fixes
those CLAP tail semantics without changing DSP or the tail-length calculation.
`KeepAlive` still maps to unconditional CONTINUE, and normal output retains
CONTINUE_IF_NOT_QUIET. Host tail callbacks run on the audio thread, after releasing
the plugin lock; they may synchronously query the newly published tail value.

Specification: <https://github.com/free-audio/clap/blob/main/include/clap/process.h>
and <https://github.com/free-audio/clap/blob/main/include/clap/ext/tail.h>.
Actual host Bounce confirmation remains necessary; the fake host does not
prove the host's internal scheduling or wait chain.

Regression: `scripts/check-clap-render.py PLUGIN --require-no-restart` exercises
the exported CLAP ABI, detects old restart requests, and compares audio across
mode changes. This patch should be reevaluated when upgrading nice-plug.

`scripts/probe-clap-bounce.py PLUGIN --require-tail-drain` tests separate main and
audio threads, transport/note events, concurrent state snapshots, finite-tail
drain, reactivation and offline WAV rendering. Its parent watchdog only kills
its own test subprocess on a timeout. The 0.8.1 artifact fails the tail contract;
0.8.2 passes with identical float-audio hashes in all 12 offline renders.
