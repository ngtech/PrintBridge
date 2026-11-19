import { randomUUID } from 'node:crypto';
import { writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

const points = (mm) => (mm * 72 / 25.4).toFixed(5);

export function calibrationPdf(width, height, testId) {
  if (![width, height].every((value) => Number.isFinite(value) && value >= 20 && value <= 1000)) {
    throw new RangeError('Calibration dimensions must be between 20 and 1000 mm.');
  }
  if (!/^[A-Za-z0-9_-]{1,16}$/.test(testId)) throw new Error('Test ID must be 1–16 ASCII letters, numbers, hyphens or underscores.');

  const horizontal = Math.min(50, width - 18);
  const vertical = Math.min(50, height - 11);
  const commands = ['0 G 0 g 0.45 w'];
  const line = (left, bottom, right, top) => {
    commands.push(`${points(left)} ${points(bottom)} m ${points(right)} ${points(top)} l S`);
  };
  const text = (left, bottom, size, value) => {
    commands.push(`BT /F1 ${size} Tf ${points(left)} ${points(bottom)} Td (${value}) Tj ET`);
  };

  commands.push(`${points(0.5)} ${points(0.5)} ${points(width - 1)} ${points(height - 1)} re S`);
  for (const left of [4, width - 4]) {
    for (const bottom of [4, height - 4]) {
      line(left - 1.5, bottom, left + 1.5, bottom);
      line(left, bottom - 1.5, left, bottom + 1.5);
    }
  }
  const horizontalStart = 7;
  const horizontalY = 7;
  line(horizontalStart, horizontalY, horizontalStart + horizontal, horizontalY);
  for (const left of [horizontalStart, horizontalStart + horizontal]) line(left, 6, left, 8);
  text(7, 9, 5, `H ${Number(horizontal.toFixed(2))} mm`);

  const verticalX = width - 7.5;
  line(verticalX, 5, verticalX, 5 + vertical);
  for (const bottom of [5, 5 + vertical]) line(verticalX - 1, bottom, verticalX + 1, bottom);
  const verticalText = `V ${Number(vertical.toFixed(2))} mm`;
  commands.push(`BT /F1 5 Tf 0 1 -1 0 ${points(verticalX + 2)} ${points(7)} Tm (${verticalText}) Tj ET`);

  const titleSize = width < 40 ? 4 : 6;
  text(6.5, height - 5, titleSize, width < 40 ? 'CAL' : `CAL ${width} x ${height} mm`);
  text(6.5, height - 8, width < 40 ? 3.5 : 5.5, testId);

  const stream = commands.join('\n') + '\n';
  const objects = [
    '<< /Type /Catalog /Pages 2 0 R >>',
    '<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
    `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 ${points(width)} ${points(height)}] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>`,
    '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
    `<< /Length ${Buffer.byteLength(stream)} >>\nstream\n${stream}endstream`,
  ];
  let pdf = '%PDF-1.4\n';
  const offsets = [];
  for (const [index, object] of objects.entries()) {
    offsets.push(Buffer.byteLength(pdf));
    pdf += `${index + 1} 0 obj\n${object}\nendobj\n`;
  }
  const xref = Buffer.byteLength(pdf);
  pdf += 'xref\n0 6\n0000000000 65535 f \n';
  for (const offset of offsets) pdf += `${String(offset).padStart(10, '0')} 00000 n \n`;
  return Buffer.from(pdf + `trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`);
}

if (process.argv[1] && pathToFileURL(process.argv[1]).href === import.meta.url) {
  const [width, height, path, testId = randomUUID().slice(0, 8)] = process.argv.slice(2);
  try {
    if (!path) throw new Error('Usage: node scripts/calibration.mjs <width-mm> <height-mm> <output.pdf> [test-id]');
    writeFileSync(path, calibrationPdf(Number(width), Number(height), testId), { flag: 'wx' });
    console.log(`Wrote ${path} (${testId})`);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
