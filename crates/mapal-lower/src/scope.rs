//! The scope stack and binding discipline (DESIGN §3). A scope stack maps
//! `name → payload`; the payload is generic so the effects pass (Pass B) can
//! track names only while the typing/emission passes track full
//! `Binding { obj, ty, mutable, kind }`.
//!
//! Resolution searches innermost scope first (LD1). Variables shadow functions
//! (the function/`print` fallback lives in the caller, not here). Poisoning
//! (LD9/L1107) marks a carried name unreadable after its loop.

use std::collections::BTreeMap;

/// A lexical scope: name → payload, plus per-scope poison marks.
#[derive(Clone)]
struct Scope<T> {
    bindings: BTreeMap<String, T>,
    /// Names poisoned in *this* scope (read → L1107). Poisoning a name does not
    /// remove its binding (the object still exists); it forbids reads.
    poisoned: BTreeMap<String, ()>,
}

impl<T> Scope<T> {
    fn new() -> Self {
        Scope {
            bindings: BTreeMap::new(),
            poisoned: BTreeMap::new(),
        }
    }
}

/// A stack of lexical scopes (DESIGN §3). The bottom is the function-body root.
#[derive(Clone)]
pub struct ScopeStack<T> {
    scopes: Vec<Scope<T>>,
}

impl<T: Clone> Default for ScopeStack<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone> ScopeStack<T> {
    /// A stack with a single root scope.
    pub fn new() -> Self {
        ScopeStack {
            scopes: vec![Scope::new()],
        }
    }

    /// Push a fresh child scope.
    pub fn push(&mut self) {
        self.scopes.push(Scope::new());
    }

    /// Pop the innermost scope.
    pub fn pop(&mut self) {
        self.scopes.pop();
    }

    /// Bind `name` to `payload` in the innermost scope (shadowing any outer
    /// binding of the same name).
    pub fn bind(&mut self, name: &str, payload: T) {
        self.scopes
            .last_mut()
            .expect("non-empty scope stack")
            .bindings
            .insert(name.to_string(), payload);
    }

    /// Bind `name` into the scope just *below* the innermost one (the enclosing
    /// scope). Used for routing-guard exit-arm bindings, which the design places
    /// in the loop's enclosing scope, not the loop-body frame (DESIGN §8.5). If
    /// there is only one scope, binds into it.
    pub fn bind_enclosing(&mut self, name: &str, payload: T) {
        let idx = self.scopes.len().saturating_sub(2);
        self.scopes[idx].bindings.insert(name.to_string(), payload);
    }

    /// Resolve `name`, searching innermost scope first. Returns the payload and
    /// whether the binding is poisoned (reads of poisoned names → L1107).
    pub fn resolve(&self, name: &str) -> Option<(&T, bool)> {
        for scope in self.scopes.iter().rev() {
            if let Some(p) = scope.bindings.get(name) {
                let poisoned = scope.poisoned.contains_key(name);
                return Some((p, poisoned));
            }
        }
        None
    }

    /// Whether `name` resolves to a binding (poisoned or not) in any scope.
    pub fn contains(&self, name: &str) -> bool {
        self.scopes.iter().any(|s| s.bindings.contains_key(name))
    }

    /// Find the scope index (innermost-first) and rebind `name` to a new
    /// payload **in the scope that already holds it** (the one-definition-rule
    /// fresh-object rebind: the symbol rebinds where it lives, DESIGN §3).
    /// Returns `false` if the name is not bound anywhere.
    pub fn rebind_existing(&mut self, name: &str, payload: T) -> bool {
        for scope in self.scopes.iter_mut().rev() {
            if scope.bindings.contains_key(name) {
                scope.bindings.insert(name.to_string(), payload);
                return true;
            }
        }
        false
    }

    /// Poison `name` in whichever scope holds it (LD9): subsequent reads are
    /// L1107. No-op if unbound.
    pub fn poison(&mut self, name: &str) {
        for scope in self.scopes.iter_mut().rev() {
            if scope.bindings.contains_key(name) {
                scope.poisoned.insert(name.to_string(), ());
                return;
            }
        }
    }

    /// A snapshot of the whole stack (DESIGN §3 routing-guard snapshot). Cloning
    /// the stack gives each arm an independent binding view; arm-local rebinds
    /// do not escape.
    pub fn snapshot(&self) -> Self {
        self.clone()
    }
}
