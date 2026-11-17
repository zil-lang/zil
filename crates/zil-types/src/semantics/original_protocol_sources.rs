//! Exact original input projections for ordinary owning callable leaves.
//! These paths identify a source within one actual parameter/capture; they
//! grant neither cleanup capability nor equality between separate occupants.
use super::*;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Step {
    Field(String),
    Error(bool),
    Variant(String, usize),
    Boxed,
}
impl Step {
    fn work(&self) -> usize {
        match self { Self::Field(name) | Self::Variant(name, _) => name.len().saturating_add(1), _ => 1 }
    }
}
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Projection {
    root: Root,
    path: Vec<Step>,
}
impl Projection {
    fn work(&self) -> usize { self.path.iter().fold(1usize, |n, step| n.saturating_add(step.work())) }
}
impl Engine<'_, '_> {
    pub(super) fn input_source_root(&mut self, source: Root, depth: usize) -> R<Root> {
        self.pay(1, depth)?;
        if let Root::Projection(owner, index) = source {
            let projection = self.projected_sources.get(index as usize).ok_or_else(|| {
                failure("consuming contract loses its original projection directory")
            })?;
            if !matches!(projection.root, Root::Parameter(domain, _) | Root::Capture(domain, _) if domain == owner)
            {
                return Err(failure(
                    "consuming contract changes its exact projection root domain",
                ));
            }
            Ok(projection.root)
        } else {
            Ok(source)
        }
    }
    fn projected_source(&mut self, root: Root, path: &[Step], depth: usize) -> R<Root> {
        if path.is_empty() { return Ok(root); }
        let (Root::Parameter(owner, _) | Root::Capture(owner, _)) = root else {
            return Err(failure("projected callable source has no exact original input root"));
        };
        let work = path.iter().fold(1usize, |n, step| n.saturating_add(step.work()));
        self.pay(work.saturating_mul(lookup_work(self.projected_ids.len()).saturating_add(3)), depth)?;
        let source = Projection { root, path: path.to_vec() };
        if let Some(index) = self.projected_ids.get(&source) { return Ok(Root::Projection(owner, *index)); }
        if self.projected_sources.len() >= self.limits.max_nodes {
            return Err(Diagnostic::limit(Phase::Types, "original input projection inventory exceeded", None));
        }
        let index = u32::try_from(self.projected_sources.len())
            .map_err(|_| failure("original input projection ordinal overflows"))?;
        self.projected_sources.push(source.clone());
        self.projected_ids.insert(source, index);
        Ok(Root::Projection(owner, index))
    }

    /// Called only while forming the fresh complete parameter/capture value.
    /// Ordinary moves, field replacement and partial consumption then carry
    /// the actual Value, so they cannot restore a consumed field's old source.
    pub(super) fn input_sources(
        &mut self, value: &mut Value, root: Root, path: &mut Vec<Step>, depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        match value {
            Value::Cleanup { source, .. } => {
                if source.is_some() { return Err(failure("fresh original input reuses an earlier callable occupant")); }
                *source = Some(self.projected_source(root, path, depth + 1)?);
            }
            Value::Fields(fields) => {
                for (name, value) in fields {
                    self.pay(name.len().saturating_add(1), depth)?;
                    path.push(Step::Field(name.clone()));
                    self.input_sources(value, root, path, depth + 1)?;
                    path.pop();
                }
            }
            Value::Tuple(values) => {
                for (index, value) in values.iter_mut().enumerate() {
                    self.pay(32, depth)?;
                    path.push(Step::Field(index.to_string()));
                    self.input_sources(value, root, path, depth + 1)?;
                    path.pop();
                }
            }
            Value::Choice(values) => {
                for value in values { self.input_sources(value, root, path, depth + 1)?; }
            }
            Value::Error { is_error, payload } => {
                self.pay(1, depth)?; path.push(Step::Error(*is_error));
                self.input_sources(payload, root, path, depth + 1)?;
                path.pop();
            }
            Value::Variant { name, fields } => {
                for (index, value) in fields.iter_mut().enumerate() {
                    self.pay(name.len().saturating_add(1), depth)?;
                    path.push(Step::Variant(name.clone(), index));
                    self.input_sources(value, root, path, depth + 1)?;
                    path.pop();
                }
            }
            Value::Boxed(value) => {
                self.pay(1, depth)?; path.push(Step::Boxed);
                self.input_sources(value, root, path, depth + 1)?;
                path.pop();
            }
            // Genuine Task/registration roots retain their independent root
            // and generation directory. Borrowed/Copy values have no debt.
            _ => {},
        }
        Ok(())
    }

    pub(super) fn variant_input_sources(
        &mut self, value: &mut Value, source: Root, variant: &str, index: usize, depth: usize,
    ) -> R<()> {
        let (root, mut path) = if let Root::Projection(owner, ordinal) = source {
            let projection = self.projected_sources.get(ordinal as usize)
                .ok_or_else(|| failure("variant callable source omits its original projection"))?;
            if !matches!(projection.root, Root::Parameter(domain, _) | Root::Capture(domain, _) if domain == owner) {
                return Err(failure("variant callable source changes its original domain"));
            }
            let work = projection.work(); self.pay(work, depth)?;
            let projection = self.projected_sources[ordinal as usize].clone();
            (projection.root, projection.path)
        } else { (source, Vec::new()) };
        self.pay(variant.len().saturating_add(1), depth)?;
        path.push(Step::Variant(variant.to_owned(), index));
        self.input_sources(value, root, &mut path, depth + 1)
    }

    pub(super) fn original_input(
        &mut self, source: Root, target: SemanticDefinitionId, arguments: &[Value],
        captures: Option<&BTreeMap<String, Value>>, depth: usize,
    ) -> R<Option<Value>> {
        self.pay(1, depth)?;
        let (root, path) = if let Root::Projection(owner, index) = source {
            if owner != target { return Err(failure("callable projection changes its original definition domain")); }
            let projection = self.projected_sources.get(index as usize)
                .ok_or_else(|| failure("callable projection omits its original source directory"))?;
            let work = projection.work(); self.pay(work, depth)?;
            let projection = self.projected_sources[index as usize].clone();
            (projection.root, projection.path)
        } else { (source, Vec::new()) };
        let value = match root {
            Root::Parameter(owner, ordinal) if owner == target => arguments.get(ordinal as usize),
            Root::Capture(owner, ordinal) if owner == target => {
                self.pay(lookup_work(self.proof.capture_names.len()), depth)?;
                self.proof.capture_names.get(&target)
                    .and_then(|names| names.get(ordinal as usize))
                    .and_then(|name| captures.and_then(|captures| captures.get(name)))
            }
            _ => return Err(failure("callable input source changes its exact original parameter or capture")),
        }.ok_or_else(|| pending("callable cleanup omits its complete actual input correspondence"))?;
        self.projected_input(value, &path, depth + 1)
    }

    fn projected_input(&mut self, value: &Value, path: &[Step], depth: usize) -> R<Option<Value>> {
        self.pay(1, depth)?;
        let Some((step, rest)) = path.split_first() else { return self.value_copy(value, depth + 1).map(Some); };
        if let Value::Choice(values) = value {
            let mut selected = Vec::new();
            for value in values {
                if let Some(value) = self.projected_input(value, path, depth + 1)? {
                    self.pay(1, depth)?; selected.push(value);
                }
            }
            return if selected.is_empty() { Ok(None) } else { self.choice(selected, depth + 1).map(Some) };
        }
        self.pay(step.work(), depth)?;
        let selected = match (step, value) {
            (Step::Field(name), Value::Fields(fields)) => {
                self.pay(name.len().saturating_mul(lookup_work(fields.len())), depth)?;
                fields.get(name)
            }
            (Step::Field(name), Value::Tuple(values)) => name.parse::<usize>().ok().and_then(|index| values.get(index)),
            (Step::Error(wanted), Value::Error { is_error, payload }) => {
                if wanted != is_error { return Ok(None); } Some(payload.as_ref())
            }
            (Step::Variant(wanted, index), Value::Variant { name, fields }) => {
                self.pay(name.len(), depth)?;
                if wanted != name { return Ok(None); } fields.get(*index)
            }
            (Step::Boxed, Value::Boxed(value)) => Some(value.as_ref()),
            _ => None,
        }.ok_or_else(|| pending("callable cleanup lacks its complete actual owning input projection"))?;
        self.projected_input(selected, rest, depth + 1)
    }
}
