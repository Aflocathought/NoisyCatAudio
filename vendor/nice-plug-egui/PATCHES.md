# Local editor lifecycle patch

Source: crates.io `nice-plug-egui` 0.5.1, ISC license. Original package and
license metadata are retained. This adapter matches nice-plug-core 0.4.2.

`src/editor.rs` forwards window-handler destruction to `NiceEguiApp::editor_closed`
(declared but unused upstream), releases the retained egui Context, and updates
`EguiEditorState::is_open()` on successful show/hide. This lets the audio side
disable visual capture while hidden, and releases UI textures/gestures on close.
These changes do not touch DSP or the CLAP render/tail patches in nice-plug.

`src/widgets/param_slider.rs` bounds the numeric entry field to the remaining
column width; the default egui text width otherwise expands compact panels.
