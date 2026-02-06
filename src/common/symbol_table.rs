use crate::common::types::CType;
use crate::common::source::Span;
use crate::common::fx_hash::FxHashMap;

/// Linkage of a file-scope symbol per C11 6.2.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Linkage {
    /// No storage-class specifier at file scope → external linkage.
    External,
    /// Declared with `static` → internal linkage.
    Internal,
    /// Declared with `extern` (explicitly).
    Extern,
    /// Block-scope or other (no linkage tracking needed).
    None,
}

/// Information about a declared symbol.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub ty: CType,
    /// Explicit alignment from _Alignas or __attribute__((aligned(N))).
    /// Used by _Alignof(var) to return the correct alignment per C11 6.2.8p3.
    pub explicit_alignment: Option<usize>,
    /// Linkage as determined by storage-class specifiers at the declaration site.
    pub linkage: Linkage,
    /// Whether this symbol was declared with the `const` qualifier.
    pub is_const: bool,
    /// Source span of the declaration, used for -Wshadow notes.
    pub span: Option<Span>,
}

/// A scope in the symbol table.
#[derive(Debug)]
struct Scope {
    symbols: FxHashMap<String, Symbol>,
}

impl Scope {
    fn new() -> Self {
        Self { symbols: FxHashMap::default() }
    }
}

/// Scoped symbol table supporting nested lexical scopes.
#[derive(Debug)]
pub struct SymbolTable {
    scopes: Vec<Scope>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self { scopes: vec![Scope::new()] }
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(Scope::new());
    }

    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    pub fn declare(&mut self, symbol: Symbol) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.symbols.insert(symbol.name.clone(), symbol);
        }
    }

    pub fn lookup(&self, name: &str) -> Option<&Symbol> {
        for scope in self.scopes.iter().rev() {
            if let Some(sym) = scope.symbols.get(name) {
                return Some(sym);
            }
        }
        None
    }

    /// Look up a name in all scopes except the current (innermost) one.
    /// Used by -Wshadow to detect when a new declaration shadows an outer one.
    pub fn lookup_outer(&self, name: &str) -> Option<&Symbol> {
        if self.scopes.len() < 2 {
            return None;
        }
        for scope in self.scopes[..self.scopes.len() - 1].iter().rev() {
            if let Some(sym) = scope.symbols.get(name) {
                return Some(sym);
            }
        }
        None
    }

}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}
