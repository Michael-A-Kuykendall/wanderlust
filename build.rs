//! # Build Script
//!
//! This script runs during the build process (before compilation).
//! Its primary job currently is to embed the Windows Application Manifest (`app.manifest`)
//! into the final executable.
//!
//! The manifest controls:
//! - DPI Awareness (High DPI support).
//! - User Account Control (UAC) behavior (requestedExecutionLevel).
//! - Windows Version Compatibility (identifying as Win10/11 compatible).

fn main() {
    // Embeds the 'app.manifest' file as a Windows resource.
    // We ignore the result because if it fails, the app still builds, just without the manifest.
    let _ = embed_resource::compile("app.manifest", embed_resource::NONE);
}
