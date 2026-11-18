//! Each reaching state owns an exact sparse SSA table. Pages and their immutable
//! class values can be shared between sibling snapshots until a real write.
use super::*;

const PAGE_LEN: usize = u64::BITS as usize;

#[derive(Clone)]
pub(super) struct Page {
    pub(super) present: u64,
    classed: u64,
    cells: [Option<SharedType>; PAGE_LEN],
}
impl Page {
    fn empty() -> Self {
        Self { present: 0, classed: 0, cells: std::array::from_fn(|_| None) }
    }
    pub(super) fn entries(&self) -> impl Iterator<Item = (usize, &SharedType)> {
        self.entries_for(self.present)
    }
    fn entries_for(&self, members: u64) -> impl Iterator<Item = (usize, &SharedType)> {
        Occupied(members).map(|offset| {
            (offset, self.cells[offset].as_ref().expect("occupied SSA page slot"))
        })
    }
}

#[derive(Clone)]
pub(super) struct Values {
    len: usize,
    pub(super) pages: Vec<Option<Rc<Page>>>,
}
impl Values {
    pub(super) fn new(world: &mut World<'_, '_>, len: usize) -> R<Self> {
        let pages = len.div_ceil(PAGE_LEN);
        world.pay(pages.saturating_add(1))?;
        Ok(Self { len, pages: vec![None; pages] })
    }
    pub(super) fn len(&self) -> usize { self.len }
    pub(super) fn get(&self, index: usize) -> Option<&SharedType> {
        if index >= self.len { return None; }
        self.pages[index / PAGE_LEN].as_ref()?.cells[index % PAGE_LEN].as_ref()
    }
    pub(super) fn set(
        &mut self,
        world: &mut World<'_, '_>,
        index: usize,
        value: SharedType,
    ) -> R<()> {
        world.pay(2)?;
        if index >= self.len {
            return Err(bad("original SSA result leaves its actual class directory"));
        }
        let SemanticTypeValue::Resolved { classes, .. } = value.as_ref() else {
            return Err(bad("original SSA cell has an unresolved complete type"));
        };
        let has_classes = !classes.is_empty();
        let page = &mut self.pages[index / PAGE_LEN];
        if page.is_none() {
            world.pay(PAGE_LEN.saturating_add(2))?;
            *page = Some(Rc::new(Page::empty()));
        }
        let page = page.as_mut().expect("SSA write initialized its page");
        let page = writable(world, page)?;
        let offset = index % PAGE_LEN;
        let bit = 1u64 << offset;
        page.cells[offset] = Some(value);
        page.present |= bit;
        if has_classes {
            page.classed |= bit;
        } else {
            page.classed &= !bit;
        }
        Ok(())
    }
    pub(super) fn iter(&self) -> impl Iterator<Item = (usize, &SharedType)> {
        self.pages.iter().enumerate().flat_map(|(number, page)| {
            page.iter().flat_map(move |page| page.entries().map(move |(offset, value)| {
                (number * PAGE_LEN + offset, value)
            }))
        })
    }
    pub(super) fn classed(&self) -> impl Iterator<Item = &SharedType> {
        self.pages.iter().flatten().flat_map(|page| {
            page.entries_for(page.classed).map(|(_, value)| value)
        })
    }
    pub(super) fn retain(
        &mut self,
        world: &mut World<'_, '_>,
        mut keep: impl FnMut(usize) -> bool,
    ) -> R<()> {
        world.pay(self.pages.len())?;
        for (number, slot) in self.pages.iter_mut().enumerate() {
            let Some(page) = slot else { continue; };
            world.pay(page.present.count_ones() as usize)?;
            let remove = Occupied(page.present)
                .filter(|offset| !keep(number * PAGE_LEN + *offset))
                .fold(0u64, |bits, offset| bits | 1u64 << offset);
            if remove == 0 { continue; }
            if page.present == remove {
                *slot = None;
                continue;
            }
            let page = writable(world, page)?;
            world.pay((remove.count_ones() as usize).saturating_add(1))?;
            for offset in Occupied(remove) {
                page.cells[offset] = None;
            }
            page.present &= !remove;
            page.classed &= !remove;
        }
        Ok(())
    }
}
fn writable<'a>(world: &mut World<'_, '_>, shared: &'a mut Rc<Page>) -> R<&'a mut Page> {
    if Rc::strong_count(shared) != 1 {
        world.pay(PAGE_LEN.saturating_add(2))?;
    }
    Ok(Rc::make_mut(shared))
}
struct Occupied(u64);
impl Iterator for Occupied {
    type Item = usize;
    fn next(&mut self) -> Option<usize> {
        if self.0 == 0 { return None; }
        let offset = self.0.trailing_zeros();
        self.0 &= self.0 - 1;
        Some(offset as usize)
    }
}
