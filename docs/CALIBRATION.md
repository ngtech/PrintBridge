# Calibration and Epson acceptance

## Generate

From the repository root, use Node.js to generate dimensioned test pages:

```text
node scripts/calibration.mjs 68 31 calibration-68x31.pdf SMALL-01
node scripts/calibration.mjs 100 45 calibration-100x45.pdf LARGE-01
```

The PDF page is the exact size requested, with a 0.5 mm inset boundary, registration cross centres 4 mm from the four edges, and a test ID. Measure reference lines between the centres of the end ticks: horizontal **50 mm** on both example labels; vertical **20 mm** on 68 × 31 and **34 mm** on 100 × 45. For other sizes the generator shortens lines as needed and labels their nominal length. It accepts pages from 20 to 1000 mm per axis. Host the generated PDF at an approved HTTPS origin serving `application/pdf` and submit it through the same browser/profile path as actual labels.

## Measure on Windows with the actual printer

1. Record the Epson model suffix, driver version, queue, Windows/Sumatra/browser versions, roll dimensions and detection method. Select the exact custom layout in the Epson driver as the signed-in operator. Disable scaling, fit, N-up, mirroring and driver rotation. Load the matching roll and use the corresponding profile.
2. Submit one test ID with `autoRotate: false`. Confirm the correct queue, no application/dialog, one label, expected orientation and colour, and a terminal status. Repeat for both layouts and confirm their particular roll/queue pairings. Verify exact page dimension errors by deliberately using the other profile and driver-media errors by using a mismatched test queue. Verify these failures enqueue nothing in Windows.
3. Measure the horizontal reference with a calibrated ruler or caliper: it must measure **50 mm**. Record the actual reading, instrument resolution and measurement uncertainty. For the pilot, flag any reading outside 49.8–50.2 mm; don't treat that interval as a product tolerance. Measure the vertical reference too and record any directional error. This checks scale, not position.
4. Measure the centres of the registration crosses from the die-cut label edges; nominal inset is 4 mm. Compare left/right and leading/trailing placement to the intended layout and record deviations and clipping. The 0.5 mm inset boundary can be partly outside the printable area; if a 4 mm cross is clipped, record it and verify the artwork’s actual safety margin before approval. Repeat at least three labels per size and record skew or feed drift. Compare representative Stockman artwork for legibility and correct placement, including barcodes and areas near the edge.
5. Submit two copies with one ID, repeat identical submission, and verify the physical count is two, not four. Test missing SumatraPDF, a stopped spooler or uninstalled queue, a blocked origin, and a cancelled queued job. Restart the agent and repeat the successful job ID; it must not print again. Preserve Windows queue observations and physical measurements separately from agent status.

## Interpret the result

| Concept | What to check |
| --- | --- |
| PDF page dimensions | TCPDF `MediaBox` in points (72 pt = 25.4 mm), converted to width/height in mm. Wrong boxes fail before printing. |
| Driver media dimensions | The form/layout, orientation, and media type reported by the configured Windows queue. Wrong dimensions fail preflight. |
| Printable area | Driver/printer hardware margins can clip marks even if both page and media dimensions match. |
| Scaling | Changes a measured length. If 50 mm does not measure 50 mm, look for driver scaling/resolution or the wrong media size; do not “fix” it with an offset. |
| Physical registration | Position/feeding relative to the die cut. Correct reference lengths and shifted marks indicate a registration/origin issue, not scaling. |

SumatraPDF 3.6.1 has no supported arbitrary X/Y print-offset flag for this path. Use a dedicated queue's alignment/registration controls where the exact Epson driver exposes them, or deliberately translate artwork at PDF generation while keeping the same page boxes. This agent never modifies a PDF. Actual placement and accuracy are unverified until someone performs these measurements.
