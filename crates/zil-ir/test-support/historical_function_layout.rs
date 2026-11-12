//! Test-only structural V40 function walker. No proof or production decoder.
//! Caller must authenticate the existing 17 SHA-256 inputs before calling.
//! The untouched suffix is validated by the current decoder and round-trip.
type R<T> = Result<T, String>;
struct Walk<'a> {
    bytes: &'a [u8],
    pos: usize,
    work: usize,
    nodes: usize,
    depth: usize,
}
impl Walk<'_> {
    fn take(&mut self, n: usize) -> R<&[u8]> {
        self.work = self.work.checked_sub(n).ok_or("fixture walk budget")?;
        let end = self.pos.checked_add(n).ok_or("fixture offset overflow")?;
        let out = self.bytes.get(self.pos..end).ok_or("truncated fixture")?;
        self.pos = end;
        Ok(out)
    }
    fn byte(&mut self) -> R<u8> {
        Ok(self.take(1)?[0])
    }
    fn word(&mut self) -> R<usize> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()) as usize)
    }
    fn tag(&mut self, max: u8) -> R<u8> {
        let tag = self.byte()?;
        if tag > max {
            return Err(format!("unsupported tag {tag} at {}", self.pos - 1));
        }
        Ok(tag)
    }
    fn absent(&mut self) -> R<()> {
        if self.byte()? != 0 {
            return Err(format!("unsupported optional relation at {}", self.pos - 1));
        }
        Ok(())
    }
    fn count(&mut self) -> R<usize> {
        let n = self.word()?;
        if n > self.nodes || n > self.bytes.len() - self.pos {
            return Err("fixture count limit".into());
        }
        self.work = self
            .work
            .checked_sub(n)
            .ok_or("fixture collection budget")?;
        Ok(n)
    }
    fn text(&mut self) -> R<()> {
        let n = self.count()?;
        std::str::from_utf8(self.take(n)?).map_err(|_| "fixture UTF8")?;
        Ok(())
    }
    fn words(&mut self) -> R<()> {
        let n = self.count()?;
        self.take(n.checked_mul(4).ok_or("fixture count overflow")?)?;
        Ok(())
    }
    fn optional_word(&mut self) -> R<()> {
        if self.tag(1)? == 1 {
            self.take(4)?;
        }
        Ok(())
    }
    fn ty(&mut self, depth: usize) -> R<()> {
        if depth >= self.depth.min(64) {
            return Err("fixture type depth".into());
        }
        let tag = self.byte()?;
        match tag {
            0..=2 | 18 | 24 => {}
            6 => {
                self.tag(1)?;
                self.ty(depth + 1)?;
            }
            7 | 8 | 23 => self.ty(depth + 1)?,
            9 => {
                for _ in 0..self.count()? {
                    self.ty(depth + 1)?;
                }
                self.ty(depth + 1)?;
            }
            10 => {
                self.take(4)?;
            }
            13 | 17 => {
                self.text()?;
                for _ in 0..self.count()? {
                    self.text()?;
                    self.ty(depth + 1)?;
                }
            }
            14 => {
                self.ty(depth + 1)?;
                self.take(4)?;
            }
            _ => {
                return Err(format!(
                    "unsupported fixture type {tag} at {}",
                    self.pos - 1
                ));
            }
        }
        Ok(())
    }
    fn calls(&mut self) -> R<()> {
        for _ in 0..self.count()? {
            self.take(4)?;
            self.tag(3)?;
        }
        Ok(())
    }
    fn callable(&mut self) -> R<()> {
        for _ in 0..self.count()? {
            self.take(8)?;
            self.tag(3)?;
            self.absent()?;
            for _ in 0..3 {
                self.words()?;
            }
            self.tag(2)?;
        }
        match self.tag(1)? {
            0 => {
                for _ in 0..self.count()? {
                    self.calls()?;
                    self.tag(1)?;
                }
            }
            1 => {
                self.take(4)?;
                for _ in 0..self.count()? {
                    self.calls()?;
                    if self.tag(1)? == 0 {
                        self.words()?;
                    } else {
                        self.tag(1)?;
                    }
                }
            }
            _ => unreachable!(),
        }
        Ok(())
    }
    fn constant(&mut self) -> R<()> {
        match self.byte()? {
            0 | 6 | 7 => {}
            1 => {
                self.tag(1)?;
            }
            2 | 11 => {
                self.take(4)?;
            }
            3 | 9 | 10 | 12 => {
                self.take(8)?;
            }
            4 => {
                self.take(1)?;
            }
            5 => {
                self.take(2)?;
            }
            tag => return Err(format!("unsupported fixture constant {tag}")),
        }
        Ok(())
    }
    fn instruction(&mut self) -> R<()> {
        self.optional_word()?;
        let tag = self.byte()?;
        match tag {
            0 => self.constant()?,
            1 | 10 | 11 | 12 | 30 | 38 | 40 | 43 | 44 | 52 | 77 => {
                self.take(4)?;
            }
            2 | 23 | 24 | 29 | 50 | 51 => {
                self.take(8)?;
            }
            4 => {
                self.tag(21)?;
                self.take(8)?;
            }
            5 | 6 | 8 | 41 | 42 => {
                self.take(4)?;
                self.words()?;
            }
            49 | 72 => self.words()?,
            28 | 68 | 78 => {
                self.take(4)?;
                self.tag(1)?;
            }
            45 => {
                self.take(4)?;
                self.tag(4)?;
            }
            53 => {
                self.take(4)?;
                self.tag(3)?;
                self.words()?;
            }
            59 => {
                self.take(4)?;
                self.ty(0)?;
            }
            63 => {}
            _ => {
                return Err(format!(
                    "unsupported fixture opcode {tag} at {}",
                    self.pos - 1
                ));
            }
        }
        self.take(20)?;
        Ok(())
    }
    fn function(&mut self) -> R<Vec<(usize, &'static [u8])>> {
        let mut insertions: Vec<(usize, &'static [u8])> = vec![(self.pos, &[0, 0])]; // No dynamic instance; unknown retained result sources.
        self.absent()?; // Direct CallKind.
        insertions.push((self.pos, &[0])); // Local resume bound.
        self.absent()?; // StorageLifeContract.
        if self.tag(1)? == 1 {
            self.words()?;
        }
        if self.tag(1)? == 1 {
            // V48 inserts Some(empty) before the old return-source tag. The
            // authenticated subset has no qualified retained source relation;
            // its actual old capture/role semantics are still reverified.
            let insertion = self.pos;
            if self.tag(1)? == 1 {
                self.take(4)?;
            }
            self.take(4)?;
            self.tag(3)?;
            for _ in 0..5 {
                self.words()?;
            }
            insertions.push((insertion, &[1, 0, 0, 0, 0])); // Some(empty).
        }
        if self.tag(1)? == 1 {
            self.callable()?;
        }
        insertions.push((self.pos, &[0, 0, 0, 0])); // Ordered protocol role count is zero.
        self.take(4)?;
        self.text()?;
        if self.tag(1)? == 1 {
            self.text()?;
            self.text()?;
            self.words()?;
            self.tag(1)?;
            insertions.push((self.pos, &[0])); // No driver-transfer promise.
            self.tag(2)?;
        }
        self.optional_word()?;
        self.words()?;
        self.ty(0)?;
        self.tag(7)?;
        for _ in 0..self.count()? {
            self.take(4)?;
            self.text()?;
            self.ty(0)?;
            self.tag(1)?;
            self.take(20)?;
        }
        for _ in 0..self.count()? {
            self.ty(0)?;
        }
        for _ in 0..self.count()? {
            self.take(4)?;
            for _ in 0..self.count()? {
                self.instruction()?;
            }
            match self.tag(4)? {
                0 => self.optional_word()?,
                1 => {
                    self.take(4)?;
                }
                2 => {
                    self.take(12)?;
                }
                3 | 4 => {}
                _ => unreachable!(),
            }
        }
        self.take(24)?;
        Ok(insertions)
    }
}
/// Structurally insert V44 None and V48 known-empty capture metadata, then
/// relabel only the transport envelope. The immutable inputs never change.
/// Module/plan schema and fingerprints deliberately remain historical: the
/// caller must current-decode, round-trip, update identity, and re-encode.
pub fn transport(
    bytes: &[u8],
    max_bytes: usize,
    max_nodes: usize,
    max_depth: usize,
    max_work: usize,
) -> R<Vec<u8>> {
    if bytes.len() > max_bytes {
        return Err("fixture byte quota".into());
    }
    let mut w = Walk {
        bytes,
        pos: 0,
        work: max_work,
        nodes: max_nodes,
        depth: max_depth,
    };
    if w.take(8)? != b"ZILMODP\0" || w.word()? != 40 || w.word()? != 40 {
        return Err("not frozen schema40".into());
    }
    w.take(36)?; // module ID, source fingerprint.
    w.text()?;
    w.text()?;
    w.take(2)?;
    w.tag(1)?;
    w.text()?;
    w.text()?;
    w.tag(1)?;
    for _ in 0..5 {
        if w.word()? != 0 {
            return Err("nonempty historical life/dyn inventory".into());
        }
    }
    let count = w.count()?;
    let capacity = count.checked_mul(5).ok_or("fixture insertion overflow")?;
    w.work = w
        .work
        .checked_sub(capacity)
        .ok_or("fixture insertion work quota")?;
    let mut insertions: Vec<(usize, &'static [u8])> = Vec::with_capacity(capacity);
    let mut added = 0usize;
    for _ in 0..count {
        for (position, fields) in w.function()? {
            insertions.push((position, fields));
            added = added
                .checked_add(fields.len())
                .ok_or("fixture output overflow")?;
        }
    }
    let length = bytes
        .len()
        .checked_add(added)
        .ok_or("fixture output overflow")?;
    if length > max_bytes {
        return Err("fixture output quota".into());
    }
    w.work
        .checked_sub(length)
        .ok_or("fixture output work quota")?;
    let mut output = Vec::with_capacity(length);
    let mut start = 0;
    for (end, fields) in insertions {
        output.extend_from_slice(&bytes[start..end]);
        output.extend_from_slice(fields);
        start = end;
    }
    output.extend_from_slice(&bytes[start..]);
    output[8..12].copy_from_slice(&zil_ir::wire::WIRE_VERSION.to_le_bytes());
    Ok(output)
}
