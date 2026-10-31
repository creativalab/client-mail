# Ferrowl — pacchetto di consegna per Claude Code

Ferrowl è un client di posta desktop scritto in Rust: veloce, con indice di ricerca locale, OpenPGP integrato e sei temi colore. Questa cartella contiene il design e le specifiche per costruirlo.

## Contenuto

```
ferrowl-handoff/
├── README.md              ← questo file: specifiche e istruzioni
├── design/
│   ├── Main.dc.html       ← finestra principale (cartelle · elenco · lettura · pannello Aspetto)
│   ├── Settings.dc.html   ← impostazioni, 9 sezioni
│   ├── Compose.dc.html    ← finestra "Nuovo messaggio"
│   ├── Icon.dc.html       ← tavola dell'icona e palette
│   ├── canvas.json        ← disposizione delle tavole (solo riferimento)
│   └── themes.json        ← token colore dei 6 temi, tipografia, misure
└── icons/
    ├── ferrowl.svg        ← icona master 512
    ├── ferrowl-64.svg / -32.svg / -16.svg   ← versioni semplificate per misure piccole
    ├── tray-light.svg / tray-dark.svg / tray-accent.svg  ← icona a tratto per la barra di sistema
    ├── ferrowl.ico        ← Windows (16/32/48/256)
    └── png/               ← 16 … 1024 px, tray 32 px
```

### Come leggere i file `.dc.html`
Sono mockup HTML con un piccolo motore di template. Vanno usati come **riferimento visivo e di comportamento**, non come codice da riusare:
- gli stili inline danno colori, spaziature, raggi e dimensioni esatte;
- `{{t.xxx}}` è un token del tema (vedi `themes.json`);
- `<sc-for>` è un elenco, `<sc-if>` è un blocco condizionale;
- la classe `Component` in fondo (`renderVals()`) contiene i dati di esempio e la logica di stato (selezione del messaggio, cartella attiva, tema, sezioni delle impostazioni, interruttori).

Tutti i contenuti (mittenti, email, importi) sono **inventati**. I valori tra parentesi quadre, come `[IMPRONTA DELLA CHIAVE]`, sono segnaposto.

---

## Stack proposto
Questa è una proposta: verifica le versioni aggiornate dei crate prima di fissarle.

| Ambito | Crate suggeriti |
|---|---|
| GUI | **iced** (Rust puro, architettura Elm, temi personalizzabili). Alternativa: Slint |
| Runtime asincrono | tokio |
| IMAP | async-imap (con IDLE) |
| SMTP | lettre |
| JMAP | jmap-client |
| Parsing / creazione MIME | mail-parser, mail-builder |
| Cache locale | SQLite (rusqlite o sqlx) |
| Ricerca full-text | tantivy |
| OpenPGP | sequoia-openpgp |
| Password | keyring (portachiavi di sistema) |
| HTML delle email | ammonia (sanitizzazione) e niente risorse remote di default |
| Notifiche | notify-rust |
| Tray | tray-icon |
| Configurazione | serde + toml, percorsi con directories |
| Log | tracing |

### Struttura consigliata (workspace Cargo)
```
crates/
  ferrowl-core/     modelli (Account, Folder, Message, Thread, Label, Rule), errori
  ferrowl-store/    SQLite + Maildir, indice tantivy
  ferrowl-proto/    IMAP / JMAP / SMTP dietro un trait comune `MailBackend`
  ferrowl-crypto/   OpenPGP, verifica DKIM/firme
  ferrowl-rules/    filtri (compatibili con Sieve)
  ferrowl-ui/       app iced: viste, temi, scorciatoie
  ferrowl/          binario
```

---

## Schermate

### 1. Finestra principale (`Main.dc.html`, riferimento 1440×900, layout fluido)
- **Barra superiore**: logo e nome, ricerca globale con operatori (`from:`, `has:attachment`, `label:`) e scorciatoia `Ctrl K`, pulsante Sincronizza, Impostazioni (apre il pannello rapido Aspetto) e avatar dell'account.
- **Barra laterale** (min 220 px): pulsante **Scrivi**, cartelle (Posta in arrivo, Speciali, Inviati, Bozze, Archivio, Spam, Cestino) con contatori, **Etichette** colorate, **Account** con stato (connesso o offline) e riquadro di stato della sincronizzazione e dell'indice.
- **Elenco messaggi** (min 340 px): filtri Tutti / Non letti / Allegati. Ogni riga mostra un pallino se il messaggio non è letto, il mittente (in grassetto se non letto), l'icona allegato, l'orario, l'oggetto, una riga di anteprima e un'etichetta. La riga selezionata ha lo sfondo `soft` e una barra di 3 px in `accent` a sinistra.
- **Pannello di lettura**: barra con Rispondi, Inoltra, Archivia, Elimina, Posticipa e un badge di sicurezza ("Firmato · DKIM ok" / "Cifrato · OpenPGP"). Poi l'oggetto (26 px), mittente con avatar, data, pulsante preferiti, corpo (testo largo al massimo 820 px), schede degli allegati e un campo di risposta rapida (`R`).
- **Pannello Aspetto** (popover dall'ingranaggio): griglia con i 6 temi, opzione "segui il tema scuro di sistema" e link "Tutte le impostazioni".
- Sotto i 1000 px circa le colonne vanno una sotto l'altra.

### 2. Impostazioni (`Settings.dc.html`)
Barra superiore con "Torna alla posta" e ricerca nelle impostazioni. A sinistra l'elenco delle sezioni, a destra il contenuto largo al massimo 760 px, organizzato in card con righe "titolo + descrizione + controllo".
1. **Generale**: lingua, avvio all'accesso, app di posta predefinita (mailto:), quando segnare come letto, raggruppa in conversazioni.
2. **Account**: elenco degli account con stato e "Aggiungi account". Dettagli: nome visualizzato, email, server IMAP e SMTP, autenticazione (password nel portachiavi, OAuth2, certificato), sincronizzazione (IDLE, ogni 5 o 15 minuti, manuale), Verifica connessione, Cartelle e alias, Rimuovi.
3. **Aspetto**: 6 temi, densità (Compatta, Comoda, Ampia), dimensione del testo (da 12 a 20), carattere di lettura, anteprima del testo nell'elenco.
4. **Notifiche**: notifiche desktop, solo contatti VIP, suono, contatore sull'icona, non disturbare in una fascia oraria.
5. **Composizione**: formato predefinito (Testo, Markdown, HTML), annulla invio (0/5/10/30 s), posizione della risposta, controllo ortografico, firma per account.
6. **Sicurezza e privacy**: chiavi OpenPGP (importa o genera, Ed25519), firma sempre, cifra quando possibile, blocca immagini remote, avvisa sui link sospetti, blocco dell'app per inattività.
7. **Filtri e regole**: regole ordinate "Se … allora …" con interruttore, Nuova regola.
8. **Scorciatoie**: schema (Ferrowl, Vim, Gmail) e tabella modificabile.
9. **Avanzate**: statistiche (messaggi indicizzati, spazio usato, cache allegati), cartella dati, ricostruisci indice, livello di log, accelerazione grafica, esporta e importa le impostazioni in TOML, Ripristina (zona rossa).

### 3. Nuovo messaggio (`Compose.dc.html`, 760×640)
Mittente (scelta dell'account), destinatari come chip, Cc/Ccn, oggetto, barra di formattazione (grassetto, corsivo, sottolineato, elenco, link, codice) con il formato indicato, corpo del messaggio, allegati, **Invia** (`Ctrl ↵`), programma l'invio, allega, interruttore **Firma e cifra (OpenPGP)**, elimina la bozza, indicatore "Bozza salvata".
Nel mockup è disegnata solo in Ruggine: nell'app deve usare il tema attivo.

---

## Temi
`design/themes.json` contiene tutti i token. Il default è **Ruggine**. Il cambio di tema è immediato e vale per tutte le finestre e per il logo nell'intestazione. "Notte" è il tema scuro: sopra `accent` usa testo scuro (`onAccent`). Il testo rispetta un contrasto minimo di 4.5:1. Le aree cliccabili sono di almeno 44 px.

## Scorciatoie predefinite
| Azione | Tasto |
|---|---|
| Nuovo messaggio | C |
| Rispondi / Rispondi a tutti | R / Shift R |
| Inoltra | F |
| Archivia | E |
| Elimina | Canc |
| Messaggio successivo / precedente | J / K |
| Cerca | Ctrl K |
| Invia | Ctrl ↵ |

## Icona
Un gufo a forma di busta: le orecchie sono gli angoli alzati, la chiusura fa da sopracciglia, gli occhi sono dadi esagonali di ferro e il becco cade sulla punta della busta. Sotto i 32 px usa i file `-32`/`-16`, che sono semplificati. Per la barra di sistema usa `tray-*`.
Palette: Ruggine `#A63A0C` · Ferro `#2A1A12` · Carta `#FBF3EA` · Brace `#E07A4A`.

---

## Roadmap suggerita
1. **Scheletro**: workspace, finestra iced con le tre colonne, temi da `themes.json`, dati finti.
2. **Lettura**: un account IMAP, sincronizzazione delle cartelle, cache SQLite, elenco e lettura (HTML sanitizzato, immagini remote bloccate).
3. **Invio**: composizione e SMTP, bozze, annulla invio.
4. **Ricerca**: indice tantivy con gli operatori.
5. **Sicurezza**: OpenPGP (firma, cifratura, verifica), badge DKIM.
6. **Resto**: più account, JMAP, regole, notifiche e tray, impostazioni complete, scorciatoie.

## Primo messaggio da dare a Claude Code
> Leggi `README.md` e i file in `design/`. Crea il workspace Cargo di Ferrowl come descritto e implementa il passo 1 della roadmap: finestra principale in iced fedele a `design/Main.dc.html`, con i 6 temi caricati da `design/themes.json` e selezionabili dal pannello Aspetto, usando dati finti. Usa le icone in `icons/`. Prima di scrivere codice proponimi le versioni dei crate e la struttura dei moduli.
