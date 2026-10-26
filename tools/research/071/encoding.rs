//! Coordinate representation only. It never decides which raw instruction is legal.
//! An overlap in an actually reachable normal image is an error, never a filtered row.
use crate::{
    bdd::{Bdd, Id},
    budget::{R, Work},
    extract::Layout,
    schema::Schema,
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Representation {
    Dense,
    Location,
    Local,
}
impl Representation {
    pub fn name(self) -> &'static str {
        match self {
            Self::Dense => "dense",
            Self::Location => "location",
            Self::Local => "local",
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Projection {
    Zero,
    Direct(usize),
    Incident { location: usize, code: u32 },
}
#[derive(Debug)]
struct Location {
    at: usize,
    width: usize,
    // Original owner order, one dense coordinate per owner. No deduplication of
    // raw instruction operands occurs in this representation module.
    dense_bits: Vec<usize>,
}
#[derive(Debug)]
pub struct Encoding {
    pub representation: Representation,
    pub physical: Schema,
    pub incidence_bits: usize,
    pub location_bits: usize,
    pub domain_pairs: usize,
    pub raw_domain_edges: usize,
    projections: Vec<Projection>,
    locations: Vec<Location>,
}
pub type Assignments = Vec<(usize, Id, Id)>;
type Compiled = (Vec<(usize, Id)>, Assignments, Id);

impl Encoding {
    pub fn from_function(
        layout: &Layout,
        m: &zil_ir::TypedModule,
        f: &zil_ir::TypedFunction,
        representation: Representation,
        work: &mut Work,
    ) -> R<Self> {
        let domains = if representation == Representation::Local {
            Some(crate::domains::compute(m, f, layout, work)?)
        } else {
            None
        };
        Self::new_with_domains(layout, representation, domains.as_ref(), work)
    }

    #[cfg(test)]
    pub fn new(layout: &Layout, representation: Representation, work: &mut Work) -> R<Self> {
        Self::new_with_domains(layout, representation, None, work)
    }

    pub fn new_with_domains(
        layout: &Layout,
        representation: Representation,
        domains: Option<&crate::domains::Domains>,
        work: &mut Work,
    ) -> R<Self> {
        if representation == Representation::Local && domains.is_none() {
            return Err("InternalError: local representation requires computed domains".into());
        }
        work.charge(layout.schema.bits.saturating_mul(3))?;
        let incidence_bits = layout.owners.len().saturating_mul(layout.tokens.len());
        let physical = if representation == Representation::Dense {
            work.charge(layout.schema.fields.keys().map(|s| s.len() + 2).sum())?;
            layout.schema.clone()
        } else {
            let mut fields = BTreeMap::new();
            for (key, &(_, width)) in &layout.schema.fields {
                work.charge(key.len() + 2)?;
                if !key.starts_with("i:") {
                    fields.insert(key.clone(), width);
                }
            }
            for (token_index, token) in layout.tokens.iter().enumerate() {
                let owner_count =
                    domains.map_or(layout.owners.len(), |d| d.owners[token_index].len());
                let width = (usize::BITS - owner_count.leading_zeros()).max(1) as usize;
                work.charge(token.len() + 3)?;
                fields.insert(format!("p:{token}"), width);
            }
            Schema::from_fields(fields, work)?
        };
        let mut projections = vec![Projection::Direct(usize::MAX); layout.schema.bits];
        let mut locations = Vec::new();
        if representation != Representation::Dense {
            for (token_index, token) in layout.tokens.iter().enumerate() {
                let (at, width) = physical.field(&format!("p:{token}"))?;
                let mut dense_bits = Vec::new();
                for (index, owner) in layout.owners.iter().enumerate() {
                    work.charge(token.len() + owner.len() + 4)?;
                    let (dense, w) = layout.schema.field(&format!("i:{owner}:{token}"))?;
                    if w != 1 {
                        return Err("InternalError: incidence width".into());
                    }
                    let included =
                        domains.is_none_or(|d| d.owners[token_index].binary_search(&index).is_ok());
                    if included {
                        projections[dense] = Projection::Incident {
                            location: locations.len(),
                            code: dense_bits.len() as u32 + 1,
                        };
                        dense_bits.push(dense);
                    } else {
                        projections[dense] = Projection::Zero;
                    }
                }
                locations.push(Location {
                    at,
                    width,
                    dense_bits,
                });
            }
        }
        for (key, &(dense, width)) in &layout.schema.fields {
            if representation != Representation::Dense && key.starts_with("i:") {
                continue;
            }
            work.charge(key.len() + width)?;
            let (physical, w) = physical.field(key)?;
            if w != width {
                return Err("InternalError: direct coordinate width".into());
            }
            for offset in 0..width {
                projections[dense + offset] = Projection::Direct(physical + offset);
            }
        }
        if projections
            .iter()
            .any(|p| matches!(p, Projection::Direct(usize::MAX)))
        {
            return Err("InternalError: incomplete coordinate map".into());
        }
        let location_bits = locations.iter().map(|l| l.width).sum();
        let domain_pairs = if representation == Representation::Dense {
            incidence_bits
        } else {
            locations.iter().map(|l| l.dense_bits.len()).sum()
        };
        Ok(Self {
            representation,
            physical,
            incidence_bits,
            location_bits,
            domain_pairs,
            raw_domain_edges: domains.map_or(0, |d| d.raw_edges),
            projections,
            locations,
        })
    }

    pub fn read(&self, dense: usize, bdd: &mut Bdd) -> R<Id> {
        bdd.work.charge(1)?;
        match self.projections[dense] {
            Projection::Zero => Ok(0),
            Projection::Direct(physical) => bdd.var(physical, false),
            Projection::Incident { location, code } => {
                let location = &self.locations[location];
                let mut equal = 1;
                for i in 0..location.width {
                    let bit = bdd.var(location.at + i, false)?;
                    let test = if code >> i & 1 != 0 {
                        bit
                    } else {
                        bdd.not(bit)?
                    };
                    equal = bdd.and(equal, test)?;
                }
                Ok(equal)
            }
        }
    }

    /// Simultaneous dense output formulas become physical equations. The
    /// multiplicity predicate is checked against normal input by the solver.
    pub fn equations(
        &self,
        changes: BTreeMap<usize, (Id, Id)>,
        bdd: &mut Bdd,
    ) -> R<(Vec<(usize, Id)>, Id)> {
        let (factors, _, invalid) = self.compile(changes, bdd, false)?;
        Ok((factors, invalid))
    }
    pub fn assignments(
        &self,
        changes: BTreeMap<usize, (Id, Id)>,
        bdd: &mut Bdd,
    ) -> R<(Assignments, Id)> {
        let (_, assignments, invalid) = self.compile(changes, bdd, true)?;
        Ok((assignments, invalid))
    }
    fn compile(
        &self,
        changes: BTreeMap<usize, (Id, Id)>,
        bdd: &mut Bdd,
        direct: bool,
    ) -> R<Compiled> {
        let mut factors = Vec::new();
        let mut assignments = Vec::new();
        let mut invalid = 0;
        let mut changed_locations = vec![false; self.locations.len()];
        bdd.work.charge(changed_locations.len() + changes.len())?;
        for (&dense, &(rhs, free)) in &changes {
            match self.projections[dense] {
                Projection::Zero => {
                    if free != 0 {
                        return Err("Unsupported: unconstrained incidence update".into());
                    }
                    invalid = bdd.or(invalid, rhs)?;
                }
                Projection::Direct(physical) => {
                    bdd.context.equation = Some(physical);
                    if direct {
                        assignments.push((physical, rhs, free));
                    } else {
                        let post = bdd.var(physical, true)?;
                        let equal = bdd.equal(post, rhs)?;
                        let factor = bdd.or(free, equal)?;
                        factors.push((physical, factor));
                    }
                }
                Projection::Incident { location, .. } => {
                    if free != 0 {
                        return Err("Unsupported: unconstrained incidence update".into());
                    }
                    changed_locations[location] = true;
                }
            }
        }
        for (location_index, &changed) in changed_locations.iter().enumerate() {
            if !changed {
                continue;
            }
            let location = &self.locations[location_index];
            let mut seen = 0;
            let mut bits = vec![0; location.width];
            for (owner, &dense) in location.dense_bits.iter().enumerate() {
                bdd.work.charge(2 + location.width)?;
                let rhs = if let Some(&(rhs, _)) = changes.get(&dense) {
                    rhs
                } else {
                    self.read(dense, bdd)?
                };
                let overlap = bdd.and(seen, rhs)?;
                invalid = bdd.or(invalid, overlap)?;
                seen = bdd.or(seen, rhs)?;
                let code = owner as u32 + 1;
                for (i, bit) in bits.iter_mut().enumerate() {
                    if code >> i & 1 != 0 {
                        *bit = bdd.or(*bit, rhs)?;
                    }
                }
            }
            for (i, rhs) in bits.into_iter().enumerate() {
                let physical = location.at + i;
                bdd.context.equation = Some(physical);
                if direct {
                    assignments.push((physical, rhs, 0));
                } else {
                    let post = bdd.var(physical, true)?;
                    factors.push((physical, bdd.equal(post, rhs)?));
                }
            }
        }
        bdd.context.equation = None;
        // Stable factor order is part of the comparison; location equations use
        // their physical coordinate order rather than old matrix enumeration.
        let size = if direct {
            assignments.len()
        } else {
            factors.len()
        };
        bdd.work
            .charge(size.saturating_mul(size.max(1).ilog2() as usize + 1))?;
        factors.sort_by_key(|(bit, _)| *bit);
        assignments.sort_by_key(|(bit, _, _)| *bit);
        Ok((factors, assignments, invalid))
    }

    pub fn initial(&self, values: &BTreeMap<usize, Option<bool>>, bdd: &mut Bdd) -> R<Id> {
        let mut physical = BTreeMap::new();
        bdd.work.charge(values.len() + self.physical.bits)?;
        for (&dense, &value) in values {
            match self.projections[dense] {
                Projection::Direct(bit) => {
                    physical.insert(bit, value);
                }
                Projection::Zero if value != Some(false) => {
                    return Err("Mismatch: initial token outside local domain".into());
                }
                _ => {}
            }
        }
        for location in &self.locations {
            let mut code = 0u32;
            for (index, &dense) in location.dense_bits.iter().enumerate() {
                bdd.work.charge(1)?;
                match values.get(&dense) {
                    Some(Some(true)) if code == 0 => code = index as u32 + 1,
                    Some(Some(true)) => {
                        return Err("InternalError: initial ownership multiplicity".into());
                    }
                    Some(Some(false)) => {}
                    _ => return Err("Unsupported: nondeterministic initial incidence".into()),
                }
            }
            for i in 0..location.width {
                physical.insert(location.at + i, Some(code >> i & 1 != 0));
            }
        }
        if physical.len() != self.physical.bits {
            return Err("InternalError: incomplete initial cube".into());
        }
        bdd.cube(&physical)
    }

    pub fn encode(&self, row: &[u64], work: &mut Work) -> R<Vec<u64>> {
        if row.len() != self.projections.len().div_ceil(64) {
            return Err("Mismatch: dense row width".into());
        }
        work.charge(self.projections.len() + self.physical.bits)?;
        let mut result = vec![0; self.physical.bits.div_ceil(64)];
        for (dense, projection) in self.projections.iter().enumerate() {
            match *projection {
                Projection::Direct(physical) => put(&mut result, physical, get(row, dense)),
                Projection::Zero if get(row, dense) => {
                    return Err("Mismatch: oracle token outside local domain".into());
                }
                _ => {}
            }
        }
        for location in &self.locations {
            let mut code = 0u32;
            for (owner, &dense) in location.dense_bits.iter().enumerate() {
                work.charge(1)?;
                if get(row, dense) {
                    if code != 0 {
                        return Err("Mismatch: oracle prestate ownership multiplicity".into());
                    }
                    code = owner as u32 + 1;
                }
            }
            for i in 0..location.width {
                put(&mut result, location.at + i, code >> i & 1 != 0);
            }
        }
        Ok(result)
    }

    pub fn decode(&self, row: &[u64], work: &mut Work) -> R<Vec<u64>> {
        if row.len() != self.physical.bits.div_ceil(64) {
            return Err("Mismatch: physical row width".into());
        }
        work.charge(self.projections.len() + self.physical.bits)?;
        let mut codes = Vec::new();
        for location in &self.locations {
            let mut code = 0;
            for i in 0..location.width {
                code |= u32::from(get(row, location.at + i)) << i;
            }
            if code as usize > location.dense_bits.len() {
                return Err("Mismatch: unused location code reachable".into());
            }
            codes.push(code);
        }
        let mut result = vec![0; self.projections.len().div_ceil(64)];
        for (dense, projection) in self.projections.iter().enumerate() {
            let bit = match *projection {
                Projection::Zero => false,
                Projection::Direct(physical) => get(row, physical),
                Projection::Incident { location, code } => codes[location] == code,
            };
            put(&mut result, dense, bit);
        }
        Ok(result)
    }
}
fn get(row: &[u64], bit: usize) -> bool {
    row[bit / 64] >> (bit % 64) & 1 != 0
}
fn put(row: &mut [u64], bit: usize, value: bool) {
    let mask = 1 << (bit % 64);
    if value {
        row[bit / 64] |= mask;
    } else {
        row[bit / 64] &= !mask;
    }
}
