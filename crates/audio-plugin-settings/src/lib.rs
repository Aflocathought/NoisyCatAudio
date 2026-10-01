// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 Noisy Cat Audio contributors
// Distributed without warranty; see the root LICENSE and COPYRIGHT.md.

//! Shared, per-user preferences for a family of audio plugins.
//! Never call this crate from an audio callback. The client performs disk I/O
//! on a joined background worker; the store coordinates independent processes.
#![forbid(unsafe_code)]

mod client;
mod store;
pub use client::{Preferences, Snapshot};
pub use serde_json::{Value, json};
pub use store::{Document, Edit, Error, Store, settings_path};

#[cfg(test)]
mod tests;
