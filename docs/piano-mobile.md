# Ferrowl per iOS e Android — piano

Stato: proposta, 2 novembre 2026.

## Obiettivo

App Ferrowl native per iPhone/iPad e Android con le stesse funzioni della
versione desktop (account IMAP/SMTP, mail HTML, ricerca, OpenPGP, 10 lingue,
6 temi), riusando il più possibile il codice Rust del desktop.

## Cosa si può riusare

Il desktop oggi ha circa 9.800 righe di Rust.

| Area | File | Righe | Riusabile sul mobile |
|---|---|---|---|
| IMAP/SMTP, MIME, PGP/MIME | `mail.rs`, `backend.rs` | ~1.570 | sì, così com'è |
| Cache + ricerca full-text (SQLite FTS5) | `cache.rs` | ~360 | sì |
| OpenPGP, Autocrypt | `crypto.rs` | ~420 | sì |
| Markdown → HTML (scrittura) | `markdown.rs` | ~130 | sì |
| Configurazione, traduzioni (`locales/*.toml`), temi (`design/themes.json`) | `config.rs`, `i18n.rs`, `theme.rs` | ~560 | sì (dati condivisi, caricamento da adattare) |
| Anteprima allegati (PDF con hayro) | `preview.rs` | ~190 | sì (restituisce immagini PNG) |
| Rendering mail HTML (Blitz) | `html_view.rs` | ~350 | non serve: sul telefono si usa la WebView di sistema |
| Push (IMAP IDLE) | `push.rs` | ~100 | no: i sistemi mobili non tengono connessioni aperte in background |
| Interfaccia (iced) | `app.rs`, `ui/*` | ~5.600 | no: iced non supporta iOS/Android |

Circa il **40% del codice** (il "motore") passa in una libreria comune;
l'interfaccia si riscrive per ogni piattaforma.

## Architettura

```
ferrowl-core (libreria Rust, senza interfaccia)
├── posta: IMAP, SMTP, MIME, cartelle, flag
├── cache: SQLite + FTS5
├── crittografia: OpenPGP, Autocrypt
├── scrittura: Markdown → HTML, allegati
└── traduzioni / temi (file dati condivisi)
        │
        ├── desktop: ferrowl (iced) — l'app attuale, usa la libreria direttamente
        ├── iOS: app SwiftUI       ── collegamento UniFFI (Swift)
        └── Android: app Jetpack Compose ── collegamento UniFFI (Kotlin)
```

- **UniFFI** (di Mozilla, usato da Firefox e dai client Matrix) genera il codice
  Swift e Kotlin per chiamare la libreria Rust. Il core espone funzioni
  asincrone (`connect`, `list_folders`, `headers`, `message`, `send`,
  `search`, …) e tipi di dati semplici (`Header`, `FullMessage`, `Folder`).
- **Mail HTML**: mostrate in `WKWebView` (iOS) / `WebView` (Android) con
  JavaScript disattivato, Content-Security-Policy restrittiva e contenuti
  remoti bloccabili (stessa impostazione privacy del desktop).
- **Segreti**: password e chiavi OpenPGP nel Portachiavi iOS / Keystore Android
  (anche il desktop dovrebbe passare al portachiavi di sistema).
- **Interfaccia nativa**, così le app si comportano come le altre del telefono
  (gesti, condivisione, tema scuro, testo dinamico, accessibilità).

### Alternativa valutata: un'unica interfaccia per tutto

Slint o Dioxus permettono un'unica interfaccia Rust per desktop e mobile. Si
arriva prima a una prima versione, però: va rifatta anche l'interfaccia
desktop, il loro supporto mobile è più giovane di SwiftUI/Compose e il
risultato sembra meno "nativo". Consigliata solo se un team piccolo deve
fare tutto da solo.

## Notifiche push

Su iOS (e su Android con i limiti di batteria attuali) un'app non può tenere
aperta una connessione IMAP in background, quindi IDLE non funziona come sul
desktop.

Opzioni:

1. **Server di inoltro push (consigliato)**: un piccolo servizio tiene le
   connessioni IDLE al posto dell'utente e manda le notifiche tramite APNs
   (Apple) / FCM (Google) ("nuova posta in arrivo"), senza conservare il
   contenuto dei messaggi. Serve hosting, le credenziali IMAP dell'utente (o
   una password per app) sul server, un'informativa privacy e la cifratura per
   utente delle credenziali salvate.
2. **Solo aggiornamento in background**: il sistema risveglia l'app ogni
   15–60 minuti (senza garanzie). Nessun server, ma notifiche in ritardo.
3. Provider con push nativo (Gmail API, Microsoft Graph, JMAP push): niente
   server solo per quegli account.

Proposta: partire con la 2 (funziona con ogni account, nessuna
infrastruttura) e aggiungere la 1 come servizio opzionale.

## Fasi e stime

Stime per uno sviluppatore esperto; i tempi di calendario dipendono anche
dalle revisioni e dall'approvazione di App Store / Play Store.

| # | Fase | Contenuto | Stima |
|---|---|---|---|
| 0 | Estrazione del core | Workspace Cargo: `ferrowl-core` + `ferrowl-desktop`; spostare posta/cache/crittografia/traduzioni; il desktop continua a funzionare e i test restano verdi | 1 settimana |
| 1 | Collegamenti | API UniFFI, gestione asincrona, tipi di errore; build per `aarch64-apple-ios`, simulatore, `aarch64-linux-android`, `x86_64-linux-android`; pacchetti XCFramework + AAR nella CI | 1–2 settimane |
| 2 | Android, prima versione | Configurazione account, cartelle, elenco messaggi, lettura (WebView), scrittura, invio, ricerca, temi, 10 lingue | 3–4 settimane |
| 3 | iOS, prima versione | Stesse funzioni in SwiftUI | 3–4 settimane |
| 4 | Parità col desktop | OpenPGP, allegati (anteprima/condivisione), più account, azioni con lo swipe, aggiornamento in background + notifiche locali | 3 settimane |
| 5 | Store | Icone e screenshot, schede privacy, TestFlight / test interni Play, correzioni dopo la revisione | 1–2 settimane |
| 6 | Server push (opzionale) | Server, APNs/FCM, cifratura credenziali, informativa privacy | 3–4 settimane |

Totale fino alla pubblicazione sugli store, senza server push: **circa 3–4
mesi** per uno sviluppatore; con due persone Android e iOS (fasi 2–3) vanno in
parallelo.

## Account e costi

- Apple Developer Program: 99 USD l'anno (necessario per TestFlight e App Store).
- Google Play Console: 25 USD una tantum.
- Per compilare e firmare l'app iOS serve un Mac con Xcode (la CI può usare i
  runner macOS di GitHub, che sui repo privati costano 10 volte i minuti).
- Server push: una piccola VPS; APNs e FCM sono gratuiti.

## Rischi

- **Su mobile le WebView sostituiscono Blitz**: la resa può differire un po'
  dal desktop; accettabile (succede anche con gli altri client di posta).
- **Revisione App Store**: per i client di posta chiedono una gestione chiara e
  trasparente delle credenziali, soprattutto con un server push.
- **OpenPGP su mobile**: importazione delle chiavi da file e richiesta della
  passphrase vanno progettate con cura; le chiavi devono stare in
  Portachiavi/Keystore.
- **Dimensione dell'app**: il core Rust con SQLite e rPGP aggiunge circa
  8–15 MB per architettura; accettabile, da misurare nella fase 1.

## Decisioni da prendere

1. Interfacce native (consigliato) o un'unica interfaccia Rust (Slint/Dioxus)?
2. Prima Android, prima iOS, o entrambe in parallelo?
3. Server push: sì o no, e chi lo ospita?
4. Stesso repository (workspace con `core/`, `desktop/`, `ios/`, `android/`) o
   repository separati?
