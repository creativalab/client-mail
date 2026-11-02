# Ferrowl — come guadagnarci

Stato: proposta, 2 novembre 2026. Prezzi e costi sono **stime** con le ipotesi
scritte accanto, da verificare prima di decidere.

## Il punto di partenza

Ferrowl è open source con licenza MIT: chiunque può prendere il codice,
modificarlo e ridistribuirlo, anche togliendo un eventuale blocco a pagamento.
Ne seguono tre regole:

1. **Si fanno pagare servizi, non funzioni.** Quello che gira su un server tuo
   (notifiche, sincronizzazione, backup) non si copia con un fork.
2. **L'app resta completa e gratuita.** Tutto ciò che oggi fa sul desktop
   rimane gratis: è la vetrina, e toglierlo rovinerebbe la fiducia.
3. **Niente che contraddica la privacy.** Nessun tracciamento delle aperture,
   nessuna pubblicità, nessuna vendita di dati: è il motivo per cui la gente
   sceglie Ferrowl.

## Cosa si può vendere

### 1. Ferrowl Plus — abbonamento personale

Il prodotto principale, legato all'arrivo dell'app per telefono (vedi
`piano-mobile.md`).

| Servizio | Perché vale un abbonamento |
|---|---|
| **Notifiche push immediate su iPhone e Android** | Su iOS sono impossibili senza un server che tenga la connessione IMAP al posto dell'utente: costo reale per te, valore chiaro per l'utente. |
| **Sincronizzazione tra dispositivi** | Account, firme, temi, regole e chiavi OpenPGP uguali su desktop e telefono, con cifratura end-to-end (il server vede solo dati cifrati). |
| **Invio programmato e promemoria** | «Invia domattina alle 9» e «ricordamelo se non risponde entro 3 giorni» funzionano anche a computer spento, perché li esegue il server. |
| **Backup cifrato della posta** (opzionale, a scaglioni) | Copia di sicurezza della casella, utile con provider dalle caselle piccole o in chiusura. |

Prezzo indicativo: **2,99 € al mese o 24 € all'anno** (in linea con i servizi
di posta «premium» per privati; da testare con due prezzi diversi).

### 2. Ferrowl per aziende

Per studi professionali e piccole aziende, venduto direttamente con fattura:

- configurazione centralizzata degli account (un file o un pannello) e
  installazione su molti PC;
- firme aziendali uniformi;
- supporto con tempi di risposta garantiti;
- in futuro: accesso Microsoft 365 / Google Workspace gestito
  dall'amministratore.

Prezzo indicativo: **4 € per utente al mese**, minimo 5 utenti, oppure un
canone annuo per studio. È la fonte più semplice da fatturare con la tua
partita IVA e la meno esposta alle commissioni degli store.

### 3. Supporto e servizi

- **Supporto prioritario** per privati: 49 € all'anno (risposta entro un giorno
  lavorativo, aiuto con la configurazione).
- **Installazione e migrazione** per aziende (spostare la posta da un altro
  programma, configurare OpenPGP): a giornata o a pacchetto.
- **Donazioni**: GitHub Sponsors o Liberapay. Rendono poco, ma costano zero e
  danno un segnale di sostegno.

### Da evitare

| Idea | Perché no |
|---|---|
| Far pagare l'accesso Microsoft/Google (OAuth2) | È una funzione di base: chi non può usare il proprio account non prova nemmeno l'app, e con un fork si aggira. |
| Tracciamento delle aperture | Contraddice tutto il messaggio sulla privacy. |
| Temi o icone a pagamento | Incassi minimi, si copiano subito. |
| Pubblicità nell'app | Rovina il prodotto e la fiducia. |

## Costi da coprire

Ipotesi: server su un provider europeo (es. Hetzner), prezzi indicativi 2026.

| Voce | Costo | Note |
|---|---|---|
| Server per le notifiche push | ~10–20 € al mese per alcune migliaia di account | Una connessione IMAP in attesa occupa pochissima memoria con un server scritto in Rust; si aggiungono server man mano. |
| Sincronizzazione | trascurabile | Pochi kilobyte cifrati per utente. |
| Backup della posta | ~5–15 € al mese per terabyte | Storage a oggetti; meglio venderlo a scaglioni di spazio. |
| APNs (Apple) e FCM (Google) | gratuiti | |
| Apple Developer Program | 99 USD all'anno | Necessario per l'app iPhone. |
| Google Play Console | 25 USD una tantum | |
| Firma del codice per Windows | ~10 $ al mese con Microsoft Trusted Signing, oppure ~100–300 € all'anno con un certificato tradizionale | Toglie l'avviso di SmartScreen; utile anche senza abbonamenti. |
| Notarizzazione macOS | inclusa nel programma Apple | Toglie l'avviso «sviluppatore non identificato». |

## Commissioni e incassi

| Canale | Commissione | Chi gestisce l'IVA |
|---|---|---|
| Acquisto in-app su iPhone | 15% (programma Small Business, sotto 1 milione l'anno; 15% anche sugli abbonamenti dopo il primo anno) | Apple |
| Acquisto in-app su Android | 15% sugli abbonamenti | Google |
| Vendita sul sito con Paddle o Lemon Squeezy | circa 5% + 0,50 $ a transazione | loro (sono il venditore verso il cliente: niente IVA da gestire per ogni paese UE) |
| Vendita sul sito con Stripe | circa 1,5% + 0,25 € (carte UE) | tu: per i privati UE serve il regime **OSS**, con l'IVA del paese del cliente |
| Aziende con fattura diretta | solo i costi del bonifico | tu, come di consueto (reverse charge per le aziende UE) |

Sugli store le regole per indirizzare gli utenti a pagare sul sito cambiano per
paese e nel tempo: per iniziare conviene usare l'acquisto in-app sul telefono e
la vendita sul sito per il desktop.

## Esempio di conto

Ipotesi prudente dopo un anno dall'uscita delle app per telefono: 20.000
utenti attivi, il 2% abbonati a Plus (400), metà annuali e metà mensili, metà
via store e metà dal sito.

| | Al mese |
|---|---|
| Incasso lordo (200 × 2 € + 200 × 2,99 €) | ~1.000 € |
| Commissioni (media ~10%) | −100 € |
| Server e servizi | −50 € |
| Quote sviluppatore e firma del codice (ripartite) | −40 € |
| **Margine indicativo** | **~800 €** |

Un singolo cliente aziendale da 20 utenti vale da solo circa 80 € al mese: per
i primi anni le aziende contano più degli abbonati privati.

## Aspetti legali da preparare

- **Termini di servizio** e **informativa privacy** aggiornati: con il server
  delle notifiche Ferrowl tratterebbe credenziali o token degli utenti, quindi
  serve una nomina come responsabile del trattamento (GDPR) per i clienti
  aziendali.
- **Diritto di recesso** per gli abbonamenti venduti a privati (14 giorni).
- Le credenziali sul server vanno **cifrate per utente** e cancellate quando
  l'abbonamento termina.

## Ordine consigliato

1. **Subito, a costo quasi zero:** donazioni (GitHub Sponsors), pagina
   «Supporto prioritario» sul sito, firma del codice per Windows e
   notarizzazione macOS per togliere gli avvisi.
2. **Insieme alle app per telefono:** Ferrowl Plus con notifiche push e
   sincronizzazione.
3. **Con i primi clienti aziendali:** configurazione centralizzata e contratto
   di supporto.
4. **Dopo:** invio programmato, promemoria e backup.

## Decisioni da prendere

1. Partire subito con donazioni e supporto prioritario, o aspettare le app per
   telefono?
2. Vendita sul sito: Paddle/Lemon Squeezy (più semplice, commissione più alta)
   o Stripe con OSS (più economico, più burocrazia)?
3. Ferrowl per aziende: offrirlo da subito ai clienti di Creativalab?
4. Prezzi da provare per Ferrowl Plus.
