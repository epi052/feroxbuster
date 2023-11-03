use std::borrow::BorrowMut;

/// single word term for text processing
#[derive(Debug, Hash, Eq, PartialEq, Default, Clone)]
pub(crate) struct Term {
    /// underlying string that the term represents
    raw: String,
}

impl Term {
    /// given a word, create a new `Term`
    pub(super) fn new(word: &str) -> Self {
        Self {
            raw: word.to_owned(),
        }
    }

    /// return a reference to the underlying string
    pub(super) fn raw(&self) -> &str {
        &self.raw
    }
}

/// metadata to be associated with a `Term`
#[derive(Debug, Clone, Default)]
pub(super) struct TermMetaData {
    /// number of times the associated `Term` was seen in a single document
    count: u32,

    /// collection of term frequencies for the associated `Term`
    term_frequencies: Vec<f32>,

    /// tf-idf score for the associated `Term`
    tf_idf_score: f32,
}

impl TermMetaData {
    /// number of times a `Term` has appeared in any `Document` within the corpus
    pub(super) fn document_frequency(&self) -> usize {
        self.term_frequencies().len()
    }

    /// mutable reference to the collection of term frequencies
    pub(super) fn term_frequencies_mut(&mut self) -> &mut Vec<f32> {
        self.term_frequencies.borrow_mut()
    }

    /// immutable reference to the collection of term frequencies
    pub(super) fn term_frequencies(&self) -> &[f32] {
        &self.term_frequencies
    }

    /// mutable reference to the number of times a `Term` was seen in a particular `Document`
    pub(super) fn count_mut(&mut self) -> &mut u32 {
        self.count.borrow_mut()
    }

    /// number of times a `Term` was seen in a particular `Document`
    pub(super) fn count(&self) -> u32 {
        self.count
    }

    /// mutable reference to the term's tf-idf score
    pub(super) fn tf_idf_score_mut(&mut self) -> &mut f32 {
        self.tf_idf_score.borrow_mut()
    }

    /// immutable reference to the term's tf-idf score
    pub(super) fn tf_idf_score(&self) -> f32 {
        self.tf_idf_score
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// test accessors for correctness
    fn nlp_term_accessor_test() {
        let term = Term::new("stuff");
        assert_eq!(term.raw(), "stuff");
    }

    #[test]
    /// test accessors for correctness
    fn nlp_term_metadata_accessor_test() {
        let mut metadata = TermMetaData::default();

        *metadata.count_mut() += 1;
        assert_eq!(metadata.count(), 1);

        metadata.term_frequencies_mut().push(1.0);
        assert_eq!(metadata.document_frequency(), 1);
        assert_eq!(metadata.term_frequencies().first().unwrap(), &1.0);

        *metadata.tf_idf_score_mut() = 1.0_f32;
        assert_eq!(metadata.tf_idf_score(), 1.0);
    }
}
