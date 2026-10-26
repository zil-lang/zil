import {test, expect} from 'bun:test';
import {createClock} from '../clock';
const host = () => ({setTimeout() {}, clearTimeout() {}, requestAnimationFrame: undefined, cancelAnimationFrame: undefined}) as any;
test('roam-clock-01 time is monotonic and disposed clocks reject use', async () => {
    const g = host(), c = createClock(g), values: number[] = [];
    g.setTimeout(() => values.push(c.now()), -10);
    await c.adv(1); expect(values).toEqual([0]); expect(c.now()).toBe(1);
    await expect(c.adv(-1)).rejects.toThrow(); await expect(c.adv(NaN)).rejects.toThrow();
    c.off(); await expect(c.adv(1)).rejects.toThrow('disposed');
});
test('roam-clock-02 independently owned hosts restore their own timers', () => {
    const a = host(), b = host(), original = a.setTimeout, ca = createClock(a), cb = createClock(b);
    expect(() => createClock(a)).toThrow('already owned');
    ca.off(); expect(a.setTimeout).toBe(original); cb.off();
    createClock(a).off();
});
test('roam-clock-03 self-rescheduling timer reports unfinished work instead of jumping time', async () => {
    const g = host(), c = createClock(g);
    const again = () => g.setTimeout(again, 0); again();
    try {await expect(c.adv(10)).rejects.toThrow('task budget'); expect(c.now()).toBe(0);}
    finally {c.off();}
}, 15000);
test('roam-clock-04 rejected installation leaves descriptors and ownership untouched', () => {
    const g=host(),original=g.setTimeout;
    Object.defineProperty(g,'clearTimeout',{value:g.clearTimeout,writable:false,configurable:true});
    const before=Object.getOwnPropertyDescriptors(g);
    expect(()=>createClock(g)).toThrow('not writable');
    expect(g.setTimeout).toBe(original); expect(Object.getOwnPropertyDescriptors(g)).toEqual(before);
    Object.defineProperty(g,'clearTimeout',{writable:true});
    const clock=createClock(g); clock.off(); expect(g.setTimeout).toBe(original);
});
test('roam-clock-05 disposal restores absent properties rather than adding undefined keys', () => {
    const g={setTimeout(){},clearTimeout(){}},before=Object.getOwnPropertyDescriptors(g);
    createClock(g).off(); expect(Object.getOwnPropertyDescriptors(g)).toEqual(before);
});
