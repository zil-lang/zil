import {test, expect} from 'bun:test';
import {same, diff, loc} from '../check';
test('roam-check-01 errors compare their non-enumerable semantic fields', () => {
    expect(same(new Error('first'), new Error('second'))).toBe(false);
    expect(same(new Error('first', {cause: 'one'}), new Error('first', {cause: 'two'}))).toBe(false);
    expect(same(new Error('first'), new Error('first'))).toBe(true);
});
test('roam-check-02 ArrayBuffer and DataView compare visible bytes', () => {
    const a = Uint8Array.from([1, 2]), b = Uint8Array.from([1, 3]);
    expect(same(a.buffer, b.buffer)).toBe(false);
    expect(diff(new DataView(a.buffer, 1, 1), new DataView(b.buffer, 1, 1))?.p).toBe('[0]');
    expect(same(new DataView(a.buffer, 0, 1), new DataView(b.buffer, 0, 1))).toBe(true);
});
test('roam-check-03 moved assertions report the caller location', () => {
    expect(loc('Error\n at ok (D:/res/cc/zit/roam/check.ts:80:10)\n at example (D:/res/cc/zit/ui/example.test.ts:42:3)')).toBe('example.test.ts:42');
});
