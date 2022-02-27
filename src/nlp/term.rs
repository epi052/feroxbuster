use std::borrow::BorrowMut;

/// single word term for text processing
#[derive(Debug, Hash, Eq, PartialEq, Default, Clone)]
pub struct Term {
    /// underlying string that the term represents
    raw: String,
}

impl Term {
    /// given a word, create a new `Term`
    pub fn new(word: &str) -> Self {
        Self {
            raw: word.to_owned(),
        }
    }

    /// return a reference to the underlying string
    pub fn raw(&self) -> &str {
        &self.raw
    }
}

/// metadata to be associated with a `Term`
#[derive(Debug, Clone, Default)]
pub struct TermMetaData {
    /// number of times the associated `Term` was seen in a single document
    count: u32,

    /// collection of term frequencies for the associated `Term`
    term_frequencies: Vec<f32>,

    /// collection of tf-idf scores for the associated `Term`
    tf_idf_scores: Vec<f32>,
}

impl TermMetaData {
    /// create a new metadata container
    pub fn new() -> Self {
        Self::default()
    }

    /// number of times a `Term` has appeared in any `Document` within the corpus
    pub fn document_frequency(&self) -> usize {
        self.term_frequencies().len()
    }

    /// mutable reference to the collection of term frequencies
    pub fn term_frequencies_mut(&mut self) -> &mut Vec<f32> {
        self.term_frequencies.borrow_mut()
    }

    /// immutable reference to the collection of term frequencies
    pub fn term_frequencies(&self) -> &[f32] {
        &self.term_frequencies
    }

    /// mutable reference to the number of times a `Term` was seen in a particular `Document`
    pub fn count_mut(&mut self) -> &mut u32 {
        self.count.borrow_mut()
    }

    /// number of times a `Term` was seen in a particular `Document`
    pub fn count(&self) -> u32 {
        self.count
    }

    /// mutable reference to the collection of tf-idf scores
    pub fn tf_idf_scores_mut(&mut self) -> &mut Vec<f32> {
        self.tf_idf_scores.borrow_mut()
    }

    /// immutable reference to the collection of tf-idf scores
    pub fn tf_idf_scores(&self) -> &[f32] {
        &self.tf_idf_scores
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
        let mut metadata = TermMetaData::new();

        *metadata.count_mut() += 1;
        assert_eq!(metadata.count(), 1);

        metadata.term_frequencies_mut().push(1.0);
        assert_eq!(metadata.document_frequency(), 1);
        assert_eq!(metadata.term_frequencies().first().unwrap(), &1.0);

        metadata.tf_idf_scores_mut().push(1.0);
        assert_eq!(metadata.tf_idf_scores().first().unwrap(), &1.0);
    }
}
