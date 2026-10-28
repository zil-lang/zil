use zil_base::{Fingerprint, FingerprintBuilder};
// Known expected digests were generated independently with Python hashlib,
// not by the production streaming implementation under test.
#[test]
fn streaming_sha256_boundaries_and_empty_updates() {
    let vectors = [
        (
            0usize,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            1usize,
            "ab897fbdedfa502b2d839b6a56100887dccdc507555c282e59589e06300a62e2",
        ),
        (
            55usize,
            "0c43a30970ee9525a6603d5656803c325bded148fa65bbd363df54cb3ac3dee5",
        ),
        (
            56usize,
            "f4dcdc09c846d1b3621701516b300055c1e7875e216ba019dd4423f5a420f2d0",
        ),
        (
            63usize,
            "ba3bb62c4369fe74e7b94392d0f04ed284a7b5d7295bfa48fabdb25c01d85ab7",
        ),
        (
            64usize,
            "cb6862268ca7c02e7b6af558f0e3d236ce9ec7ff41cc746b2848c04c582da94e",
        ),
        (
            65usize,
            "b68af55df9ce8ee60870e45d10e36b2e8363c9896f6e62478dcb6f593a4d7133",
        ),
        (
            127usize,
            "a8365b6e6ed1daf56c41bae89ab089c03b671240ae103070480a7f271bdbad57",
        ),
        (
            128usize,
            "d399f065c9f21e2fd51e2aeadb7768eab7e6e45e5150f31227c9711934a4d1d3",
        ),
        (
            129usize,
            "d672408e0571636ea21092304eb31ba8fc1fbffc5d5ea6531d62b072714599db",
        ),
        (
            1024usize,
            "e051d1007607de494c073da3c29903d6c0abfee7a4c0609f560a340a1947b470",
        ),
        (
            1000000usize,
            "467288749485a13fcb1a0c432c6615ec392db0b70198f26599306a79ed72fb8d",
        ),
    ];
    for (length, expected) in vectors {
        let data = (0..length)
            .map(|i| ((i * 73 + 19) % 256) as u8)
            .collect::<Vec<_>>();
        assert_eq!(Fingerprint::of(&data).to_hex(), expected);
        for chunk in [1, 7, 55, 64, 65, 127, 8192] {
            let mut state = FingerprintBuilder::new();
            for bytes in data.chunks(chunk) {
                state.update(&[]);
                state.update(bytes);
            }
            state.update(&[]);
            assert_eq!(
                state.digest().to_hex(),
                expected,
                "length={length} chunk={chunk}"
            );
        }
    }
}
#[test]
fn cloning_a_partial_hash_preserves_both_streams() {
    let mut a = FingerprintBuilder::new();
    a.update(b"prefix");
    let mut b = a.clone();
    a.update(b"-a");
    b.update(b"-b");
    assert_eq!(a.digest(), Fingerprint::of(b"prefix-a"));
    assert_eq!(b.digest(), Fingerprint::of(b"prefix-b"));
}
