# Local redraw pacing patch (0.10.0)

Source: crates.io egui-baseview 0.7.2; original metadata, license and provenance
are retained. Existing users default to the upstream uncapped policy.

`Frame::set_max_fps` sets an application redraw limit. `on_frame` checks its
deadline before consuming repaint requests, including pointer and automation
requests. Fractional absolute deadlines avoid adding render time to every
period; missed deadlines are dropped instead of producing catch-up bursts.
The setting can change while the window is open.

`GraphicsConfig::with_unthrottled_presentation` selects wgpu AutoNoVsync so
the explicit application cap can pace rendering. Actual presentation remains
subject to the GPU, display and platform fallback behavior. The plugin's debug
FPS measures actual UI frame intervals, not monitor scanout or the configured
limit. The plugin interpolates scrolling independently of its 30 Hz analysis.

The wgpu surface is no longer reconfigured every frame when MSAA is disabled.
In that case a missing MSAA texture is expected, so only a size change needs
surface reconfiguration. The previous condition forced a swapchain rebuild
and GPU wait on every draw; a real Windows CLAP window benchmark exposed this
while CPU analysis/UI construction stayed below one millisecond per frame.

## Explicit key exclusions (0.11.1)

`KeyCapture` exclusions are applied before queueing egui input, preventing a
host shortcut from also activating a focused widget. The new baseview key
preflight queries the same capture policy and egui focus state. Spectral
Resonator excludes Space outside numeric text editing and restores normal
text capture during editing; the settings follow each UI frame and reopen.
