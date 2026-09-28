# Why this copy

The crates.io `windows_x86_64_msvc` 0.52.6, unchanged except for
`lib/windows.0.52.0.lib`: its 1028 `icu.dll` import entries are removed
(`llvm-ar d windows.0.52.0.lib icu.dll`, repeated until none are left).

Skia (through Vizia) links its own, bundled ICU. winit pulls in windows-sys
0.52's `Win32_Globalization`, whose import library also declares ICU's
functions from Windows' `icu.dll`. That library comes first on the link
line, so the linker resolved some of Skia's ICU calls to Windows' ICU - a
different version, mixed with Skia's own - or refused to link at all. Shor
never calls Windows' ICU, so its entries can go.

Used only when building for Windows (MSVC); see `[patch.crates-io]` in the
workspace `Cargo.toml`.
