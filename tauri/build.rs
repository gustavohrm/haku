fn main() {
    // Tauri links against ComCtl32 v6, which the loader only resolves when the
    // binary carries a manifest declaring the dependency. `tauri_build` embeds
    // one in the application, but not in test binaries, so those fail to start
    // with STATUS_ENTRYPOINT_NOT_FOUND before a single test runs. Declaring it
    // for test targets gives them the same manifest.
    #[cfg(all(windows, target_env = "msvc"))]
    println!(
        "cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' \
         name='Microsoft.Windows.Common-Controls' version='6.0.0.0' \
         processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
    );

    tauri_build::build();
}
