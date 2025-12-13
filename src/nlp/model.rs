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
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// accessor method for the collection of `Term`s and `TermMetaData`
    fn terms(&self) -> &HashMap<Term, TermMetaData> {
        self.terms.borrow()
    }

    /// accessor method for the number of `Document`s the model has processed
    pub(crate) fn num_documents(&self) -> usize {
        self.num_documents
    }

    /// add a `Document` to the model
    pub(crate) fn add_document(&mut self, document: Document) {
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
    pub(crate) fn calculate_tf_idf_scores(&mut self) {
        for metadata in self.terms.borrow_mut().values_mut() {
            let num_frequencies = metadata.term_frequencies().len();

            let mut to_add = Vec::with_capacity(num_frequencies);

            for frequency in metadata.term_frequencies() {
                let idf = inverse_document_frequency(
                    self.num_documents as f32,
                    metadata.document_frequency() as f32,
                );

                let score = tf_idf_score(*frequency, idf);
                to_add.push(score);
            }

            let average = if to_add.is_empty() {
                0.0
            } else {
                to_add.iter().sum::<f32>() / to_add.len() as f32
            };

            *metadata.tf_idf_score_mut() = average;
        }
    }

    /// select all terms with a non-zero tf-idf score
    pub(crate) fn all_words(&self) -> Vec<String> {
        self.terms()
            .iter()
            .filter(|(_, metadata)| metadata.tf_idf_score() > 0.0)
            .map(|(term, _)| term.raw().to_owned())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// helper for this test suite
    fn get_score(word: &str, model: &TfIdf) -> f32 {
        model.terms().get(&Term::new(word)).unwrap().tf_idf_score()
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

        assert_eq!(get_score("quality", &model), 0.0);
        assert_eq!(get_score("air", &model), 0.0);
        assert_eq!(get_score("wednesday", &model), 0.018906077);
        assert_eq!(get_score("island", &model), 0.014047348);
        assert_eq!(get_score("singapore", &model), 0.016427131);
        assert_eq!(get_score("sunny", &model), 0.08600858);
        assert_eq!(get_score("monitoring", &model), 0.05017167);
        assert_eq!(get_score("stations", &model), 0.05017167);
        assert_eq!(get_score("parts", &model), 0.05017167);
        assert_eq!(get_score("haze", &model), 0.06689556);
        assert_eq!(get_score("hit", &model), 0.06689556);
        assert_eq!(get_score("worse", &model), 0.04682689);
    }

    #[test]
    /// given the example data at https://remykarem.github.io/tfidf-demo/, ensure the model
    /// produces the same results
    fn select_n_words_grabs_correct_words() {
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

        assert_eq!(model.num_documents(), 4);

        model.calculate_tf_idf_scores();

        let non_zero_words = model.all_words();

        [
            "gradually",
            "network",
            "hit",
            "located",
            "continued",
            "island",
            "worse",
            "monitored",
            "monitoring",
            "haze",
            "different",
            "stations",
            "sunny",
            "singapore",
            "improved",
            "parts",
            "wednesday",
        ]
        .iter()
        .for_each(|word| {
            assert!(non_zero_words.contains(&word.to_string()));
        });
    }
}
