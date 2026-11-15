//! Deterministic, project-local CSS identifier allocation.
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Names {
    pub length: usize,
    entries: BTreeMap<(String, String, String), String>,
}
impl Default for Names {
    fn default() -> Self {
        Self {
            length: 3,
            entries: BTreeMap::new(),
        }
    }
}
impl Names {
    pub fn allocate(
        length: usize,
        identities: BTreeSet<(String, String, String)>,
        reserved: &BTreeSet<(String, String)>,
    ) -> Result<Self, String> {
        if !(1..=16).contains(&length) {
            return Err("suffixLength must be an integer from 1 through 16".into());
        }
        let mut entries = BTreeMap::new();
        let mut counters = BTreeMap::<(String, String), u128>::new();
        let mut used = reserved.clone();
        for (namespace, base, identity) in identities {
            let counter = counters
                .entry((namespace.clone(), base.clone()))
                .or_default();
            loop {
                if *counter >= 36u128.pow(length as u32) {
                    return Err(format!(
                        "Scoped name `{base}` exhausted its suffixes; increase suffixLength"
                    ));
                }
                let mut n = *counter;
                let mut suffix = vec![b'0'; length];
                for digit in suffix.iter_mut().rev() {
                    *digit = b"0123456789abcdefghijklmnopqrstuvwxyz"[(n % 36) as usize];
                    n /= 36;
                }
                *counter += 1;
                let name = format!("{base}_{}", String::from_utf8(suffix).unwrap());
                if used.insert((namespace.clone(), name.clone())) {
                    entries.insert((namespace, base, identity), name);
                    break;
                }
            }
        }
        Ok(Self { length, entries })
    }
    pub fn get(&self, namespace: &str, base: &str, identity: &str) -> Option<&str> {
        self.entries
            .get(&(namespace.into(), base.into(), identity.into()))
            .map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counters_are_per_name_and_skip_globals() {
        let ids = [
            ("class", "button", "b"),
            ("class", "card", "a"),
            ("class", "button", "a"),
        ]
        .into_iter()
        .map(|(a, b, c)| (a.into(), b.into(), c.into()))
        .collect();
        let reserved = BTreeSet::from([("class".into(), "button_000".into())]);
        let names = Names::allocate(3, ids, &reserved).unwrap();
        assert_eq!(names.get("class", "button", "a"), Some("button_001"));
        assert_eq!(names.get("class", "button", "b"), Some("button_002"));
        assert_eq!(names.get("class", "card", "a"), Some("card_000"));
    }
    #[test]
    fn base36_and_capacity() {
        let ids = (0..36)
            .map(|i| ("class".into(), "button".into(), format!("{i:03}")))
            .collect();
        let names = Names::allocate(1, ids, &BTreeSet::new()).unwrap();
        assert_eq!(names.get("class", "button", "010"), Some("button_a"));
        assert_eq!(names.get("class", "button", "035"), Some("button_z"));
        let ids = (0..37)
            .map(|i| ("class".into(), "button".into(), format!("{i:03}")))
            .collect();
        assert!(
            Names::allocate(1, ids, &BTreeSet::new())
                .unwrap_err()
                .contains("increase suffixLength")
        );
    }
}
