# Overhaul log

Crates processed by the `glide-overhaul` skill (`.claude/skills/glide-overhaul`).

| Date | Crate | Commit | Lines (+/−, net) | Left for later |
|---|---|---|---|---|
| 2026-10-27 | glide-portal | afaee1e | +31 / −16, +15 | — |
| 2026-10-27 | glidectl | c947850 | +9 / −27, −18 | — |
| 2026-10-27 | glide-ui | 5115d84 | +28 / −26, +2 | `Reveal::open` / `close` restart from `self.ease` (linear unless `with_ease`), while topbar, dock, launcher and dock_preview draw through `progress(duration, ease_out / ease_io)`: a popup reversed mid-flight jumps. Fix in the compositor callers. The value / jump / retarget workaround in `dock.rs` badge tweens is now redundant |
| 2026-10-27 | glide-ipc | 529772c | +0 / −10, −10 | — |
| 2026-10-27 | glide-media | 86844db | +52 / −80, −28 | — |
| 2026-10-27 | glide-notify | 8eeeafe | +82 / −236, −154 | — |
| 2026-10-27 | glide-iced | affac8d | +166 / −119, +47 | Button shadow blend and fixed-width label wrapping not seen on screen |
