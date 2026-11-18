# 20-band stereo headphone EQ lab — GoXLR Utility fork

This is a fork-only feature, not part of the official upstream utility.

Open the GoXLR Utility daemon's local web page and select the Headphones EQ tab,
or navigate directly to:

http://127.0.0.1:14564/headphone-eq/index.html

The editor is embedded within daemon/web-content and does not require
deploying a separate web service. Existing GoXLR functionality is unchanged.

## Implemented and tested

- Twenty 48 kHz parametric peaking EQ filters per stereo channel with linked
  and independent L/R editing, gain, frequency and Q controls.
- Presets, custom profile JSON export/import, browser-local persistence,
  simulated frequency-response curve and configurable automatic headroom.
- Output as an Equalizer APO text configuration for selected Windows
  playback endpoints (Game, Music, Chat, System, Sample).
- Output as the 988-byte GXEQ v1 DATA-ONLY research coefficient bank, built
  from signed Q3.21 peaking filters, per-band enable flags, Q1.23 preamp and
  CRC-32.
- The GXEQ output was checked byte-for-byte against the 20-band Python
  coefficient compiler in the canonical GoXLR firmware research root using a
  boosted asymmetric stereo profile. SHA256:
  9c8910c1fe50a58258acaf3c6ab5375320c09a62064bb3fbf43d114dbfb898ee.
- Read-only GetStatus integration to show whether the local GoXLR daemon
  sees a device. This function sends no routing or DSP parameter writes.

## Important: what this does *not* do

The stock GoXLR 1.5.1.901 firmware has not been proven to support a safe
headphone-only postmix DSP EQ hook. The physical headphone DAC buffer, stream
isolation, real DSP free cycles, USB-to-DSP coefficient transport and
failed-flash recovery have not been verified. Therefore the editor includes
no device DSP apply or flash controls.

Equalizer APO is NOT a workaround for hardware headphone-only isolation.
It processes selected Windows playback endpoints before GoXLR hardware routing.
Those same sources may contribute to Broadcast Mix and stream, and analog
Line-In, Console and some exclusive/ASIO/WASAPI audio are not affected.
If you need EQ only for the physical headphones, do not treat the APO export
as a verified solution.

The .gxeq output file is NOT flashable. Never submit it to any updater.

## Windows endpoint configuration

1. Install Equalizer APO separately and use its Configurator to attach it
   to the actual GoXLR Windows playback endpoints you want to modify.
2. Select endpoints in EQ Lab and export the .txt file.
3. Open that export in Equalizer APO Configuration Editor or include it in
   an APO configuration you control. This editor does not install drivers or
   rewrite any system audio configuration.
4. Verify each endpoint is processed. Check Broadcast Mix/stream too,
   because this mode does not isolate the physical headphone output.

## Reproduction on main-server

Run from the canonical research project:

    cd /root/goxlr-research-continuation-2026-11-17/goxlr-utility-fork
    node --check daemon/web-content/headphone-eq/eq-core.mjs
    node --check daemon/web-content/headphone-eq/app.mjs
    node --test headphone-eq-tests/eq-core.test.mjs
    cargo fmt --all -- --check

The compiled web assets are embedded by the existing GoXLR daemon via
its static-file handler; no daemon API contract changed.

The verified research lives in ../eq_20band_research_2026-11-17.
The separate Attune project (https://github.com/SeayInsights/attune) was
consulted for Equalizer APO endpoint conventions. No Attune source code
was copied into the feature implementation.

Official upstream contribution policy: this feature branch is for the
user's own fork, not an upstream contribution to GoXLR-on-Linux.

## Fork CI compatibility fixes

The upstream fork baseline had two independent build issues exposed by the
latest toolchains: Rust Clippy rejected a needless boolean return in the sample
bank helper, and the Windows tolk-sys 0.2.2 build script expected its target
debug/release examples directory to exist before copying screen-reader DLLs.

This branch simplifies the boolean without changing behavior and prepares
the destination directories in Windows check and release CI. It leaves
text-to-speech/tolk functionality enabled and does not disable all-features
builds. These compatibility changes are separate from the EQ model.

Note: The observed tolk-sys 0.2.2 build script removes three trailing OUT_DIR path components, so the Windows CI setup explicitly creates both debug/examples and debug/build/examples (and release counterparts). No TTS functionality is disabled.

All 20 center frequencies are editable independently per left/right channel (20–20,000 Hz), in addition to gain, Q and band bypass. Linked mode copies edits to both channels.

## Fork-only portable Windows build

The separate workflow Fork Windows EQ Lab portable build compiles the
Windows x64 GoXLR daemon and CLI with all original Cargo features on a
GitHub-hosted Windows runner and uploads a ZIP. It is NOT an installer;
the original Tauri UI wrapper and Windows audio drivers are not bundled.

Do not run the fork and installed GoXLR daemons simultaneously. Back up
profiles and stop the existing daemon before testing the portable executable,
then revert by stopping this daemon and restarting the installed one.
Avoid testing unless the build workflow reports success and the ZIP contains
both daemon and CLI executables. No firmware flashing is involved.

The Windows full-Clippy CI also revealed a pre-existing needless-bool expression in locate_goxlr_driver(); this fork simplifies it without changing the registry/file detection logic.

The fork portable Windows ZIP includes any native DLLs that the upstream all-features build emits next to the daemon, such as the screen-reader integration runtime. It remains a manual portable test package, not a signed production installer.

## Single-file browser preview

An additional generated standalone/GoXLR-EQ-Lab-OFFLINE-PREVIEW.html is fully
self-contained (inline CSS and JavaScript) and can be opened directly in a
modern browser. It supports EQ curve editing, local presets and
profile/APO/research-data exports. Browser localStorage may not persist on
some file:// origins, and the read-only device-status button cannot be relied
on outside the installed local daemon. This is NOT hardware EQ or an
installer, and does not change the actual GoXLR sound.
Rebuild using python3 headphone-eq-tests/build_standalone.py.

## Integrated device tab (2026-11-18)

The EQ editor is now embedded inside the existing Vue device settings tab list,
after System, rather than requiring external navigation or a separate window.
Existing tabs and keyboard shortcut indices remain unchanged. It uses the
same-origin EQ editor in an iframe; switching tabs retains browser-local
profiles. The direct editor URL still works.

The fork ships a reproducible hash-pinned patch to the original prebuilt UI
bundle. Run python3 headphone-eq-tests/integrate_tab.py to regenerate it.
The original bundle is retained unchanged. The integration test removes the
single extra render expression and compares the result against the original.
No upstream submission is made.

10 local Node tests passed (including tab render expression and original-byte
preservation); actual browser/device UI operation and the new Windows binary
are not yet validated. No new DSP protocol or audio apply function is enabled.
