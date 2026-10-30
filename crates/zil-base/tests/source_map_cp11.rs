//! Frozen source-map validation controls for request-local index reuse.
use zil_base::{
    CompileResult, ExpansionRole, FileId, Fingerprint, Limits, ModuleId, NodeId, ResultCategory,
    SourceMap, SourceMapEntry, SourceMapId, SourceTarget, SourceUnit, Span,
};

// Frozen by running this control matrix on the unchanged 542d30676 baseline.
const SOURCE_IDS: &[&[u32]] = &[&[], &[7], &[7, 8, 9, 10], &[7, 7, 8, 9], &[7, 8, 7, 9]];
const BASELINE_DIGESTS: &[&str] = &[
    "5120ad48ab73d9a9519224f7a56e13b519361f3815b390cc31cdc0a073b6e5a2",
    "23cf175a11d1d031b36eabda5002ed0845dcafb27a428f834065a774b042109c",
    "546195ec05bff62a197e5413942d3440869f24c450459c545a24d96bd09aed9b",
    "1058bf98784278426a9314938de523ea581a7066c3aad3a9cba965ed83196864",
    "35412e8136077c8b75bfd5e37b2f613ef7f857728d66807340a7c84fbf0ff2b7",
];

fn sources(ids: &[u32]) -> Vec<SourceUnit> {
    ids.iter()
        .enumerate()
        .map(|(index, id)| {
            SourceUnit::new(FileId(*id), ModuleId(index as u32 + 20), "source.zil", "éx")
        })
        .collect()
}

fn valid() -> SourceMap {
    SourceMap {
        // This sidecar has never required its module_id to name a source.
        module_id: ModuleId(99),
        entries: vec![
            SourceMapEntry {
                id: SourceMapId(0),
                target: SourceTarget::Node(NodeId(1)),
                origin: Span::new(FileId(7), 0, 2),
                role: ExpansionRole::Source,
                related: Vec::new(),
                parent: None,
            },
            SourceMapEntry {
                id: SourceMapId(1),
                target: SourceTarget::Span(Span::new(FileId(8), 0, 3)),
                origin: Span::new(FileId(7), 2, 3),
                role: ExpansionRole::TemporaryRead,
                related: vec![Span::new(FileId(9), 0, 2)],
                parent: Some(SourceMapId(0)),
            },
            SourceMapEntry {
                id: SourceMapId(2),
                target: SourceTarget::Parameter {
                    function: NodeId(3),
                    index: 0,
                },
                origin: Span::new(FileId(10), 0, 3),
                role: ExpansionRole::ReceiverExplicit,
                related: vec![Span::new(FileId(7), 0, 2)],
                parent: Some(SourceMapId(1)),
            },
        ],
    }
}

fn cases() -> Vec<SourceMap> {
    let mut maps = vec![SourceMap::new(ModuleId(99)), valid()];
    let mut bad = valid();
    bad.entries[0].id = SourceMapId(9);
    maps.push(bad);
    let mut bad = valid();
    bad.entries[0].parent = Some(SourceMapId(1));
    maps.push(bad);
    let mut bad = valid();
    bad.entries[1].target = bad.entries[0].target.clone();
    bad.entries[1].origin.file = FileId(123);
    maps.push(bad);
    let mut bad = valid();
    bad.entries[0].origin = Span::new(FileId(7), 1, 2);
    maps.push(bad);
    let mut bad = valid();
    bad.entries[0].origin.file = FileId(123);
    maps.push(bad);
    let mut bad = valid();
    bad.entries[1].related[0].end = 4;
    maps.push(bad);
    let mut bad = valid();
    bad.entries[1].target = SourceTarget::Span(Span::new(FileId(8), 1, 2));
    maps.push(bad);
    maps
}

fn matrix(mut validate: impl FnMut(&SourceMap, &Limits) -> CompileResult<()>) -> String {
    let maps = cases();
    let mut trace = String::new();
    // A successful source index may precede an entry failure. The following
    // valid map also repeats targets, which must be unique only within a map.
    for index in [2, 1, 1, 0] {
        trace.push_str(&format!(
            "warm:{index}:{:?}\n",
            validate(&maps[index], &Limits::default())
        ));
    }
    for nodes in [0, 1, 2, 3, 4, 5, usize::MAX] {
        for work in 0..=12 {
            let limits = Limits {
                max_nodes: nodes,
                max_analysis_steps: work,
                ..Limits::default()
            };
            for (index, map) in maps.iter().enumerate() {
                trace.push_str(&format!(
                    "{nodes}:{work}:{index}:{:?}\n",
                    validate(map, &limits)
                ));
            }
        }
    }
    Fingerprint::of(trace.as_bytes()).to_hex()
}

#[test]
fn source_identity_errors_are_charged_in_source_order() {
    let empty = SourceMap::new(ModuleId(99));
    for (ids, work, category, message, span) in [
        (
            [7, 7, 8, 9],
            1,
            ResultCategory::ResourceLimit,
            "source-map validation work budget exceeded",
            None,
        ),
        (
            [7, 7, 8, 9],
            2,
            ResultCategory::SafeReject,
            "duplicate source file identity",
            Some(Span::new(FileId(7), 0, 3)),
        ),
        (
            [7, 8, 7, 9],
            2,
            ResultCategory::ResourceLimit,
            "source-map validation work budget exceeded",
            None,
        ),
        (
            [7, 8, 7, 9],
            3,
            ResultCategory::SafeReject,
            "duplicate source file identity",
            Some(Span::new(FileId(7), 0, 3)),
        ),
    ] {
        let errors = empty
            .validate(
                &sources(&ids),
                &Limits {
                    max_analysis_steps: work,
                    ..Limits::default()
                },
            )
            .unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].category, category);
        assert_eq!(errors[0].message, message);
        assert_eq!(errors[0].span, span);
    }
}

#[test]
fn each_map_has_fresh_nodes_targets_and_validation() {
    let sources = sources(&[7, 8, 9, 10]);
    let map = valid();
    let limits = Limits {
        max_nodes: 4,
        max_analysis_steps: 9,
        ..Limits::default()
    };
    map.validate(&sources, &limits).unwrap();
    let error = map
        .validate(
            &sources,
            &Limits {
                max_analysis_steps: 8,
                ..limits.clone()
            },
        )
        .unwrap_err();
    assert_eq!(error[0].category, ResultCategory::ResourceLimit);
    map.validate(&sources, &limits).unwrap();
    let error = map
        .validate(
            &sources,
            &Limits {
                max_nodes: 3,
                ..limits
            },
        )
        .unwrap_err();
    assert_eq!(error[0].message, "source-map node budget exceeded");
    let duplicate = &cases()[4];
    let errors = duplicate
        .validate(&sources, &Limits::default())
        .unwrap_err();
    assert_eq!(
        errors[0].message,
        "source-map target is mapped more than once"
    );
    SourceMap::new(ModuleId(99))
        .validate(
            &[],
            &Limits {
                max_nodes: 0,
                max_analysis_steps: 0,
                ..Limits::default()
            },
        )
        .unwrap();
}

#[test]
fn standalone_validation_matrix_preserves_complete_diagnostics() {
    for (ids, expected) in SOURCE_IDS.iter().zip(BASELINE_DIGESTS) {
        let sources = sources(ids);
        let digest = matrix(|map, limits| map.validate(&sources, limits));
        assert_eq!(&digest, expected);
        println!("CP11_SOURCE_MAP_MATRIX:{ids:?}:{digest}");
    }
}

#[test]
fn shared_validation_matrix_matches_the_original_standalone_results() {
    for (ids, expected) in SOURCE_IDS.iter().zip(BASELINE_DIGESTS) {
        let sources = sources(ids);
        let mut validator = zil_base::SourceMapValidator::new(&sources);
        let digest = matrix(|map, limits| validator.validate(map, limits));
        assert_eq!(&digest, expected);
        println!("CP11_SHARED_SOURCE_MAP_MATRIX:{ids:?}:{digest}");
    }
}

#[test]
fn incomplete_source_scans_never_supply_a_later_index() {
    let empty = SourceMap::new(ModuleId(99));
    for ids in [&[7, 8, 9, 10][..], &[7, 8, 7, 9][..]] {
        let sources = sources(ids);
        let mut validator = zil_base::SourceMapValidator::new(&sources);
        let limits = Limits {
            max_analysis_steps: 2,
            ..Limits::default()
        };
        assert_eq!(
            validator.validate(&empty, &limits),
            empty.validate(&sources, &limits)
        );
        assert_eq!(
            validator.validate(&empty, &Limits::default()),
            empty.validate(&sources, &Limits::default())
        );
        assert_eq!(
            validator.validate(&valid(), &Limits::default()),
            valid().validate(&sources, &Limits::default())
        );
    }
}
