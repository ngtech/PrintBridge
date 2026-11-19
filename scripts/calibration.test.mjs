import assert from 'node:assert/strict';
import test from 'node:test';
import { calibrationPdf } from './calibration.mjs';

test('generates exact label boxes and 50 mm horizontal references for both examples', () => {
  for (const [width, height] of [[68, 31], [100, 45]]) {
    const pdf = calibrationPdf(width, height, 'CAL-123').toString('ascii');
    const [, pageWidth, pageHeight] = pdf.match(/\/MediaBox \[0 0 ([\d.]+) ([\d.]+)\]/);
    assert.ok(Math.abs(Number(pageWidth) * 25.4 / 72 - width) < 0.00001);
    assert.ok(Math.abs(Number(pageHeight) * 25.4 / 72 - height) < 0.00001);
    const lines = [...pdf.matchAll(/([\d.]+) ([\d.]+) m ([\d.]+) ([\d.]+) l S/g)];
    assert.ok(lines.some(([, left, bottom, right, top]) => bottom === top
      && Math.abs((Number(right) - Number(left)) * 25.4 / 72 - 50) < 0.00001));
    assert.match(pdf, /\(CAL-123\)/);
    assert.match(pdf, /\(H 50 mm\)/);
    const xref = Number(pdf.match(/startxref\n(\d+)/)[1]);
    assert.equal(pdf.slice(xref, xref + 4), 'xref');
  }
});

test('rejects unsupported dimensions and invalid test IDs', () => {
  assert.throws(() => calibrationPdf(68, 0, 'test'), RangeError);
  assert.throws(() => calibrationPdf(68, 31, 'invalid(id)'));
});
