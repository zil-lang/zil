import { property } from '../../core';
import type { RoamCase, Action } from '../../core';
export const spec = {
    contract: 'counter/1',
    init: () => 0,
    step: (value: number, action: Action) => value + (action.by as number),
    check: (value: number, observed: number) => property('counter-value', value === observed),
};
export const driver = {
    init: () => ({ value: 0 }),
    step: (real: { value: number }, action: Action) => { real.value += action.by as number; if (action.by === 13) real.value++; },
    observe: (real: { value: number }) => real.value,
};
export function generate(seed: number, steps: number): RoamCase { return { format: 'roam/1', contract: 'counter/1', seed, steps: Array.from({ length: steps }, (_, i) => ({ op: 'add', by: seed === 13 && i === steps - 1 ? 13 : 1 })) }; }
