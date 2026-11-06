# Ferrowl — handoff package for Claude Code

Ferrowl is a desktop email client written in Rust: fast, with a local search index, built-in OpenPGP and six color themes. This folder contains the design and the specifications to build it.

## Contents

```
ferrowl-handoff/
├── README.md              ← this file: specifications and instructions
├── design/
│   ├── Main.dc.html       ← main window (folders · list · reader · Appearance panel)
│   ├── Settings.dc.html   ← settings, 9 sections
│   ├── Compose.dc.html    ← "New message" window
│   ├── Icon.dc.html       ← icon sheet and palette
│   ├── canvas.json        ← board layout (reference only)
│   └── themes.json        ← color tokens for the 6 themes, typography, measurements
└── icons/
    ├── ferrowl.svg        ← master icon 512
    ├── ferrowl-64.svg / -32.svg / -16.svg   ← simplified versions for small sizes
    ├── tray-light.svg / tray-dark.svg / tray-accent.svg  ← outline icon for the system bar
    ├── ferrowl.ico        ← Windows (16/32/48/256)
    └── png/               ← 16 … 1024 px, tray 32 px
```

### How to read the `.dc.html` files
They are HTML mockups with a small template engine. Use them as a **visual and behavioral reference**, not as code to reuse:
- inline styles give the exact colors, spacing, radii and sizes;
- `{{t.xxx}}` is a theme token (see `themes.json`);
- `<sc-for>` is a list, `<sc-if>` is a conditional block;
- the `Component` class at the bottom (`renderVals()`) holds the sample data and the state logic (message selection, active folder, theme, settings sections, toggles).

All content (senders, emails, amounts) is **made up**. Values in square brackets, such as `[KEY FINGERPRINT]`, are placeholders.

---

## Proposed stack
This is a proposal: check the latest crate versions before pinning them.

| Area | Suggested crates |
|---|---|
| GUI | **iced** (pure Rust, Elm architecture, customizable themes). Alternative: Slint |
| Async runtime | tokio |
| IMAP | async-imap (with IDLE) |
| SMTP | lettre |
| JMAP | jmap-client |
| MIME parsing / building | mail-parser, mail-builder |
| Local cache | SQLite (rusqlite or sqlx) |
| Full-text search | tantivy |
| OpenPGP | sequoia-openpgp |
| Passwords | keyring (system keychain) |
| Email HTML | ammonia (sanitization) and no remote resources by default |
| Notifications | notify-rust |
| Tray | tray-icon |
| Configuration | serde + toml, paths with directories |
| Logging | tracing |

### Recommended structure (Cargo workspace)
```
crates/
  ferrowl-core/     models (Account, Folder, Message, Thread, Label, Rule), errors
  ferrowl-store/    SQLite + Maildir, tantivy index
  ferrowl-proto/    IMAP / JMAP / SMTP behind a common `MailBackend` trait
  ferrowl-crypto/   OpenPGP, signature verification
  ferrowl-rules/    filters (Sieve-compatible)
  ferrowl-ui/       iced app: views, themes, shortcuts
  ferrowl/          binary
```

---

## Screens

### 1. Main window (`Main.dc.html`, reference 1440×900, fluid layout)
- **Top bar**: logo and name, global search with operators (`from:`, `has:attachment`, `label:`) and the `Ctrl K` shortcut, Sync button, Settings (opens the quick Appearance panel) and account avatar.
- **Sidebar** (min 220 px): **Compose** button, folders (Inbox, Starred, Sent, Drafts, Archive, Spam, Trash) with counters, colored **Labels**, **Accounts** with status (connected or offline) and a status box for sync and indexing.
- **Message list** (min 340 px): All / Unread / Attachments filters. Each row shows a dot if the message is unread, the sender (bold if unread), the attachment icon, the time, the subject, a preview line and a label. The selected row has the `soft` background and a 3 px `accent` bar on the left.
- **Reading pane**: bar with Reply, Forward, Archive, Delete, Snooze and a security badge ("Signed" / "Encrypted · OpenPGP"). Then the subject (26 px), sender with avatar, date, favorite button, body (text at most 820 px wide), attachment cards and a quick reply field (`R`).
- **Appearance panel** (popover from the gear): grid with the 6 themes, "follow the system dark theme" option and an "All settings" link.
- Below roughly 1000 px the columns stack vertically.

### 2. Settings (`Settings.dc.html`)
Top bar with "Back to mail" and settings search. On the left the list of sections, on the right the content at most 760 px wide, organized in cards with "title + description + control" rows.
1. **General**: language, launch at login, default mail app (mailto:), when to mark as read, group into conversations.
2. **Accounts**: list of accounts with status and "Add account". Details: display name, email, IMAP and SMTP servers, authentication (password in keychain, OAuth2, certificate), sync (IDLE, every 5 or 15 minutes, manual), Test connection, Folders and aliases, Remove.
3. **Appearance**: 6 themes, density (Compact, Comfortable, Spacious), text size (12 to 20), reading font, text preview in the list.
4. **Notifications**: desktop notifications, VIP contacts only, sound, badge counter on the icon, do not disturb during a time window.
5. **Composing**: default format (Text, Markdown, HTML), undo send (0/5/10/30 s), reply position, spell check, signature per account.
6. **Security and privacy**: OpenPGP keys (import or generate, Ed25519), always sign, encrypt when possible, block remote images, warn about suspicious links, app lock on inactivity.
7. **Filters and rules**: ordered "If … then …" rules with a toggle, New rule.
8. **Shortcuts**: scheme (Ferrowl, Vim, Gmail) and editable table.
9. **Advanced**: statistics (indexed messages, used space, attachment cache), data folder, rebuild index, log level, hardware acceleration, export and import settings as TOML, Reset (red zone).

### 3. New message (`Compose.dc.html`, 760×640)
Sender (account choice), recipients as chips, Cc/Bcc, subject, formatting bar (bold, italic, underline, list, link, code) with the current format shown, message body, attachments, **Send** (`Ctrl ↵`), schedule send, attach, **Sign and encrypt (OpenPGP)** toggle, discard draft, "Draft saved" indicator.
The mockup is drawn only in Ruggine: in the app it must use the active theme.

---

## Themes
`design/themes.json` contains all the tokens. The default is **Ruggine**. Theme switching is immediate and applies to all windows and to the logo in the header. "Notte" is the dark theme: on top of `accent` it uses dark text (`onAccent`). Text meets a minimum contrast of 4.5:1. Clickable areas are at least 44 px.

## Default shortcuts
| Action | Key |
|---|---|
| New message | C |
| Reply / Reply all | R / Shift R |
| Forward | F |
| Archive | E |
| Delete | Del |
| Next / previous message | J / K |
| Search | Ctrl K |
| Send | Ctrl ↵ |

## Icon
An envelope-shaped owl: the ears are the raised corners, the flap acts as eyebrows, the eyes are iron hex nuts and the beak sits on the tip of the envelope. Below 32 px use the `-32`/`-16` files, which are simplified. For the system bar use `tray-*`.
Palette: Ruggine `#A63A0C` · Ferro `#2A1A12` · Carta `#FBF3EA` · Brace `#E07A4A`.

---

## Suggested roadmap
1. **Skeleton**: workspace, iced window with the three columns, themes from `themes.json`, fake data.
2. **Reading**: one IMAP account, folder sync, SQLite cache, list and reader (sanitized HTML, remote images blocked).
3. **Sending**: composing and SMTP, drafts, undo send.
4. **Search**: tantivy index with operators.
5. **Security**: OpenPGP (sign, encrypt, verify).
6. **The rest**: multiple accounts, JMAP, rules, notifications and tray, full settings, shortcuts.

## First message to give Claude Code
> Read `README.md` and the files in `design/`. Create the Ferrowl Cargo workspace as described and implement step 1 of the roadmap: main window in iced faithful to `design/Main.dc.html`, with the 6 themes loaded from `design/themes.json` and selectable from the Appearance panel, using fake data. Use the icons in `icons/`. Before writing code, propose the crate versions and the module structure to me.
