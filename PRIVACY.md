# Polityka prywatności - Tablica Ofert

[English version below](#privacy-policy---tablica-ofert)

Stan na 2.11.2026. Tablica Ofert to aplikacja na komputer, bez kont i bez własnego serwera. Autor aplikacji nie dostaje żadnych Twoich danych.

## Jakie dane i gdzie

- **Oferty, profile „czego szukam”, doświadczenie, notatki, terminy, kontakty** - tylko na Twoim komputerze, w zaszyfrowanej bazie (SQLCipher) w folderze `%APPDATA%\pl.prywatne.tablicaofert\`. Klucz bazy leży w Menedżerze poświadczeń Windows, nie w pliku.
- **Klucze API, hasło do skrzynki z alertami** - tylko w Menedżerze poświadczeń Windows. Nie trafiają do bazy, kopii, eksportu CSV, logów ani do AI. Na ekranie widać tylko 4 ostatnie znaki.
- **Kopie zapasowe** - 5 ostatnich kopii bazy w podfolderze `kopie` (zaszyfrowane tym samym kluczem). „Zapisz kopię jako…” tworzy plik zabezpieczony hasłem, które wybierasz.

## Z czym aplikacja się łączy

Tylko z tym, co włączysz w ustawieniach:

- **Adzuna** (agregator ofert) - wysyłane są słowa kluczowe i miasto z profilu oraz Twój klucz Adzuna.
- **Remotive** (praca zdalna) - wysyłane są słowa kluczowe z profilu.
- **Twoja skrzynka z alertami** (IMAP) - aplikacja tylko czyta maile; niczego nie usuwa, nie oznacza jako przeczytane i nie wysyła.
- **Wybrany dostawca AI** (Gemini, Groq, OpenRouter, Claude, OpenAI) - wysyłana jest treść oferty, opisy profili, oczekiwania finansowe i doświadczenie. Nigdy klucze, hasła ani notatki. Przy Ollamie AI działa na Twoim komputerze i nic nie wychodzi na zewnątrz. Dane w usłudze AI podlegają polityce prywatności tej usługi.
- **Strony ofert otwarte przez Ciebie** w oknie podglądu.

Aplikacja nie ma analityki, reklam ani żadnych „telemetrycznych” połączeń.

## Twoje prawa i usuwanie danych

Wszystkie dane są u Ciebie. Ustawienia → Dane → „Usuń wszystkie dane” kasuje bazę, kopie, klucze z Menedżera poświadczeń i zadanie w Harmonogramie zadań. Odinstalowanie usuwa zadanie w Harmonogramie zadań.

---

# Privacy policy - Tablica Ofert

As of 2 November 2026. Tablica Ofert is a desktop app with no accounts and no server of its own. The author of the app receives none of your data.

## What data and where

- **Offers, "what I'm looking for" profiles, experience, notes, dates, contacts** - only on your computer, in an encrypted database (SQLCipher) in `%APPDATA%\pl.prywatne.tablicaofert\`. The database key is kept in Windows Credential Manager, not in a file.
- **API keys, the alert mailbox password** - only in Windows Credential Manager. They never go to the database, copies, CSV export, logs or AI. Only the last 4 characters are shown.
- **Backups** - the 5 newest copies of the database in the `kopie` subfolder (encrypted with the same key). "Save a copy as…" creates a file protected with a password you choose.

## What the app connects to

Only what you turn on in the settings:

- **Adzuna** (job aggregator) - profile keywords and city, and your Adzuna key.
- **Remotive** (remote jobs) - profile keywords.
- **Your alert mailbox** (IMAP) - read only; nothing is deleted, marked as read or sent.
- **The AI provider you choose** (Gemini, Groq, OpenRouter, Claude, OpenAI) - the offer text, profile descriptions, salary expectations and experience. Never keys, passwords or notes. With Ollama, AI runs on your computer and nothing leaves it. Data at an AI service is covered by that service's privacy policy.
- **Offer pages you open yourself** in the preview window.

The app has no analytics, no ads and no telemetry.

## Your rights and deleting data

All data is with you. Settings → Data → "Delete all data" removes the database, the backups, the keys in Credential Manager and the Task Scheduler task. Uninstalling removes the Task Scheduler task.
