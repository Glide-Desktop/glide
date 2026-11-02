# Review of crates below 7,000 Rust lines

Scope fixed at the start of the review; physical `.rs` lines include tests,
comments and blank lines. Existing worktree changes predate this review.

Order: glide-login-key (82), glide-geom (123), glide-portal (438),
glide-pam (450), glidectl (484), glide-ui (520), glide-ipc (956),
glide-notify (1057), glide-gestures (1246), glide-media (1248),
glide-iced (1307), glide-app (1754), glide-config (1811),
glide-appmenu (1973), glide-tray (2491), glide-cast (2582),
glide-layout (2604), glide-netbird (2616), glide-services (2699),
glide-catalog (2870), glide-system (3245), glide-paint (3612),
glide-shot (3691), glide-shell (3809), glide-theme (4363),
glide-greeter (4661), glide-dock (4815), glide-phone (4880),
glide-overlays (5972), glide-launcher (6312), glide-terminal (6726).

## Progress

- glide-login-key: source review complete; no confirmed defect. All three
  existing unit tests passed.
- glide-geom: source review complete. Fixed saturated right/bottom boundaries
  rejecting valid containment and overlap at `i32::MAX`, using widened edge
  arithmetic. Two regression tests passed, covering coordinate limits,
  half-open boundaries, adjacent rectangles and empty dimensions.
- glide-portal: source reviewed. Fixed duplicate D-Bus request registration
  being treated as success: a duplicate now terminates its chooser, returns
  FAILED and preserves the existing interface. Added regression assertions
  to the private-bus integration test. Three option-conversion tests pass;
  integration execution is blocked by sandbox denial of Unix socket binding
  (confirmed with `dbus-daemon --session --nofork --print-address=1`).
  Follow-up: screenshot/color requests ignore their cancellation handle;
  inspect compositor IPC cancellation support before declaring this crate done.
- glide-pam: source and integration tests reviewed. File-system identity
  switching now verifies GID as well as UID and restores the previous
  identity if either switch fails. Four unit tests pass; module builds.
  Both integration tests fail with PAM status 4 at pam_authenticate;
  cause still needs investigation. Integration verification remains pending.
- glidectl: source reviewed; five tests pass. Subscription events and initial
  responses now use glide-ipc's bounded line reader; oversized or incomplete
  messages terminate the connection rather than allocating without a bound
  or parsing a truncated frame.
- glide-ui: source reviewed; 21 tests pass. No confirmed defect found.
- Clippy passes for glide-portal, glide-pam, glidectl and glide-ui with
  all targets and warnings denied. Ran `cargo fmt --all` successfully.
- glide-ipc: all source reviewed. Added a shared 16 MiB line reader that
  requires a newline, distinguishes clean EOF from truncation, and preserves
  subsequent buffered messages. Added two regression tests covering limits,
  truncation, EOF and consecutive messages. All 10 tests pass.
- glide-notify: all source reviewed. Duplicate detection now includes sender,
  urgency, actions/reply, icon, timeout, category, persistence, sound and image
  data; another client or a behavior change can no longer be rejected solely
  because the visible text matches. Regression coverage added; 32 tests pass.
- glide-gestures: all source reviewed. Velocity now sums deltas strictly
  after the oldest retained timestamp: its own delta precedes the measured
  interval and previously inflated flick speed. Regression test verifies
  exact speed before and after pruning the rolling window. All 29 tests pass.
- Latest checks: 76 unit tests pass across glide-ipc, glidectl, glide-notify
  and glide-gestures. Clippy passes for all four with all targets and warnings
  denied. Gesture motion has not been exercised on a physical touchpad.
- glide-media: all source and integration tests reviewed. Player selection
  retains the current bus on equal playback status, avoiding card/control
  target changes when ListNames order changes. Playing still outranks paused,
  and a vanished player is replaced. Cover downloads now have a bounded,
  nonblocking queue; a rejected current URL is retried on the next poll.
  Added regression tests for selection and queue backpressure. 35 unit tests
  pass. Three integration tests skip because no session bus is available;
  real MPRIS interaction remains unverified.
- glide-iced: all six source files reviewed. Menu items now use the specified
  8% hover and 13% press state layers instead of identical fill-2 backgrounds.
  Field/search focus rings now respect Paint.alpha, matching the other field
  colors during transitions. Added a light/dark state-layer regression test.
  All 10 tests pass, including headless text layout, glyphs and elevation
  renderer checks. Live visual interaction remains unverified.
- Clippy passes for glide-media and glide-iced with all targets and warnings
  denied; formatting completed.
- glide-app: all source files and the menu integration test reviewed.
  Disabled menu actions and descendants of disabled submenus cannot activate
  via D-Bus events. Extreme negative recursion depth no longer overflows.
  Added regression coverage for both. Shell look watcher now discards failed
  bus connections so retries can reconnect. All 23 unit tests pass. Live
  Wayland and private-bus integration remain unverified in this sandbox.
- glide-config: all source files read. Atomic/private writes now reject
  cyclic or overlong symlink chains with ELOOP instead of replacing a link.
  Failure to create a temporary file no longer removes a preexisting file at
  that name. Added a cycle-preservation test. Finished process timeout handling:
  stdin/stdout/stderr use nonblocking pipes and share the child's deadline;
  every I/O thread is joined before returning. Descendants retaining pipe
  ends can no longer make a completed parent hang run(). Two regression
  tests cover inherited output and blocked inherited input; the existing
  large-output and direct-child timeout tests also pass. All 55 unit tests
  pass. Source review complete.
- Latest Clippy passes for glide-app and glide-config with all targets and
  warnings denied. Formatting and git diff whitespace checks pass.
- glide-appmenu: all source and integration test files reviewed. Added a
  shared 4096-node D-Bus menu parsing budget across sibling submenus, with
  regression coverage. GTK Start batches now respect the remaining group
  subscription limit; failed fetches release acquired/requested groups rather
  than leaving subscriptions behind. All 19 unit tests pass. D-Bus integration
  remains unverified because the sandbox denies Unix socket binding.
- Latest checks: 55 glide-config tests and 19 glide-appmenu tests pass.
  Clippy passes for both with all targets and warnings denied; formatting
  completed.
- glide-tray: all five source files and private-bus tests reviewed. Owner
  refresh now walks indices in reverse so removing a vanished item cannot
  shift an unprocessed item or panic at an obsolete index. Retry scheduling
  wakes immediately for deadlines that became due after retry_due(). Added
  boundary coverage. Registration addresses now validate both D-Bus names
  and object paths. Raster output rejects sides above MAX_PIXMAP_SIDE before
  allocation, with an i32::MAX regression assertion. All 44 unit tests pass;
  Clippy passes for all targets with warnings denied. Six private-bus tests
  return early when dbus-daemon produces no address in the sandbox, so their
  reported success does not verify live integration.
- glide-cast: reviewed rtsp.rs and abort.rs. RTSP size checks now apply to
  terminated headers and the complete header-plus-body message; previously a
  complete oversized header bypassed the check, and a maximum-size body
  could exceed the combined limit. Added regression coverage; 19 RTSP tests
  pass and crate-wide Clippy passes for all targets with warnings denied.
  Finished reviewing the remaining modules and embedded integration tests.
  H.264 negotiation now checks every codec for the preferred common mode
  instead of committing to a single codec before checking supported modes;
  unsupported profiles, zero level masks and invalid preference indices are
  rejected. Added regression coverage. Full suite: 55 passed, four failed
  at TCP/UDP bind with EPERM in the sandbox. Hardware encoders are absent;
  real Wi-Fi Direct and streaming behavior remain unverified. Clippy passes.
- glide-layout: source and protocol-boundary tests reviewed. Centering windows
  and dialogs uses saturating coordinate arithmetic near i32 limits.
  Overview rejects non-finite/nonpositive scale, checks label/gap/size sums
  for overflow, and treats negative gap as zero to prevent overlapping tiles.
  Added regressions for coordinate extremes, invalid scales, excessive gaps
  and labels. All 77 unit tests and four protocol-boundary tests pass;
  Clippy passes with all targets and warnings denied. Live compositor output
  remains unverified.
- glide-netbird: Rust source, build script, example and fake-daemon tests
  reviewed. Time parsing rejects invalid month/day combinations, out-of-range
  timezone offsets, oversized year values and nonnumeric fractional suffixes;
  protobuf nanosecond fields outside their valid range are rejected instead
  of normalized into a different timestamp/duration. Added regression cases.
  All 32 unit tests pass. Three fake-daemon tests fail at UnixListener::bind
  with EPERM in this sandbox, before exercising the client. Clippy passes for
  all targets. Event subscription intentionally waits for the first event;
  the existing idle-subscription integration test documents this behavior.
- glide-services: review in progress. Reviewed oneshot, app index, bus
  registration, XDG autostart, logind and polkit-helper source. Wake callbacks
  now run outside the oneshot mutex, preventing reentrant wake deadlocks.
  AppDirectory only marks a process as requested after successful enqueue,
  allowing retries after attaching/replacing an unavailable worker.
  Registration watcher rejects forged NameOwnerChanged messages by checking
  the real bus sender and interface. Added three regression tests; these pass.
  Full initial suite: 46 passed, one failed on sandbox-denied socket I/O,
  three ignored system-polkit checks. Added bus-origin test passes separately.
  Clippy passes for all targets. Finish reading screensaver, polkit_agent,
  x11_menus and test_bus without output truncation before completing review.
- All remaining crates: pending, in the order above. Next: finish
  glide-services, then glide-catalog.
  Keep portal/PAM follow-ups open.

Workspace-wide formatting, clippy, tests and build remain required before
the full review is complete. No compositor runtime or visual validation has
been performed by this review.
