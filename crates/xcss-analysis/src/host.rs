use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UpdateError {
    StaleRevision,
    InvalidModuleId,
}

#[derive(Default)]
pub struct AnalysisHost {
    files: BTreeMap<FileId, Arc<ParseResult>>,
    revisions: BTreeMap<FileId, u64>,
    resolutions: BTreeMap<(FileId, String), Option<FileId>>,
    generation: u64,
    cached: Option<AnalysisSnapshot>,
}
impl AnalysisHost {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn set_file(
        &mut self,
        id: FileId,
        revision: u64,
        text: impl Into<Arc<str>>,
    ) -> Result<(), UpdateError> {
        if id.0.is_empty() || id.0.contains('\\') {
            return Err(UpdateError::InvalidModuleId);
        }
        if self.revisions.get(&id).is_some_and(|&old| revision <= old) {
            return Err(UpdateError::StaleRevision);
        }
        let parsed = Arc::new(xcss_syntax::parse(Arc::new(SourceFile::new(
            id.clone(),
            revision,
            text,
        ))));
        self.files.insert(id.clone(), parsed);
        self.revisions.insert(id, revision);
        self.changed();
        Ok(())
    }
    pub fn remove_file(&mut self, id: &FileId, revision: u64) -> Result<(), UpdateError> {
        if self.revisions.get(id).is_some_and(|&old| revision <= old) {
            return Err(UpdateError::StaleRevision);
        }
        self.files.remove(id);
        self.revisions.insert(id.clone(), revision);
        self.resolutions.retain(|(file, _), _| file != id);
        self.changed();
        Ok(())
    }
    pub fn set_resolution(
        &mut self,
        importer: FileId,
        specifier: impl Into<String>,
        target: Option<FileId>,
    ) {
        self.resolutions
            .insert((importer, specifier.into()), target);
        self.changed();
    }
    fn changed(&mut self) {
        self.generation += 1;
        self.cached = None;
    }
    pub fn snapshot(&mut self) -> AnalysisSnapshot {
        if let Some(snapshot) = &self.cached {
            return snapshot.clone();
        }
        let snapshot = AnalysisSnapshot(Arc::new(analyze(
            self.generation,
            self.files.clone(),
            &self.resolutions,
        )));
        self.cached = Some(snapshot.clone());
        snapshot
    }
}

#[derive(Clone)]
pub struct AnalysisSnapshot(Arc<Database>);
impl AnalysisSnapshot {
    pub fn imports(&self) -> &[ImportBinding] {
        &self.0.imports
    }
    pub fn variable_registration(&self, id: SymbolId) -> Option<VariableRegistration> {
        let symbol = self.symbol(id)?;
        if symbol.kind != SymbolKind::Variable {
            return None;
        }
        let parsed = self.syntax(&symbol.location.file)?;
        let syntax = parsed.tree.descendants().find_map(|n| {
            parsed
                .variable(n)
                .filter(|v| v.range == symbol.location.range)
        })?;
        Some(VariableRegistration {
            symbol: id,
            location: symbol.location.clone(),
            value_type: symbol.value_type.clone(),
            initial: syntax.initial.map(str::to_owned),
            inherits: syntax.inherits,
        })
    }
    pub fn files(&self) -> impl Iterator<Item = &FileId> {
        self.0.files.keys()
    }
    pub fn compositions(&self) -> &[ResolvedComposition] {
        &self.0.compositions
    }
    pub fn resolved_call(&self, file: &FileId, node: &Node) -> Option<SymbolId> {
        let syntax = self.syntax(file)?.call(node)?;
        self.0
            .references
            .iter()
            .find(|r| r.location.file == *file && r.location.range == syntax.name_range)
            .and_then(|r| r.target)
    }
    pub fn injection_definition(&self, id: SymbolId) -> Option<(&ParseResult, &Node)> {
        let symbol = self.symbol(id)?;
        if symbol.kind != SymbolKind::Injection {
            return None;
        }
        let parse = self.syntax(&symbol.location.file)?;
        let node = parse.tree.descendants().find(|n| {
            parse
                .injection(n)
                .is_some_and(|i| i.name_range == symbol.location.range)
        })?;
        Some((parse, node))
    }
    pub fn revision(&self) -> u64 {
        self.0.generation
    }
    pub fn syntax(&self, file: &FileId) -> Option<&ParseResult> {
        self.0.files.get(file).map(Arc::as_ref)
    }
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.0.diagnostics
    }
    pub fn symbols(&self) -> &[Symbol] {
        &self.0.symbols
    }
    pub fn scopes(&self) -> &[Scope] {
        &self.0.scopes
    }
    pub fn references(&self) -> &[Reference] {
        &self.0.references
    }
    pub fn dependencies(&self) -> &[Dependency] {
        &self.0.dependencies
    }
    pub fn expressions(&self) -> &[Expression] {
        &self.0.expressions
    }
    pub fn symbol(&self, id: SymbolId) -> Option<&Symbol> {
        self.0.symbols.get(id.0)
    }
    pub fn symbol_at(&self, file: &FileId, offset: usize) -> Option<&Symbol> {
        self.0
            .symbols
            .iter()
            .find(|s| &s.location.file == file && s.location.range.contains(offset))
            .or_else(|| {
                self.0
                    .references
                    .iter()
                    .filter(|r| &r.location.file == file && r.location.range.contains(offset))
                    .min_by_key(|r| r.location.range.end - r.location.range.start)
                    .and_then(|r| r.target)
                    .and_then(|id| self.symbol(id))
            })
    }
    pub fn references_to(&self, id: SymbolId) -> impl Iterator<Item = &Reference> {
        self.0
            .references
            .iter()
            .filter(move |r| r.target == Some(id))
    }
    pub fn signature(&self, id: SymbolId) -> Option<&InjectionSignature> {
        self.0.signatures.get(&id)
    }
    pub fn type_at(&self, file: &FileId, offset: usize) -> Option<&ValueType> {
        self.symbol_at(file, offset)
            .map(|s| &s.value_type)
            .or_else(|| {
                self.0
                    .expressions
                    .iter()
                    .find(|e| &e.location.file == file && e.location.range.contains(offset))
                    .map(|e| &e.value_type)
            })
    }
}
