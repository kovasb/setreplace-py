//! Index from atoms to the tokens that contain them (libSetReplace's
//! `AtomsIndex`). Only tokens that are alive *and* eligible for matching are
//! kept in the index; consumed tokens are removed.

use std::collections::{BTreeSet, HashMap};

use crate::rule::Atom;
use crate::system::TokenId;

#[derive(Default)]
pub(crate) struct AtomsIndex {
    index: HashMap<Atom, BTreeSet<TokenId>>,
}

impl AtomsIndex {
    pub(crate) fn add_tokens<'a>(
        &mut self,
        ids: &[TokenId],
        atoms_of: impl Fn(TokenId) -> &'a [Atom],
    ) {
        for &id in ids {
            for &atom in atoms_of(id) {
                self.index.entry(atom).or_default().insert(id);
            }
        }
    }

    pub(crate) fn remove_tokens<'a>(
        &mut self,
        ids: &[TokenId],
        atoms_of: impl Fn(TokenId) -> &'a [Atom],
    ) {
        for &id in ids {
            for &atom in atoms_of(id) {
                if let Some(set) = self.index.get_mut(&atom) {
                    set.remove(&id);
                    if set.is_empty() {
                        self.index.remove(&atom);
                    }
                }
            }
        }
    }

    pub(crate) fn tokens_containing(&self, atom: Atom) -> Option<&BTreeSet<TokenId>> {
        self.index.get(&atom)
    }
}
