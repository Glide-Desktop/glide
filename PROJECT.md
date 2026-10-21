# Project: Glide Island (Meridian Island) Concept Prototype

## Architecture
Standalone high-fidelity interactive prototype for the Glide desktop environment implementing "Glide Island" (Meridian Island) in `design/dynamic-island-concept.html`.
- **Core Philosophy**: "Structured Calm" departing strictly from iOS/macOS notch-cutout tropes, OLED blacks, and cartoon bouncy springs in favor of architectural precision, frosted warm glass, and monotonic `E_OUT` motion curves.
- **Form Scale**: Strict 5-radius scale (`--r-1: 4px`, `--r-2: 8px`, `--r-3: 12px`, `--r-4: 18px`, `--r-round: 999px`). Zero intermediate radii.
- **Motion Physics**: `E_OUT` (`cubic-bezier(0.2, 0, 0, 1)`) for spatial expansions; `E_IO` (`cubic-bezier(0.4, 0, 0.2, 1)`) for color shifts; `E_OVER` (`cubic-bezier(0.32, 1.28, 0.5, 1)`) only for micro switch toggles and badge pop-ins.
- **Glass Materials**: Light theme frosted warm white glass (`rgba(251, 250, 248, 0.82..0.84)` / `#fbfaf8`), Dark theme warm graphite glass (`rgba(38, 37, 42, 0.84)` / `#29282d`). Strictly no pitch-black OLED blobs (`#000000`).
- **State Layers**: Translucent `currentColor` overlays (`--st-hover: .08`, `--st-press: .13`, `--st-drag: .18`) via `::before`.
- **Typography**: Inter for UI text, JetBrains Mono with tabular numbers for digits (`.t-num`). Uppercase exclusively in `.t-micro`.
- **Tripartite Architecture**:
  1. Topbar & Island Shell (Variants 1, 2, 3)
  2. Calendar & Notification Stack
  3. Inline & Expanded Media Player
  4. Playground Controller Dock & Simulation Suite

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | Variant 1: Integrated Glass Shelf | Recessed well in topbar, expands downward seamlessly attached without gap | M1 | ORIGINAL_REQUEST R1.1 |
| 2 | Variant 2: Dock-Style Floating Plate | Autonomous floating glass plate, `--bezel-hi` top specular line, `--el-2`/`--el-3` shadow | M1 | ORIGINAL_REQUEST R1.2 |
| 3 | Variant 3: Dual-Sector Status Horizon | Symmetrical instrument-panel console divided into functional bays (Media, Status, Calendar) | M1 | ORIGINAL_REQUEST R1.3 |
| 4 | Meridian Motion Physics | Pure `E_OUT` and `E_IO` curves, T1–T3 durations, strict ban on springs/jelly | M1 | ORIGINAL_REQUEST R2 |
| 5 | Meridian Glass & Color Tokens | Frosted white glass light mode, warm graphite dark mode, no OLED black, state layers | M1 | MERIDIAN_DESIGN_SPEC |
| 6 | Embedded Interactive Calendar | 7×6 42-cell Sakamoto grid, Ukrainian month/day names, month nav slide, today jump/highlight | M2 | ORIGINAL_REQUEST R3 |
| 7 | Notification Center Header | Uppercase `.t-micro` title, "Очистити все" button, animated DND toggle with moon icon | M2 | ORIGINAL_REQUEST R3 |
| 8 | Notification Stack & Dismiss | Rich cards, 32px icons, grouped count badges (`+N`), timestamp, individual dismiss (`×`), empty state | M2 | ORIGINAL_REQUEST R3 |
| 9 | Compact Media Activity Indicator | Discrete mini audio level meter (3-bar animated) and track info in compact island | M3 | ORIGINAL_REQUEST R4 |
| 10 | Expanded Media Player Card | 56px album art, dynamic cover tinting (`MEDIA_TINT = 0.14`), metadata, transport controls | M3 | ORIGINAL_REQUEST R4 |
| 11 | Interactive Audio Scrub Bar | Draggable/clickable scrub track with pointer capture, elapsed and remaining timestamps | M3 | ORIGINAL_REQUEST R4 |
| 12 | Floating Playground Dock | Role-toolbar container with live segmented switches, 8-accent swatches, audio track selector | M4 | ORIGINAL_REQUEST R5 |
| 13 | Live Alert Simulation Suite | Toast/banner simulation buttons (+Telegram, +Mail, +Calendar, +System), auto-expand & collapse | M4 | ORIGINAL_REQUEST R5 |
| 14 | Theme & Accent Switcher | Real-time Dark/Light theme toggle, 8 canonical 6-step accent ramps with soft tint adjustment | M4 | ORIGINAL_REQUEST R5 |
| 15 | Keyboard Accessibility & A11y | Escape key hierarchical unwinding, Tab focus navigation, aria attributes | M4 | ORIGINAL_REQUEST R5 |
| 16 | Comprehensive E2E Test Suite | 4-tier requirement-driven opaque-box verification test harness and test cases | E2E | Project Pattern |
| 17 | Final Acceptance Verification & Adversarial Hardening | 100% pass across E2E tiers + white-box challenger stress tests + forensic audit veto | M5 | Project Pattern |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| E2E | E2E Testing Track | Test harness, test runners, Tiers 1-4 test cases, TEST_READY.md | none | IN_PROGRESS |
| M1 | Architectural Shell & Motion Physics | 3-Variant Switcher, topbar anchor, Meridian glass & token system, E_OUT motion | none | PLANNED |
| M2 | Meridian Calendar & Notification Stack | 7×6 Sakamoto calendar, UK localization, notification feed, badges, DND switch | M1 | PLANNED |
| M3 | Media Player Live Activity & Expanded Card | Compact level meter, expanded player, scrub bar, dynamic cover tinting | M1 | PLANNED |
| M4 | Playground Dock & Simulation Suite | Floating dock, theme/accent switches, alert generator, keyboard navigation | M1, M2, M3 | PLANNED |
| M5 | Final E2E Pass & Adversarial Hardening | 100% pass on all E2E test tiers + Tier 5 Challenger stress testing + Audit | E2E, M4 | PLANNED |

## Interface Contracts
### Shell ↔ Content Modules
- `#glideIsland` root container with `data-variant="shelf|dock|horizon"`, `data-state="compact|alert|media|calendar"`, `data-theme="dark|light"`.
- Event contracts:
  - `setIslandState(state)`: Transitions island between states with `E_OUT` motion.
  - `setIslandVariant(variant)`: Updates architectural structure and layout classes immediately.
  - `setTheme(theme)`: Toggles light frosted glass / dark warm graphite glass, updates CSS custom properties.
  - `setAccent(accentName)`: Re-applies 6-step ramp and recalculates `--acc-soft`, `--acc-ring`.
  - `simulateAlert(alertData)`: Expands island to alert banner state, displays message, auto-collapses or persists.
  - `updatePlayback(trackData)`: Syncs track title, artist, album art, elapsed/duration, and `--art-soft` tint.

## Code Layout
- `design/dynamic-island-concept.html`: Single self-contained HTML/CSS/JS file.
- `.agents/teamwork/`: Metadata, plans, progress, handoffs, and verification logs.
