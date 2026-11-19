# Printer profiles

A profile is a local, named mapping from an exact PDF page size to a Windows queue and media setting. The browser can list or read profiles, but cannot create or modify them or specify a queue directly.

1. Start the tray app once to generate `label-settings.json` in `%LOCALAPPDATA%\PrintBridge`.
2. Open **Label setup and pairing** to see its path. Edit it with a text editor. Copy the [example file](../examples/labels/label-settings.example.json) if useful, but replace every illustrative hostname and queue name. Keep only the origins you use.
3. Add or edit a key under `profiles`. Save and restart the tray app. The same file is loaded on every restart. Invalid configuration disables the listener and reports a setup error in the tray window.
4. Pair and run `printers.list` to copy the **exact** Windows queue name if unsure. Profile names are case-sensitive and use 1–64 ASCII letters, digits, underscores or hyphens.

| Field | Meaning |
| --- | --- |
| `printer` | Exact queue name as discovered under the signed-in account. |
| `widthMm`, `heightMm` | PDF width and height, in that order, each 10–1000 mm. The actual driver/device can support a narrower range. Each PDF page must match within ±0.10 mm. No mismatches are resized. |
| `scale` | Only `none` is supported and is the default. Disable separate driver-level scaling too. |
| `autoRotate` | Defaults to false. When true, 3.6.1 can rotate wide pages by 90°; the preflight then requires the swapped driver dimensions. It never relaxes the PDF geometry check. Use `false` for the pilot. |
| `copies` | Default copy count (1–20, defaults to 1). A request may override it within the same bounds. Counts above one require driver support; a job is capped at 500 output pages. |
| `paperKind` | Optional positive numeric Windows media ID. When omitted, the queue's current DEVMODE media ID is used. A form name or width/height is not a valid substitute. Use an explicit ID only if the driver exposes and honors it. |

Register/select a *matching* custom layout in the driver. The agent does not install driver forms or change queue defaults. Some custom Epson settings may live in the driver’s private data; if selecting by ID alone doesn't produce the expected dimensions, configure one queue per layout using the same physical printer where the driver supports independently saved defaults. If no positive media ID is exposed or the driver rejects it, preflight fails; this driver configuration needs hardware investigation before use. Never guess a media ID from the label’s dimensions. Feeding a different roll into an otherwise correctly configured queue remains an operator error.

In Stockman, map each product label template to a configured profile name (for example, 68 × 31 → `meadows-small`). Use explicit size-and-product/profile selection when two profiles share dimensions. Don't pick a profile by nearest size, transpose axes, or default to the system printer. The lookup verifies the chosen profile against every PDF page; it is not a media search or conversion feature.
