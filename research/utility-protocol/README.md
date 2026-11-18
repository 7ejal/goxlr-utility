# GoXLR Utility control-plane research (offline only)

Reference source: existing fork, commit 909ac5d before this addition.
The decoder consumes a single raw logical packet (16-byte header plus body),
not a PCAP file or USB transaction header. It never opens a device or socket.
Run the tests from the repository root:
    python3 -m unittest discover -s research/utility-protocol -p 'test_*.py' -v

Transport sources:
- usb/src/commands.rs defines IDs; usb/src/device/base.rs encodes the body.
- usb/src/device/tusb/device.rs sends request 2, waits for a response,
  receives request 3. Header: LE command u32, body length u16, index u16,
  eight remaining bytes (kept raw).
- types/src/lib.rs defines EffectKey and MicrophoneParamKey. The Python
  decoder extracts tables from source, including all 106 EffectKey values.
- The five stock groups 807, 80E, 812, 813 and 815 are labeled only by
  number; their meanings are not assumed. They are not added as active commands.
- Request and response must be labeled explicitly. Response fields are
  interpreted only for known matching layouts; no command status inferred.
- Historical 8/10-byte mix payloads and 22/26-byte routing layouts are
  recognized, not certified as valid for each firmware.
- Firmware update group/action is labeled, never executed.

Important application behavior:
- ipc/src/lib.rs defines GoXLRCommand; daemon/src/device.rs implements
  perform_command. Many app actions resolve to profile changes, multiple
  hardware calls, or host-only audio operations instead of one USB command.
- SetEqGain/SetEqFreq update a *microphone* profile then apply EffectKey.
  Full-size microphone EQ uses 801000; Mini uses 80B000. Never reuse
  existing microphone keys as custom headphone EQ.
- SetMonitorMix(OutputDevice) changes the profile monitor tree. It saves
  original headphone routes/assignment, mirrors selected output routes
  to headphones, and restores the previous state when returning.
  apply_routing rewrites headphone route bits for the monitored output.
  load_submix_settings sends 818000 (A/B selection) separately from
  817000 (other output assignment). This is a route mirror, not a new
  PCM processing stage and not a digital copy of the other output's PCM.
- Full-size capability thresholds: submix 1.4.2.107, Mix2 1.5.0.0,
  animations 1.3.40.0. None implies custom EQ support.
- Audio crate serves computer-side sampler playback/recording; it cannot
  automatically intercept internal monitoring or analog/console sources.
- Firmware file loader checks device header, extracts version. This is
  not a proof of actual boot safety, recovery, or native DSP integration.

Tests use synthetic payloads, including every source effect key. No capture
is claimed. Real-time transport, final headphone DAC wiring, independent
PCM ownership and safe firmware flashing remain unresolved. Existing EQ11
host laboratory controls are unchanged.
