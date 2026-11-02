# Ferrowl

A fast desktop email client written in Rust, with a native UI built on [iced](https://iced.rs).

<img src="assets/icons/png/ferrowl-128.png" width="64" alt="Ferrowl icon">

## Features

- **Multiple IMAP + SMTP accounts**: switch from the sidebar, add/edit/remove in Settings (IMAP over TLS on 993; SMTP on 465 TLS or 587 STARTTLS), pure-Rust TLS (rustls)
- Three-pane window: folders · message list · reader
- Special folders detected automatically (Inbox, Sent, Drafts, Archive, Spam, Trash), unread counters, collapsible tree for all other folders
- List filters (All / Unread / Attachments) and search with `from:`, `has:attachment`, `is:unread`
- **Local cache + full-text search** (SQLite FTS5): folders open instantly from the cache, older messages are indexed in the background, and search covers sender, subject and text of every cached message (prefix matching, accents ignored)
- Reply, forward (with original attachments), archive, mark as unread
- **Delete moves to Trash**; deleting inside Trash asks for confirmation and removes the message for good
- Sent messages are saved to the Sent folder
- Compose with Cc/Bcc and file attachments
- Formatted messages: toolbar and shortcuts (`Ctrl B`/`I`/`K`) write Markdown, with a live preview; sent as `multipart/alternative` (HTML + plain text). Raw HTML is escaped and only `http`/`https`/`mailto`/`tel` links are kept. Can be switched to plain text only.
- Attachments saved to the Downloads folder with one click
- DKIM badge when the server reports `dkim=pass`
- **OpenPGP** (pure Rust, [rPGP](https://github.com/rpgp/rpgp)): create or import a key in Settings › Security, then *Sign* / *Encrypt* in the composer. PGP/MIME (RFC 3156) out, PGP/MIME and inline PGP in, with badges for encrypted / signed / invalid signature / unknown key. Public keys travel automatically in the `Autocrypt` header; others can be imported from `.asc` files. Replies to encrypted mail are encrypted by default. Interoperability checked against GnuPG 2.4 (both directions, signatures and encryption)
- HTML mail shown formatted (headings, emphasis, lists, quotes, links, tables) without a web engine: scripts, styles and forms are dropped and **images/remote content are never loaded**, so tracking pixels don't fire. One click switches to plain text. Links open in the browser; `mailto:` links open a new message.
- **6 color themes** (Ruggine, Ardesia, Bosco, Lavanda, Sabbia, Notte) defined in [`design/themes.json`](design/themes.json)
- **10 languages**: English, Italiano, Français, Deutsch, Español, Português, 中文, 日本語, 한국어, العربية (right-to-left layout). The system language is used by default, falling back to English.
- Keyboard shortcuts: `C` compose · `R` reply · `F` forward · `E` archive · `U` mark unread · `J`/`K` or ↓/↑ next/previous · `Del` delete · `Ctrl K` or `/` search · `Ctrl Enter` send · `Esc` close
- **Push**: a second IMAP connection waits in IDLE on the inbox, so new mail shows up as soon as the server reports it, with a desktop notification (can be turned off in Settings › General). Servers without IDLE fall back to the refresh every 2 minutes

## Download

Prebuilt binaries for Linux (x86_64), Windows (x86_64) and macOS (Apple Silicon) are attached to each [release](https://github.com/creativalab/client-mail/releases).

The binaries are not signed yet:
- **macOS**: right-click → Open, or `xattr -d com.apple.quarantine ferrowl`
- **Windows**: SmartScreen → "More info" → "Run anyway"

## Build from source

```bash
cargo run --release
```

Requires Rust 1.85+ (edition 2024). On Linux no extra system packages are needed besides a graphics driver; the app runs on Wayland and X11.

## Configuration

On first launch Ferrowl opens the account setup. Settings are stored in:

| OS | Path |
|---|---|
| Linux | `~/.config/ferrowl/config.toml` |
| macOS | `~/Library/Application Support/ferrowl/config.toml` |
| Windows | `%APPDATA%\ferrowl\config.toml` |

The local cache (message list, previews, text of opened messages, search index) is `cache.sqlite` in the data directory (`~/.local/share/ferrowl/` on Linux, `~/Library/Application Support/ferrowl/` on macOS, `%LOCALAPPDATA%\ferrowl\` on Windows), readable by the owner only. Deleting it is safe: it is rebuilt from the server.

OpenPGP keys live in `keys/` in the same data directory (owner-only): `own/` for your secret keys, `contacts/` for correspondents' public keys.

The password (and the optional OpenPGP key passphrase) is currently stored in this file (permissions `600` on Linux/macOS). Moving it to the system keychain is on the roadmap.

For Gmail, use an [app password](https://support.google.com/accounts/answer/185833).

## Translations

UI strings live in [`locales/`](locales/), one TOML file per language. `locales/en.toml` is the reference: every other file must contain the same keys with the same `{placeholders}` — `cargo test` checks this.

To add a language: copy `en.toml`, translate it, and register it in `LANGS` in [`src/i18n.rs`](src/i18n.rs).

## Development

```bash
cargo test                                   # unit tests (themes, locales, IMAP helpers)
cargo test -- --ignored                      # network tests (TLS handshake with imap.gmail.com)
cargo test live_account -- --ignored --nocapture   # uses the configured account
```

Environment variables useful during development:

| Variable | Effect |
|---|---|
| `FERROWL_LANG=ar` | Force a UI language for this run |
| `FERROWL_THEME=notte` | Force a theme for this run |
| `FERROWL_DEV_VIEW=settings\|appearance\|general\|security\|compose\|popover` | Open a view directly |
| `FERROWL_SCREENSHOT=out.png` | Capture the window after `FERROWL_SCREENSHOT_DELAY` seconds (default 8) and quit |
| `FERROWL_SCREENSHOT_OPEN=1` | Open the first message before capturing |

The design handoff (mockups, tokens, icon) is in [`design/`](design/).

## Roadmap

- Password in the system keychain
- Search across all folders
- Optional image loading per sender
- Tray icon, notification actions (reply/archive from the notification)
- OpenPGP: protected subject (memory hole), key passphrase in the system keychain
- Unified inbox across accounts, JMAP

## Support

Problems or questions: **support@ferrowl.com**

## License

[MIT](LICENSE). Bundled fonts: Inter, JetBrains Mono and Noto Sans Arabic, all under the SIL Open Font License (see `assets/fonts/`).
