//! Learns normal PATH shape across samples and classifies anomalies.

use std::collections::HashMap;

/// How strictly AUTO mode should enforce the learned baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sensitivity {
    /// Only act when entries expected in *every* sample go missing, or when a
    /// brand-new entry appears. Low false-positive rate.
    Conservative,
    /// Enforce the baseline exactly: any entry not seen before is unexpected,
    /// any expected entry missing is a problem.
    Aggressive,
}

/// The result of classifying a live PATH against the baseline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaselineVerdict {
    /// True if the current PATH deviates enough to warrant healing attention.
    pub anomaly: bool,
    /// Entries present now that were never seen in any baseline sample.
    pub unexpected_entries: Vec<String>,
    /// Entries seen in (nearly) every sample but missing now.
    pub missing_expected: Vec<String>,
}

/// A learned model of "normal" PATH shape for one machine.
#[derive(Debug, Clone)]
pub struct Baseline {
    lengths: Vec<usize>,
    /// entry (lowercased) -> number of samples it appeared in.
    presence: HashMap<String, usize>,
    samples: usize,
    sensitivity: Sensitivity,
}

impl Baseline {
    pub fn new(sensitivity: Sensitivity) -> Self {
        Self {
            lengths: Vec::new(),
            presence: HashMap::new(),
            samples: 0,
            sensitivity,
        }
    }

    /// Number of baseline samples recorded so far.
    pub fn samples(&self) -> usize {
        self.samples
    }

    /// Average PATH length across recorded samples (0 if none).
    pub fn average_length(&self) -> f64 {
        if self.lengths.is_empty() {
            0.0
        } else {
            self.lengths.iter().sum::<usize>() as f64 / self.lengths.len() as f64
        }
    }

    /// Records one observed PATH as a baseline sample.
    pub fn record(&mut self, path: &str) {
        let entries: Vec<String> = path
            .split(';')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_lowercase())
            .collect();
        self.lengths.push(entries.len());
        for e in &entries {
            *self.presence.entry(e.clone()).or_insert(0) += 1;
        }
        self.samples += 1;
    }

    /// Classifies `current` against the baseline.
    ///
    /// Requires at least one sample to produce a meaningful verdict; with zero
    /// samples it returns "not an anomaly" with empty diffs.
    pub fn classify(&self, current: &str) -> BaselineVerdict {
        let current_entries: Vec<String> = current
            .split(';')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_lowercase())
            .collect();
        let current_set: std::collections::HashSet<String> =
            current_entries.iter().cloned().collect();

        let mut unexpected_entries = Vec::new();
        for e in &current_entries {
            if !self.presence.contains_key(e) {
                unexpected_entries.push(e.clone());
            }
        }

        let mut missing_expected = Vec::new();
        // Without any baseline sample we cannot judge what is "expected".
        if self.samples == 0 {
            return BaselineVerdict {
                anomaly: false,
                unexpected_entries: Vec::new(),
                missing_expected: Vec::new(),
            };
        }
        if self.samples > 0 {
            for (entry, count) in &self.presence {
                let expected = match self.sensitivity {
                    Sensitivity::Conservative => *count == self.samples,
                    Sensitivity::Aggressive => *count >= 1,
                };
                if expected && !current_set.contains(entry) {
                    missing_expected.push(entry.clone());
                }
            }
        }

        let anomaly = !unexpected_entries.is_empty() || !missing_expected.is_empty();
        BaselineVerdict {
            anomaly,
            unexpected_entries,
            missing_expected,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn empty_baseline_is_not_anomalous() {
        let b = Baseline::new(Sensitivity::Conservative);
        let v = b.classify(r"C:\A;C:\B");
        assert!(!v.anomaly);
    }

    #[test]
    fn conservative_flags_only_universal_missing() {
        let mut b = Baseline::new(Sensitivity::Conservative);
        b.record(r"C:\A;C:\B;C:\C");
        b.record(r"C:\A;C:\B;C:\D"); // C present in 1/2, D in 1/2 → neither universal
        b.record(r"C:\A;C:\B;C:\E"); // E in 1/3

        // A and B present in all 3 → expected. C/D/E only partial → not "universal".
        let v = b.classify(r"C:\A;C:\B"); // C, D, E missing but not universal
        assert!(!v.anomaly);
        assert!(v.missing_expected.is_empty());
    }

    #[test]
    fn conservative_flags_universal_missing() {
        let mut b = Baseline::new(Sensitivity::Conservative);
        b.record(r"C:\A;C:\B");
        b.record(r"C:\A;C:\B");
        let v = b.classify(r"C:\A"); // B was universal, now missing
        assert!(v.anomaly);
        assert_eq!(v.missing_expected, vec!["c:\\b".to_string()]);
    }

    #[test]
    fn aggressive_flags_any_missing_or_unexpected() {
        let mut b = Baseline::new(Sensitivity::Aggressive);
        b.record(r"C:\A;C:\B");
        let v = b.classify(r"C:\A;C:\X"); // B missing, X unexpected
        assert!(v.anomaly);
        assert_eq!(v.missing_expected, vec!["c:\\b".to_string()]);
        assert_eq!(v.unexpected_entries, vec!["c:\\x".to_string()]);
    }

    #[test]
    fn average_length_tracks_samples() {
        let mut b = Baseline::new(Sensitivity::Conservative);
        b.record(r"C:\A;C:\B");
        b.record(r"C:\A;C:\B;C:\C");
        assert_eq!(b.samples(), 2);
        assert!((b.average_length() - 2.5).abs() < 1e-9);
    }

    proptest! {
        #[test]
        fn classify_never_panics(
            ref samples in prop::collection::vec("[a-zA-Z]:\\\\[a-zA-Z0-9_]+(;[a-zA-Z]:\\\\[a-zA-Z0-9_]+)*", 0..5),
            ref current in "[a-zA-Z]:\\\\[a-zA-Z0-9_]+(;[a-zA-Z]:\\\\[a-zA-Z0-9_]+)*",
            sensitivity in 0..1,
        ) {
            let s = if sensitivity == 0 { Sensitivity::Conservative } else { Sensitivity::Aggressive };
            let mut b = Baseline::new(s);
            for sample in samples {
                b.record(sample);
            }
            let v = b.classify(current);
            // anomaly is true iff there are unexpected_entries or missing_expected
            prop_assert_eq!(v.anomaly, !v.unexpected_entries.is_empty() || !v.missing_expected.is_empty());
        }

        #[test]
        fn recorded_entry_never_appears_as_unexpected(
            ref recorded in "[a-zA-Z]:\\\\[a-zA-Z0-9_]+(;[a-zA-Z]:\\\\[a-zA-Z0-9_]+)*",
            sensitivity in 0..1,
        ) {
            let s = if sensitivity == 0 { Sensitivity::Conservative } else { Sensitivity::Aggressive };
            let mut b = Baseline::new(s);
            b.record(recorded);
            let v = b.classify(recorded);
            // All entries from the recorded path appear in the baseline → none should be unexpected
            for entry in recorded.split(';').filter(|e| !e.is_empty()) {
                prop_assert!(
                    !v.unexpected_entries.iter().any(|u| u.eq_ignore_ascii_case(entry)),
                    "recorded entry '{}' appeared as unexpected in classify({})", entry, recorded
                );
            }
        }

        #[test]
        fn average_length_is_sane(
            ref samples in prop::collection::vec("[a-zA-Z]:\\\\[a-zA-Z0-9_]+(;[a-zA-Z]:\\\\[a-zA-Z0-9_]+)*", 0..10),
        ) {
            let mut b = Baseline::new(Sensitivity::Conservative);
            for sample in samples {
                b.record(sample);
            }
            let avg = b.average_length();
            prop_assert!(avg.is_finite());
            prop_assert!(avg >= 0.0);
            if b.samples() > 0 {
                prop_assert!(avg > 0.0, "with samples avg must be > 0");
            }
        }

        #[test]
        fn classify_never_leaks_unexpected_from_empty_baseline(
            ref current in "[a-zA-Z]:\\\\[a-zA-Z0-9_]+(;[a-zA-Z]:\\\\[a-zA-Z0-9_]+)*",
            sensitivity in 0..1,
        ) {
            let s = if sensitivity == 0 { Sensitivity::Conservative } else { Sensitivity::Aggressive };
            let b = Baseline::new(s);
            let v = b.classify(current);
            prop_assert!(!v.anomaly, "empty baseline must never flag anomaly");
            prop_assert!(v.unexpected_entries.is_empty());
            prop_assert!(v.missing_expected.is_empty());
        }
    }
}
