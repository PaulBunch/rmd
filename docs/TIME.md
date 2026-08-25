# Time Parsing Syntax Reference

`rmd` features a rich, flexible, and human-friendly time-parsing language. It is designed to interpret natural, relative, and absolute date/time combinations without forcing a single rigid format.

---

## 1. Relative Durations

Relative durations specify a delay from the current time. 

* **Format:** `[+]<number><unit> [<number><unit> ...]`
* **Optional Prefix:** A leading `+` is completely optional (`+10m` and `10m` are identical).
* **Spacing:** You can use spaces or concatenate multiple segments (`1h 30m` and `1h30m` both work).
* **Units Supported:**
  * `s` — Seconds
  * `m` — Minutes (e.g., `25m`)
  * `h` — Hours (e.g., `2h`)
  * `d` — Days (e.g., `3d`)
  * `w` — Weeks (e.g., `1w` = 7 days)
  * `mo` — Months (e.g., `2mo` = 60 days)  *(Note: `mo` is used for months to prevent ambiguity with `m` for minutes)*
  * `y` — Years (e.g., `1y` = 365 days)

### Examples
```bash
rmd 45s Check the tea
rmd +15m Standup meeting
rmd 1h 30m Pizza is done
rmd 1y 2mo 3d Far-off event
```

---

## 2. Keywords and Weekday Aliases

`rmd` supports relative days and week-based targets.

### Day Keywords
* **Today:** `today`, `tod`
* **Tomorrow:** `tomorrow`, `tom`, `tmr`

### Weekday Names (Case-insensitive)
* **Monday:** `mon`, `monday`
* **Tuesday:** `tue`, `tuesday`
* **Wednesday:** `wed`, `wednesday`
* **Thursday:** `thu`, `thursday`
* **Friday:** `fri`, `friday`
* **Saturday:** `sat`, `saturday`
* **Sunday:** `sun`, `sunday`

---

## 3. Absolute Date Formats

Dates can be written using numeric layouts or using month names.

### Numeric Formats
* **ISO-like (separated by `-`):** `[YYYY-]MM-DD` (e.g., `2026-08-10`, `08-10`)
* **EU/RU-like (separated by `.` or `/`):** `DD.MM[.YYYY]` or `DD/MM[/YYYY]` (e.g., `10.08.2026`, `10/08`, `10.08.26`—two-digit years are expanded to `20XX`)

* **Year Omission & Auto-Rollover:** If the year is omitted, `rmd` assumes the current year. However, if the resulting target time has already passed for the current year, `rmd` automatically rolls the target over to the **next year** to keep it in the future.

### Month-Name Dates (Case-insensitive)
Dates can include full month names or standard 3-to-4 letter abbreviations:
* **Formats:** `DD Month [YYYY]`, `Month DD [YYYY]`, or abbreviations (e.g., `Nov`, `sept`, `oct`).
* **Examples:** `15 November 2026`, `Nov 15`, `November 15`.

---

## 4. Time Formats

Times can be specified in 24-hour or 12-hour AM/PM formats:

* **24-hour:** `HH:MM` or `HH:MM:SS` (e.g., `18:30`, `09:15:00`). Leading zeros are optional (e.g., `9:5` is parsed as `09:05`).
* **12-hour (AM/PM):** Supports space or direct concatenation, case-insensitive (e.g., `2pm`, `02:00 PM`, `2:30am`, `11:45 PM`).

### Time-only Specs & Auto-Rollover
If you specify only a time without a date (e.g., `15:00` or `2pm`):
* If the specified time is **still ahead today**, it triggers today.
* If the specified time has **already passed today**, `rmd` automatically rolls it over to **tomorrow** at the same time.

---

## 5. Joining Dates and Times

When specifying a full date and time, multiple separators and spacing styles are supported:

* **Spaced:** `tomorrow 15:00`, `fri 22:00`, `2026-08-10 09:00`
* **Concatenated:** `tomorrow15:00`, `mon09:00`
* **Separated by `@`:** `tomorrow@15:00`, `fri@22:00`, `2026-12-01@10:00`

> **Note on Shell Quotes:** Shell quotation marks around date/time or message are optional! Both `rmd tomorrow 15:00 Call mom` and `rmd "tomorrow 15:00" "Call mom"` work identically.

---

## 6. Configuration Defaults & Timezones

### The Default Time
When a date is specified without a time (e.g., `rmd tomorrow`, `rmd fri`, or `rmd 2026-08-10`), `rmd` uses a default fallback time configured in your settings.
* **Default Value:** `09:00`
* **How to customize:**
  ```bash
  rmd config default-time 11:00
  ```

### Timezone Context
All parsed absolute datetimes and keywords are evaluated relative to your **local system timezone** (e.g. from the OS environment). The daemon converts everything to absolute Unix timestamps in seconds for execution, but keeps them timezone-aware under the hood for clean logs and human-friendly list outputs.

---

## 7. Safety Validation

To prevent scheduling reminders that will never trigger, `rmd` rejects any target time that resolves to the past relative to the exact moment you run the command:

```text
Error: Target time is in the past: 2026-08-09 09:00:00 (current time: 2026-08-09 12:00:00)
```
