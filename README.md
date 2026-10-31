# Client Mail

Client di posta desktop scritto in Rust (GUI con [egui](https://github.com/emilk/egui)).

## Funzioni

- Account IMAP (TLS, porta 993) + SMTP (465 TLS o 587 STARTTLS)
- Elenco cartelle, ultimi 200 messaggi per cartella, ricerca per mittente/oggetto
- Lettura messaggi (testo; l'HTML viene convertito in testo)
- Rispondi, Inoltra, Scrivi nuovo (con Cc)
- Elimina (con conferma)
- Salvataggio allegati nella cartella Scaricati
- Aggiornamento automatico ogni 2 minuti

## Avvio

```bash
cargo run --release
```

Al primo avvio si apre la finestra **Account**. La configurazione è salvata in
`~/.config/client-mail/config.toml` con permessi `600` (password in chiaro nel file).

Per Gmail serve una «password per le app» (con verifica in due passaggi attiva).

## Test

```bash
cargo test -- --ignored   # prova connessione TLS reale verso imap.gmail.com
```

## Limiti attuali

- Un solo account
- I messaggi inviati non vengono copiati nella cartella «Inviata» (molti provider lo fanno da soli, es. Gmail)
- Nomi cartella con caratteri accentati mostrati in UTF-7 modificato
- Solo testo semplice in composizione, niente allegati in uscita
