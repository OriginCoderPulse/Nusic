# Changelog

All notable changes to this project are documented in this file.

## [0.2.1] - 2026-07-08

### Added

- **`nusic --help` / `-h`** — print CLI usage from the shell.

### Fixed

- **Shuffle & repeat in background** — daemon keeps queue state in sync; background autoplay follows shuffle/repeat rules.
- **Shuffle & repeat on re-attach** — UI changes sync to the daemon on toggle and detach.

## [0.2.0] - 2026-07-08

### Added

- **Background playback** — keep music playing after leaving the UI via a background daemon and IPC.
- **Pin mode** — `Shift+P` toggles a pin mark; when pinned, the panel title shows `Nusic · Pin`.
- **Pinned quit** — with Pin active, `q` / `Esc` exits the UI and continues playback; without Pin, `q` stops playback entirely.
- **Re-attach** — run `nusic` again while the daemon is active to restore progress, queue, selection, lyrics, and Pin state.
- **`nusic --exit`** — stop the background player from the shell.
- **Session resume** — playback position is restored with seek support when handing off to the daemon.

### Changed

- Song info panel prefers the currently playing track over the list selection.
- Queue and list selection are persisted by file path for stable restore after library rescans.

## [0.1.3] - 2026-07-07

- Improved metadata parsing and documentation.

[0.2.1]: https://github.com/OriginCoderPulse/Nusic/releases/tag/v0.2.1
[0.2.0]: https://github.com/OriginCoderPulse/Nusic/releases/tag/v0.2.0
[0.1.3]: https://github.com/OriginCoderPulse/Nusic/releases/tag/v0.1.3
