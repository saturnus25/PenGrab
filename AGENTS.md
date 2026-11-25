# AGENTS.md

Guidance for work in the PenGrab repository.

## Scope and references

- PenGrab is a portable Windows x64 TUI built with Rust, Ratatui and Crossterm. Use the private specification and Vicoré TUI 2.0 Base standard under `LocalDocs/` when available. The public README documents the shipped application.
- Preserve the current design and navigation during unrelated changes. The app reviews selected package operations before executing them, checks installed states and supports signed releases. Document the visibility and verification status of the release channel separately from local implementation.
- Use external TUI projects and skills as learning references. Review the provenance, content, scripts and permissions of any external skill before installing it. Prefer the current dependencies when they cover the change.

## Implementation

- Use the Rust app and its normal Windows launcher, `PenGrab.bat`. Windows PowerShell 5.1 and PowerShell 7 both matter for launch and text output. `PenGrab.ps1` and `modules/` contain the legacy prototype.
- Keep catalog validation in `src/catalog.rs`, match ranking in `src/search.rs`, WinGet inventory in `src/packages.rs`, review and package operations in `src/operations.rs`, and release detection in `src/selfupdate.rs`. Keep actions and state in `src/app.rs`, layout in `src/ui.rs` and console acquisition and restoration in `src/terminal.rs`.
- Extend the catalog through `data/categories.json` and `data/tools.json`. Keep category and tool IDs stable. Add established aliases in the data and attach verified WinGet and publisher sources to the same tool entry. Reloading preserves selections by ID and keeps the last valid data on error.
- Keep interface strings in `data/locales/`. English is the default and Spanish is selectable. Distinguish an unconfirmed WinGet ID, an available publisher link and a package that WinGet did not detect on the system.
- Run the installed-package and update queries in parallel in the background once per interactive launch. Allow manual refresh and guard against overlapping scans. Use read-only WinGet queries and keep loading, failure and zero-result states distinct. Retain results for catalog matches only and cancel outstanding workers when leaving the app.

## TUI conventions

- Keep every essential action reachable by keyboard. Mouse and keyboard go through the same actions. Distinguish the active tab, the focused panel, the cursor row and checkbox selection. Keep each state recognizable in monochrome.
- Retain the focused red frame and the stronger cursor row. Pin **All tools** above the category tree and separate it visually when space permits. Keep the scroll track and thumb visible inside the panel when its contents overflow. Use colored category icons with ASCII fallbacks.
- The global search opens a results list as the user types. Prioritize exact names and aliases, then partial and approximate matches. Highlight the query and matching text. Enter or a click reveals the destination, including collapsed ancestors, and briefly highlights its row. Tab applies a filter to the current list and Esc restores the previous view.
- Branch selection includes all descendants, even when a filter hides some. Preserve selection and valid focus across navigation and resize. Esc closes the active dialog or editing mode first. Recompute mouse hit areas from the rendered layout.
- Render on state or size changes and let Ratatui apply the cell diff. Limit destination highlighting to short pulses on the destination row. Keep blocking work and child process output out of drawing. Restore console state on exit and keep text modes usable with redirected output.

## Portability and distribution

- Resolve the catalog, translations and portable preferences relative to the app folder. The launcher must work from a different current directory. A portable bundle uses `config/portable` to keep preferences beside the app.
- Build the Windows x64 preview ZIP with `Package-Portable.ps1` and signed distribution with `Package-Release.ps1`. Keep the private signing key in the ignored local directory. The executable embeds the unlettered icon. The outlined PNGs in `assets/` are GitHub branding.
- Keep [the public automation contract](docs/NonInteractive.md) synchronized with the installer and CLI. Unattended operations require explicit tool or category IDs and must stay free of prompts and UAC.
- Keep local previews, package copies for review and other proof files in `Proofs/`. Treat `Proofs/` and `LocalDocs/` as local material excluded from version control and distribution.

## Verification

Run the applicable checks from the project root.

```powershell
cargo fmt --check
cargo clippy --release --all-targets --locked -- -D warnings
cargo test --release --locked
cargo build --release --locked
```

For interaction or visual changes, launch `PenGrab.bat` in a terminal. Check keyboard navigation, active focus, mouse targets, and wide and compact views, including 32×16 where applicable. Verify English and Spanish, monochrome, and the text modes touched by the change.

For package diagnostics, exercise loading, empty, update and failure states. Use labelled mock previews for install or update examples and verify real queries without running package operations. For changes to packaging, inspect the ZIP contents and test the extracted launcher from a moved path with spaces. Check that portable preferences survive reopening. Record the shells and terminal backends actually tested.
