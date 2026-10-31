#!/usr/bin/env python3
"""Lossless event-support factoring of already-materialized canonical relations.

This is a consumer-side experiment, never a replacement for the solver that
produced the input. Pairwise independence is not a factorization certificate.
For every split, projections guarantee containment; equal product cardinality
then proves equality of the finite relations without enumerating the product.
"""

import argparse
from collections import Counter
from itertools import combinations, product
from pathlib import Path
import hashlib
import json


class FactorRelation:
    def __init__(self, fields, words, supports=(), max_search_projections=512):
        self.names = tuple(sorted(fields, key=lambda name: (fields[name][0], name)))
        self.index = {name: i for i, name in enumerate(self.names)}
        self.fields = tuple(tuple(fields[name]) for name in self.names)
        self.counters = Counter()
        self.max_search_projections = max_search_projections
        self.search_exhausted = False
        self.search_projections = 0
        self.cache = {}
        self.query_cache = {}
        self.allowed = 0
        for bit, width in self.fields:
            if not isinstance(bit, int) or not isinstance(width, int) or bit < 0 or width < 1:
                raise ValueError('Every field must have a nonnegative bit and positive width')
            mask = ((1 << width) - 1) << bit
            if self.allowed & mask:
                raise ValueError('Overlapping fields')
            self.allowed |= mask
        minimum_words = (self.allowed.bit_length() + 63) // 64
        self.word_count = len(words[0]) if words else minimum_words
        if self.word_count < minimum_words:
            raise ValueError('Words cannot represent the declared fields')
        self.original = set()
        decoded = set()
        for row in words:
            value = self.unpack(row)
            self.original.add(tuple(row))
            decoded.add(tuple((value >> bit) & ((1 << width) - 1) for bit, width in self.fields))
            self.counters['decode_rows'] += 1
            self.counters['decode_fields'] += len(self.fields)
        self.rows = frozenset(decoded)
        if len(self.rows) != len(self.original):
            raise AssertionError('Field encoding failed to preserve canonical row identity')
        self.duplicate_input_rows = len(words) - len(self.original)
        self.empty = not self.rows
        self.initial_components = []
        self.merge_reasons = []
        self.factors = []
        self.factor_for_field = {}
        if self.empty:
            self.certificate = {'relation_rows': 0, 'empty_relation': True}
            self.construction_counters = dict(self.counters)
            return
        # Constant fields are exact singleton factors regardless of event support.
        constants = []
        variable = []
        first = next(iter(self.rows))
        for i in range(len(self.names)):
            self.counters['constant_field_tests'] += 1
            values = set()
            for row in self.rows:
                self.counters['constant_cells_scanned'] += 1
                values.add(row[i])
            if len(values) == 1:
                constants.append((i,))
                self.cache[(i,)] = frozenset({(first[i],)})
            else:
                variable.append(i)
        parents = {i: i for i in variable}

        def find(i):
            while parents[i] != i:
                parents[i] = parents[parents[i]]
                i = parents[i]
            return i

        for support in supports:
            self.counters['event_support_sets'] += 1
            indices = []
            for name in support:
                self.counters['event_support_fields'] += 1
                if name not in self.index:
                    raise ValueError('Unknown support field: ' + name)
                i = self.index[name]
                if i in parents:
                    indices.append(i)
            for i in indices[1:]:
                self.counters['event_support_unions'] += 1
                parents[find(i)] = find(indices[0])
        groups = {}
        for i in variable:
            groups.setdefault(find(i), []).append(i)
        components = [tuple(group) for group in groups.values()]
        self.initial_components = [list(group) for group in components]
        self.cache[tuple(range(len(self.names)))] = self.rows
        if variable:
            # This single exact projection is also the always-safe fallback.
            self.project_original(tuple(variable), budgeted=False)
            components = self.refine(components)
        # One singleton tuple stores all constant fields without per-field set
        # and metadata overhead; it contributes cardinality one.
        constant_group = tuple(group[0] for group in constants)
        factors = ([constant_group] if constant_group else []) + components
        for indices in sorted(factors):
            values = self.project_original(indices, budgeted=False)
            self.factors.append((indices, values))
        self.factor_for_field = {field: (number, offset) for number, (indices, _) in enumerate(self.factors)
                                 for offset, field in enumerate(indices)}
        cardinality = 1
        for _, values in self.factors:
            cardinality *= len(values)
        if cardinality != len(self.rows):
            raise AssertionError('Final joint cardinality certificate failed')
        self.certificate = {
            'relation_rows': len(self.rows),
            'product_cardinality': cardinality,
            'projection_containment': 'Every factor was projected directly from the input relation',
            'exact_equality': True,
        }
        self.construction_counters = dict(self.counters)

    def unpack(self, row):
        if len(row) != self.word_count:
            raise ValueError('Nonuniform canonical word count')
        value = 0
        for i, word in enumerate(row):
            if not isinstance(word, int) or isinstance(word, bool) or not 0 <= word < 1 << 64:
                raise ValueError('Canonical words must be u64 integers')
            value |= word << (i * 64)
        if value & ~self.allowed:
            raise ValueError('Nonzero bits outside declared fields; refusing a lossy factorization')
        return value

    def pack(self, values):
        packed = 0
        for value, (bit, width) in zip(values, self.fields, strict=True):
            if not 0 <= value < 1 << width:
                raise ValueError('Field value out of range')
            packed |= value << bit
        return tuple((packed >> (i * 64)) & ((1 << 64) - 1) for i in range(self.word_count))

    def project_original(self, indices, budgeted=True):
        indices = tuple(sorted(indices))
        if indices in self.cache:
            self.counters['original_projection_cache_hits'] += 1
            return self.cache[indices]
        if budgeted:
            if self.search_projections >= self.max_search_projections:
                self.search_exhausted = True
                return None
            self.search_projections += 1
        self.counters['original_projection_builds'] += 1
        self.counters['original_projection_rows_scanned'] += len(self.rows)
        self.counters['original_projection_cells_scanned'] += len(self.rows) * len(indices)
        result = frozenset(tuple(row[i] for i in indices) for row in self.rows)
        self.cache[indices] = result
        return result

    def refine(self, groups):
        whole = tuple(sorted(i for group in groups for i in group))
        relation = self.project_original(whole, budgeted=False)
        projections = [self.project_original(group) for group in groups]
        if any(values is None for values in projections):
            self.merge_reasons.append({'fields': list(whole), 'reason': 'search_budget_safe_merge'})
            return [whole]
        cardinality = 1
        for values in projections:
            cardinality *= len(values)
        self.counters['joint_cardinality_tests'] += 1
        if cardinality == len(relation):
            return groups
        if len(groups) <= 1:
            return [whole]
        # Try complete bipartitions, never merely pairwise independence. Search
        # is bounded; a coarse factor is always exact when the budget runs out.
        for count in range(1, len(groups)):
            for chosen in combinations(range(len(groups)), count):
                if count * 2 == len(groups) and 0 not in chosen:
                    continue  # omit the equal-size complementary duplicate
                selected = set(chosen)
                left_groups = [g for i, g in enumerate(groups) if i in selected]
                right_groups = [g for i, g in enumerate(groups) if i not in selected]
                left = tuple(sorted(i for g in left_groups for i in g))
                right = tuple(sorted(i for g in right_groups for i in g))
                a, b = self.project_original(left), self.project_original(right)
                if a is None or b is None:
                    self.merge_reasons.append({'fields': list(whole), 'reason': 'search_budget_safe_merge'})
                    return [whole]
                self.counters['bipartition_cardinality_tests'] += 1
                if len(a) * len(b) == len(relation):
                    return self.refine(left_groups) + self.refine(right_groups)
            if count * 2 >= len(groups):
                break
        self.merge_reasons.append({'fields': list(whole), 'reason': 'joint_correlation_requires_merge'})
        return [whole]

    def projection(self, names):
        """Yield exactly the requested tuple order, materializing factor projections only."""
        names = tuple(names)
        if len(set(names)) != len(names) or any(name not in self.index for name in names):
            raise ValueError('Projection names must be unique known fields')
        self.counters['projection_queries'] += 1
        if self.empty:
            return
        requested = tuple(self.index[name] for name in names)
        pieces = []
        selections = {}
        for field in requested:
            number, _ = self.factor_for_field[field]
            selections.setdefault(number, []).append(field)
        for number, selected_fields in selections.items():
            indices, rows = self.factors[number]
            selected = tuple(selected_fields)
            key = (number, selected)
            if key not in self.query_cache:
                offsets = tuple(self.factor_for_field[i][1] for i in selected)
                self.query_cache[key] = frozenset(tuple(row[i] for i in offsets) for row in rows)
                self.counters['factor_projection_rows_scanned'] += len(rows)
                self.counters['factor_projection_cells_scanned'] += len(rows) * len(offsets)
                self.counters['factor_projection_builds'] += 1
            else:
                self.counters['factor_projection_cache_hits'] += 1
            pieces.append((selected, self.query_cache[key]))
        for values in product(*(rows for _, rows in pieces)):
            positions = {}
            for (indices, _), row in zip(pieces, values, strict=True):
                positions.update(zip(indices, row, strict=True))
            self.counters['projection_output_rows'] += 1
            yield tuple(positions[i] for i in requested)

    def membership(self, words):
        self.counters['membership_queries'] += 1
        try:
            packed = self.unpack(words)
        except ValueError:
            return False
        if self.empty:
            return False
        for indices, rows in self.factors:
            self.counters['membership_factor_probes'] += 1
            value = tuple((packed >> self.fields[i][0]) & ((1 << self.fields[i][1]) - 1) for i in indices)
            if value not in rows:
                return False
        return True

    def reconstruct(self):
        self.counters['reconstruction_queries'] += 1
        for values in self.projection(self.names):
            self.counters['reconstruction_output_rows'] += 1
            yield self.pack(values)

    def validate(self, supports=(), max_rows=100000):
        # Membership checks every supplied row even when output validation is capped.
        for row in self.original:
            if not self.membership(row):
                raise AssertionError('Original row rejected by factors')
        negatives = 0
        sampled_fields = sorted({i * (len(self.fields) - 1) // min(31, len(self.fields) - 1)
                                 for i in range(min(32, len(self.fields)))}) if len(self.fields) > 1 else list(range(len(self.fields)))
        for row in list(sorted(self.rows))[:16]:
            for i in sampled_fields:
                _, width = self.fields[i]
                changed = list(row)
                changed[i] = (changed[i] + 1) % (1 << width)
                packed = self.pack(changed)
                if self.membership(packed) != (packed in self.original):
                    raise AssertionError('Membership differs from canonical oracle')
                negatives += packed not in self.original
        queries = {(), self.names}
        queries.update((name,) for name in self.names)
        queries.update(tuple(name for name in self.names if name in support) for support in supports)
        if len(self.names) <= 8:
            queries.update(subset for n in range(len(self.names) + 1) for subset in combinations(self.names, n))
        compared = 0
        reconstructed = False
        if len(self.rows) <= max_rows:
            for names in sorted(queries):
                indices = tuple(self.index[name] for name in names)
                expected = {tuple(row[i] for i in indices) for row in self.rows}
                actual = set(self.projection(names))
                self.counters['oracle_projection_rows_scanned'] += len(self.rows)
                self.counters['oracle_projection_cells_scanned'] += len(self.rows) * len(indices)
                if actual != expected:
                    raise AssertionError('Projection differs from canonical oracle')
                compared += 1
            if set(self.reconstruct()) != self.original:
                raise AssertionError('Reconstruction differs from canonical oracle')
            reconstructed = True
        return {'original_members_checked': len(self.original), 'negative_members_checked': negatives, 'negative_control_rows_sampled': min(16, len(self.rows)),
                'negative_control_fields_sampled': len(sampled_fields),
                'projection_queries_compared': compared, 'complete_reconstruction_checked': reconstructed,
                'max_rows_for_projection_and_reconstruction_validation': max_rows}

    def report(self):
        factor_payload = [{'fields': [self.names[i] for i in indices], 'values': [list(row) for row in sorted(rows)]}
                          for indices, rows in self.factors]
        return {
            'input_rows': len(self.rows), 'duplicate_input_rows': self.duplicate_input_rows,
            'word_count': self.word_count, 'original_word_cells': len(self.rows) * self.word_count,
            'original_field_cells': len(self.rows) * len(self.names),
            'initial_event_components': [[self.names[i] for i in group] for group in self.initial_components],
            'factors': [{'fields': [self.names[i] for i in indices], 'tuple_count': len(rows),
                         'values': [list(row) for row in sorted(rows)]} for indices, rows in self.factors],
            'factor_tuple_count': sum(len(rows) for _, rows in self.factors),
            'canonical_rows_compact_json_bytes': len(json.dumps(sorted(self.original), separators=(',', ':')).encode()),
            'factor_payload_compact_json_bytes': len(json.dumps(factor_payload, separators=(',', ':')).encode()),
            'factor_field_cells': sum(len(indices) * len(rows) for indices, rows in self.factors),
            'certificate': self.certificate, 'merge_reasons': self.merge_reasons,
            'search_projection_budget': self.max_search_projections, 'search_projections_used': self.search_projections,
            'search_exhausted': self.search_exhausted, 'construction_counters': self.construction_counters,
            'total_counters': dict(self.counters),
            'retained_original_projection_cache_cells': sum(len(key) * len(rows) for key, rows in self.cache.items()),
            'retained_factor_projection_cache_cells': sum(len(key[1]) * len(rows) for key, rows in self.query_cache.items()),
        }


def supports_from(document):
    """Explicit field-name sets; opaque event payloads are never guessed."""
    if document is None:
        return []
    items = document.get('component_fields', document.get('supports', document.get('events', []))) if isinstance(document, dict) else document
    result = []
    for item in items:
        names = item.get('fields', item.get('support')) if isinstance(item, dict) else item
        if not isinstance(names, list) or not all(isinstance(name, str) for name in names):
            raise ValueError('Each support must be a list of field names or {fields: [...]}')
        result.append(names)
    return result


def self_test():
    fields = {'a': [0, 1], 'b': [1, 1], 'c': [2, 1], 'd': [3, 1]}
    encode = lambda rows: [[sum(value << i for i, value in enumerate(row))] for row in rows]
    cases = [
        ('independent_groups', [(a, a, c, c) for a, c in product(range(2), repeat=2)], [['a'], ['b'], ['c'], ['d']], [2, 2]),
        ('three_way_parity_with_independent_field', [(a, b, a ^ b, d) for a, b, d in product(range(2), repeat=3)], [['a'], ['b'], ['c'], ['d']], [1, 3]),
        ('correlated_branch', [(0, 0, 0, 1), (1, 1, 1, 1)], [['a'], ['b'], ['c'], ['d']], [1, 3]),
        ('constant_relation', [(1, 0, 1, 0)], [['a', 'b', 'c', 'd']], [4]),
        ('empty_relation', [], [['a'], ['b']], []),
        ('support_coarsening_stays_exact', list(product(range(2), repeat=4)), [['a', 'b'], ['c', 'd']], [2, 2]),
    ]
    results = []
    for name, rows, supports, sizes in cases:
        factor = FactorRelation(fields, encode(rows), supports)
        checked = factor.validate(supports)
        assert sorted(len(indices) for indices, _ in factor.factors) == sizes, name
        results.append({'case': name, 'pass': True, 'validation': checked, 'report': factor.report()})
    # A parity relation has pairwise independent projections, yet not a product.
    parity = FactorRelation({k: v for k, v in fields.items() if k != 'd'}, encode([(a, b, a ^ b) for a, b in product(range(2), repeat=2)]))
    assert all(len(set(parity.projection(pair))) == 4 for pair in combinations(parity.names, 2))
    assert len(parity.factors) == 1 and len(parity.factors[0][1]) == 4
    fallback = FactorRelation(fields, encode(list(product(range(2), repeat=4))), max_search_projections=0)
    assert fallback.search_exhausted and len(fallback.factors) == 1
    fallback.validate()
    # Cross-u64 fields and nonzero undeclared bits exercise the actual wire shape.
    wide = FactorRelation({'x': [63, 3], 'y': [70, 2]}, [[1 << 63, 2 | (y << 6)] for y in range(4)])
    wide.validate()
    assert set(wide.projection(['x'])) == {(5,)}
    assert set(wide.projection(['y'])) == {(0,), (1,), (2,), (3,)}
    try:
        FactorRelation({'x': [0, 1]}, [[2]])
    except ValueError:
        pass
    else:
        raise AssertionError('Undeclared nonzero bits were silently discarded')
    exhaustive = 0
    tiny_fields = {'a': [0, 1], 'b': [1, 1], 'c': [2, 1]}
    for mask in range(256):
        original = {(word,) for word in range(8) if mask & (1 << word)}
        for support in [[], [['a', 'b']]]:
            relation = FactorRelation(tiny_fields, [list(row) for row in original], support)
            assert set(relation.reconstruct()) == original
            assert all(relation.membership([word]) == ((word,) in original) for word in range(8))
            for count in range(4):
                for names in combinations(tiny_fields, count):
                    expected = {tuple((word >> tiny_fields[name][0]) & 1 for name in names) for word, in original}
                    assert set(relation.projection(names)) == expected
            exhaustive += 1
    duplicate = FactorRelation({'x': [0, 1]}, [[0], [0]])
    assert duplicate.duplicate_input_rows == 1 and set(duplicate.reconstruct()) == {(0,)}
    return {'cases': results, 'exhaustive_three_bit_relation_controls': exhaustive, 'parity_pairwise_independence_rejected': True,
            'budget_fallback_exact': True, 'cross_word_fields_exact': True, 'undeclared_bits_rejected': True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('input', nargs='?')
    parser.add_argument('output', nargs='?')
    parser.add_argument('--supports')
    parser.add_argument('--oracle', help='Independent canonical relation file; fields and every relation must agree')
    parser.add_argument('--max-search-projections', type=int, default=512)
    parser.add_argument('--max-validation-rows', type=int, default=100000)
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    if args.max_search_projections < 0 or args.max_validation_rows < 0:
        parser.error('Budgets must be nonnegative')
    result = {'scope': 'Consumer-side lossless factoring of already-materialized canonical relations; input solver, extraction, decoding, search, factor/projection caches and full reconstruction costs are not eliminated.',
              'timing': 'NotRun; recorded algorithm operation counters, not a total instruction or allocation cost model', 'tool_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
    if args.self_test:
        result['self_test'] = self_test()
    if args.input:
        raw = Path(args.input).read_bytes()
        document = json.loads(raw)
        if args.oracle:
            oracle_raw = Path(args.oracle).read_bytes()
            oracle = json.loads(oracle_raw)
            if document['fields'] != oracle['fields'] or document['relations'] != oracle['relations']:
                raise ValueError('Input differs from the supplied independent canonical oracle')
            result.update(oracle=str(Path(args.oracle).resolve()), oracle_sha256=hashlib.sha256(oracle_raw).hexdigest(),
                          oracle_fields_and_relations_equal=True)
        support_bytes = Path(args.supports).read_bytes() if args.supports else None
        support_document = json.loads(support_bytes) if support_bytes else None
        supports = supports_from(support_document)
        result.update(input=str(Path(args.input).resolve()), input_sha256=hashlib.sha256(raw).hexdigest(), input_bytes=len(raw),
                      supports=supports, support_mode='actual_supplied_supports' if support_bytes else 'singleton_field_candidates_without_event_support', supports_sha256=hashlib.sha256(support_bytes).hexdigest() if support_bytes else None,
                      event_metadata=document.get('events'), relations={})
        for name, rows in document['relations'].items():
            factor = FactorRelation(document['fields'], rows, supports, args.max_search_projections)
            validation = factor.validate(supports, args.max_validation_rows)
            result['relations'][name] = factor.report() | {'validation': validation}
    elif not args.self_test:
        parser.error('Supply input/output or --self-test')
    encoded = json.dumps(result, indent=2) + '\n'
    if args.output:
        destination = Path(args.output)
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(encoded)
        print(json.dumps({'output': str(destination), 'relations': len(result.get('relations', {})), 'self_test': args.self_test}))
    else:
        print(encoded, end='')


if __name__ == '__main__':
    main()
