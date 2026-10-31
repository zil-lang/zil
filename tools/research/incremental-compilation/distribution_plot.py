#!/usr/bin/env python3
"""Plot frozen large-module cells without combining measurement windows."""
import argparse
import json
from pathlib import Path
import matplotlib

matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.ticker import ScalarFormatter


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--analysis', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--preview', type=Path, required=True)
    parser.add_argument('--default-window', default='generated-default')
    parser.add_argument('--closed-window', default='generated-closed')
    parser.add_argument('--ablation-window', default='implementation-ablation')
    args = parser.parse_args()
    data = json.loads(args.analysis.read_text())['windows']
    args.out.mkdir(parents=True, exist_ok=True)
    args.preview.mkdir(parents=True, exist_ok=True)
    plt.rcParams.update({'svg.fonttype': 'none', 'font.size': 10,
                         'axes.spines.top': False, 'axes.spines.right': False})
    sizes = [128, 512, 2048, 4096, 8192]
    indexed = {name: {(c['case'], c['mode'], c['request']): c for c in window['cells']}
               for name, window in data.items()}

    def series(ax, name, mode, request, label, color):
        cells = [indexed[name][(f'large-{size}', mode, request)] for size in sizes]
        assert all(c['ratio'] is not None for c in cells)
        ax.plot(sizes, [c['ratio'] for c in cells], marker='o', linewidth=2,
                markersize=4, label=label, color=color)
        ax.fill_between(sizes, [c['exploratory_95'][0] for c in cells],
                        [c['exploratory_95'][1] for c in cells], color=color, alpha=.16)

    for ablation in [False, True]:
        fig, axes = plt.subplots(2, 2, figsize=(11, 7))
        for row, (request, label) in enumerate([('cold', 'Cold cache'), ('early_edit', 'Private body edit')]):
            for col, mode in enumerate(['O0', 'O2']):
                ax = axes[row, col]
                ax.axhline(1, color='#666666', linestyle='--', linewidth=1)
                if ablation:
                    series(ax, args.ablation_window, mode, request,
                           'Current default / previous ZIL', '#176B87')
                else:
                    series(ax, args.default_window, mode, request, 'Default / Rust', '#176B87')
                    if mode == 'O2':
                        series(ax, args.closed_window, mode, request,
                               'Closed executable / Rust (separate window)', '#B6501C')
                ax.set_xscale('log', base=2)
                ax.set_yscale('log')
                ax.set_xticks(sizes)
                ax.xaxis.set_major_formatter(ScalarFormatter())
                ax.set_title(f'{mode} - {label}', loc='left', fontweight='bold')
                ax.grid(axis='y', which='both', alpha=.15)
                ax.set_ylabel('Elapsed-time ratio (lower is faster)')
                if row == 1:
                    ax.set_xlabel('Scalar helper functions in one source file')
        title = ('Large-module implementation comparison: current / previous ZIL'
                 if ablation else 'Large-module compile time relative to Rust')
        fig.suptitle(title, fontsize=16, y=.98)
        handles, labels = axes[0, 1].get_legend_handles_labels()
        fig.legend(handles, labels, loc='upper center', bbox_to_anchor=(.5, .94), ncol=2, frameon=False)
        footer = 'Six paired blocks; shaded exploratory 95% intervals. Dashed line = equal elapsed time.'
        footer += '\nIdentical generated workload; not a product-wide result. Full CLI, overflow checks, 8 CGUs.'
        fig.text(.08, .025, footer, fontsize=9, color='#444444')
        fig.tight_layout(rect=(0, .08, 1, .89))
        name = 'large-module-before-after' if ablation else 'large-module-rust'
        fig.savefig(args.out / (name + '.svg'), metadata={'Date': None, 'Creator': 'ZIL frozen stage-5 analysis'})
        fig.savefig(args.preview / (name + '.png'), dpi=130)
        plt.close(fig)


if __name__ == '__main__':
    main()
