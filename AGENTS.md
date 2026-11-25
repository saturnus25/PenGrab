# AGENTS.md

Guidance for work in the PenGrab repository.

## Scope and references

- PenGrab is a portable Windows x64 TUI built with Rust, Ratatui and Crossterm. Use the private specification and Vicoré TUI 2.0 Base standard under `LocalDocs/` when available. [README.md](README.md), [README.es.md](README.es.md) and [the automation contract](docs/NonInteractive.md) document public behavior.
- Preserve the current design and navigation during unrelated changes. The app reviews selected package operations before executing them, checks installed states and supports signed releases. Document the visibility and verification status of the release channel separately from local implementation.
- Use external TUI projects and skills as learning references. Review the provenance, content, scripts and permissions of any external skill before installing it. Prefer the current dependencies when they cover the change.

## Implementation

- Use the Rust app and its normal Windows launcher, `PenGrab.bat`. Windows PowerShell 5.1 and PowerShell 7 both matter for launch and text output. `PenGrab.ps1` and `modules/` contain the legacy prototype.
- Keep catalog validation in `src/catalog.rs`, match ranking in `src/search.rs`, WinGet inventory in `src/packages.rs`, review and package operations in `src/operations.rs`, and release detection in `src/selfupdate.rs`. Keep actions and state in `src/app.rs`, layout in `src/ui.rs` and `src/ui/settings.rs`, and console acquisition and restoration in `src/terminal.rs`.
- Extend the catalog through `data/categories.json` and `data/tools.json`. Keep category and tool IDs stable. Add established aliases in the data and attach verified WinGet and publisher sources to the same tool entry. Reloading preserves selections by ID and keeps the last valid data on error.
- Keep interface strings in `data/locales/`. English is the default and Spanish is selectable. Distinguish an unconfirmed WinGet ID, an available publisher link and a package that WinGet did not detect on the system.
- Run the installed-package and update queries in parallel in the background once per interactive launch. Allow manual refresh and guard against overlapping scans. Use read-only WinGet queries and keep loading, failure and zero-result states distinct. Retain results for catalog matches only and cancel outstanding workers when leaving the app.

## TUI conventions

- Keep every essential action reachable by keyboard. Mouse and keyboard go through the same actions. Distinguish the active tab, the focused panel, the cursor row and checkbox selection. Keep each state recognizable in monochrome.
- Retain the focused red frame and the stronger cursor row. Pin **All tools** above the category tree and separate it visually when space permits. Keep the scroll track and thumb visible inside overflowing panels and leave a blank column between the scrollbar and the counts or status badges. Use colored category icons with ASCII fallbacks.
- The global search opens a results list as the user types. Prioritize exact names and aliases, then partial and approximate matches. Highlight the query and matching text. Enter or a click reveals the destination, including collapsed ancestors, and briefly highlights its row. Tab applies a filter to the current list and Esc restores the previous view.
- Branch selection includes all descendants, even when a filter hides some. Preserve selection and valid focus across navigation and resize. Esc closes the active dialog or editing mode first. Recompute mouse hit areas from the rendered layout.
- Settings previews the current value and the alternative when space permits. Clicking a label focuses its row. Enter or clicking the value applies it. Keep command-line overrides and `NO_COLOR` effective for the session. Reduced motion keeps the search highlight steady.
- Render on state or size changes and let Ratatui apply the cell diff. Keep blocking work and child process output out of drawing. Apply the dark background before the first themed frame, restore the host cursor, colors and console modes on exit, and preserve text output with redirection.

## Portability and distribution

- Resolve the catalog, translations and portable preferences relative to the app folder. The launcher must work from a different current directory. A portable bundle uses `config/portable` to keep preferences beside the app.
- Interactive installation asks for a destination when neither `-InstallPath` nor `-ChoosePath` is supplied. Enter accepts the default. Show the resolved destination at confirmation and keep automated confirmations and update handoffs prompt-free.
- Build the portable ZIP with `Package-Portable.ps1` and signed distribution with `Package-Release.ps1`. Ship the MIT license and third-party notices in the ZIP. Update the notices when shipped dependencies change. The executable embeds the unlettered icon. The outlined PNGs in `assets/` are GitHub branding.
- Keep [the automation contract](docs/NonInteractive.md) synchronized with the installer and CLI, including JSON outcomes and exit codes. Unattended package requests use explicit IDs and user-scope portable WinGet installers. App-only installation accepts an empty selection. `-NonInteractive` skips prompts, UAC and TUI launch. Dry runs validate the plan without applying package changes.
- Release scripts support invocation as files and script blocks in both PowerShell versions. Temporary `Run` verifies and extracts the bundle, uses `config/temporary` to skip self-update and deletes its owned staging directory at exit. Verify cleanup with a trailing separator in the temporary parent path.
- Signed installations cache application release checks for at least 24 hours. Keep cancellation selected initially in the `F8` confirmation. A successful update preserves preferences and keeps the previous app directory and the result report.

## Publication and local material

- The release channel is [saturnus25/PenGrab](https://github.com/saturnus25/PenGrab/releases). When authorized to publish, review staged files and ZIP contents, verify the signed manifest and compare the uploaded asset hashes. The signed assets are the versioned Windows ZIP, `install.ps1`, `bootstrap.ps1`, `PenGrab.release.json`, `PenGrab.release.sig` and `PenGrab.release-public.xml`. Use a new version for changed release assets.
- Reuse the established DPAPI-protected private signing key for compatible updates. `PenGrab.release-public.xml` is the public verification key. Keep private signing material, guest credentials, VM files and private test evidence out of commits, release assets, screenshots and logs.
- Keep local proof files in `Proofs/` and private project inputs in `LocalDocs/`. Both directories stay out of the public tree and ZIP. Public screenshots belong in `docs/images/` and use representative or labeled simulated data.
- Keep the English and Spanish quick starts aligned. Show both `irm` commands directly and keep the AI-assisted development notice visible. Use the maintainer's requested Git author and co-author metadata.
- For authorized clean-Windows checks, use `vmrun` from the console with `nogui`. Test in an isolated clone, remove guest files created for the test and verify cleanup. Honor the maintainer's instruction about retaining the clone.

## Verification

Run the applicable checks from the project root.

```powershell
cargo fmt --check
cargo clippy --release --all-targets --locked -- -D warnings
cargo test --release --locked
cargo build --release --locked
```

| Change | What to verify |
| --- | --- |
| Interface | Launch `PenGrab.bat`. Check keyboard and mouse input, English and Spanish, monochrome, wide and compact views including 32×16, and live resize with focus and hit areas intact. |
| Catalog and diagnostics | Check reload and selection, aliases and misspellings, and loading, empty, update and failure states. Use labeled mocks for package actions and read-only queries for real inventory. |
| Installer and bootstrap | Test PowerShell 5.1 and 7. Cover typed, default and explicit paths, cancellation and unattended installation. Check that the bootstrap returns control and cleans up. |
| Package and release | Inspect the ZIP and open the extracted app from a moved folder with spaces and a different working directory. Check persisted preferences. After publication, test anonymous asset downloads, the exact README `irm` commands and an `F8` update from the previous release with cancellation, preferences and backup checks. |

Record the version, shells, backend and environment actually tested. Distinguish local signed-bundle tests, public GitHub tests and real Windows Terminal mouse checks.
