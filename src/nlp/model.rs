use super::document::Document;
use super::term::{Term, TermMetaData};
use super::utils::{inverse_document_frequency, tf_idf_score};
use std::borrow::{Borrow, BorrowMut};
use std::collections::HashMap;

/// data container for the TF-IDF model
#[derive(Debug, Default)]
pub(crate) struct TfIdf {
    /// collection of `Term`s and their associated metadata
    terms: HashMap<Term, TermMetaData>,

    /// number of documents processed by the model
    num_documents: usize,
}

impl TfIdf {
    /// create an empty TF-IDF model; must be populated with `add_document` prior to use
    fn new() -> Self {
        Self {
            terms: HashMap::new(),
            num_documents: 0,
        }
    }

    /// accessor method for the collection of `Term`s and `TermMetaData`
    fn terms(&self) -> &HashMap<Term, TermMetaData> {
        self.terms.borrow()
    }

    /// add a `Document` to the model
    fn add_document(&mut self, document: Document) {
        // increment number of docs seen, since we don't preserve the document itself; this needs
        // to happen before calls to `self.inverse_document_frequency`, as it relies on the count
        // being up to date
        self.num_documents += 1;

        for (term, doc_metadata) in document.terms().iter() {
            // an incoming `Term` from a `Document` only has a valid `count` for that particular
            // document; need to get the term frequency while both are known/valid
            let term_frequency = document.term_frequency(term);

            let metadata = self
                .terms
                .entry(term.clone())
                .or_insert_with(|| doc_metadata.to_owned());

            metadata.term_frequencies_mut().push(term_frequency);
        }
    }

    /// (re)-calculate tf-idf scores for all terms, given the current number of documents
    ///
    /// # Notes
    ///
    /// old tf-idf scores are removed during calculations to keep new `Term`s at the same relative
    /// level as new ones WRT corpus size
    fn calculate_tf_idf_scores(&mut self) {
        for metadata in self.terms.borrow_mut().values_mut() {
            let num_frequencies = metadata.term_frequencies().len();

            // clear out old scores before recalculating
            metadata.tf_idf_scores_mut().clear();

            let mut to_add = Vec::with_capacity(num_frequencies);

            for frequency in metadata.term_frequencies() {
                let idf = inverse_document_frequency(
                    self.num_documents as f32,
                    metadata.document_frequency() as f32,
                );

                let score = tf_idf_score(*frequency, idf);
                to_add.push(score);
            }

            metadata.tf_idf_scores_mut().append(&mut to_add);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// helper for this test suite
    fn get_scores(word: &str, model: &TfIdf) -> Vec<f32> {
        model
            .terms()
            .get(&Term::new(word))
            .unwrap()
            .tf_idf_scores()
            .into()
    }

    #[test]
    /// given the example data at https://remykarem.github.io/tfidf-demo/, ensure the model
    /// produces the same results
    fn model_generates_expected_tf_idf_scores() {
        let one = "Air quality in the sunny island improved gradually throughout Wednesday.";
        let two =
            "Air quality in Singapore on Wednesday continued to get worse as haze hit the island.";
        let three = "The air quality in Singapore is monitored through a network of air monitoring stations located in different parts of the island";
        let four = "The air quality in Singapore got worse on Wednesday.";

        let docs = [one, two, three, four];
        let mut model = TfIdf::new();

        for doc in docs.iter() {
            let d = Document::new(doc);
            model.add_document(d);
        }

        assert_eq!(model.terms().len(), 19);

        model.calculate_tf_idf_scores();

        assert_eq!(get_scores("quality", &model), [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(get_scores("air", &model), [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(
            get_scores("wednesday", &model),
            [0.017848395, 0.013882084, 0.024987752]
        );
        assert_eq!(
            get_scores("island", &model),
            [0.017848395, 0.013882084, 0.010411563]
        );
        assert_eq!(
            get_scores("singapore", &model),
            [0.013882084, 0.010411563, 0.024987752]
        );
        assert_eq!(get_scores("sunny", &model), [0.08600858]);
        assert_eq!(get_scores("monitoring", &model), [0.05017167]);
        assert_eq!(get_scores("stations", &model), [0.05017167]);
        assert_eq!(get_scores("parts", &model), [0.05017167]);
        assert_eq!(get_scores("haze", &model), [0.06689556]);
        assert_eq!(get_scores("hit", &model), [0.06689556]);
        assert_eq!(get_scores("worse", &model), [0.03344778, 0.060206003]);
    }
}
