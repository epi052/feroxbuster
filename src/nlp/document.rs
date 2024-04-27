use super::term::{Term, TermMetaData};
use super::utils::preprocess;
use scraper::{Html, Node, Selector};
use std::collections::HashMap;

/// data container representing a single document, in the nlp sense
#[derive(Debug, Default)]
pub(crate) struct Document {
    /// collection of `Term`s and their associated metadata
    terms: HashMap<Term, TermMetaData>,

    /// number of terms contained within the document
    number_of_terms: usize,
}

impl Document {
    /// create a new `Document` from the given string
    pub(super) fn new(text: &str) -> Self {
        let mut document = Self::default();

        let processed = preprocess(text);

        document.number_of_terms += processed.len();

        for normalized in processed {
            if normalized.len() >= 2 {
                document.add_term(&normalized)
            }
        }
        document
    }

    /// add a `Term` to the document if it's not already tracked, otherwise increment the number
    /// of times the term has been seen
    fn add_term(&mut self, word: &str) {
        let term = Term::new(word);

        let metadata = self.terms.entry(term).or_default();
        *metadata.count_mut() += 1;
    }

    /// create a new `Document` from the given HTML string
    pub(crate) fn from_html(raw_html: &str) -> Option<Self> {
        let selector = Selector::parse("body").unwrap();

        let html = Html::parse_document(raw_html);

        let element = html.select(&selector).next()?;

        let text = element
            .descendants()
            .filter_map(|node| {
                if !node.value().is_text() && !node.value().is_comment() {
                    return None;
                }

                // have a Text||Comment node, trim whitespace to test for all whitespace stuff
                let trimmed = if node.value().is_text() {
                    node.value().as_text().unwrap().text.trim()
                } else {
                    node.value().as_comment().unwrap().comment.trim()
                };

                if trimmed.is_empty() {
                    return None;
                }

                // found a non-empty Text||Comment node, need to check its parent to determine if
                // it's a <script>||<style> tag. We're assuming text within a script||style tag is
                // uninteresting

                let parent = node.parent().unwrap().value();

                if !parent.is_element() {
                    return None;
                }

                // parent is an Element node, see if it's a <script> or <style>

                if let Node::Element(element) = parent {
                    if element.name() == "script" || element.name() == "style" {
                        return None;
                    }

                    // at this point, we have a non-empty Text element with a non-script|style parent;
                    // now we can return the trimmed up string
                    return Some(format!("{trimmed} "));
                }

                // not an Element node
                None
            })
            .collect::<String>();

        // call `new` to push the parsed html through the pre-processing pipeline and process all
        // the words
        Some(Self::new(&text))
    }

    /// Log normalized weighting scheme for term frequency
    pub(super) fn term_frequency(&self, term: &Term) -> f32 {
        if let Some(metadata) = self.terms.get(term) {
            metadata.count() as f32 / self.number_of_terms() as f32
        } else {
            0.0
        }
    }

    /// immutable reference to the collection of terms and their metadata
    pub(super) fn terms(&self) -> &HashMap<Term, TermMetaData> {
        &self.terms
    }

    /// number of terms the current document knows about
    fn number_of_terms(&self) -> usize {
        self.number_of_terms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// `Document::new` should preprocess text and generate a hashmap of `Term, TermMetadata`
    fn nlp_document_creation_from_text() {
        let doc = Document::new("The air quality in Singapore got worse on Wednesday.");

        let expected_terms = ["air", "quality", "singapore", "worse", "wednesday"];

        for expected in expected_terms {
            let term = Term::new(expected);
            assert!(doc.terms().contains_key(&term));
            assert_eq!(doc.number_of_terms, 5);
            assert_eq!(doc.terms().get(&term).unwrap().count(), 1);

            // since term frequencies aren't calculated on `new`, document frequency is zero in
            // addition to the empty term_frequencies slice
            let empty: &[f32] = &[];
            assert_eq!(doc.terms().get(&term).unwrap().term_frequencies(), empty);
            assert_eq!(doc.terms().get(&term).unwrap().document_frequency(), 0);
        }
    }

    #[test]
    /// `Document::new` should preprocess html and generate a hashmap of `Term, TermMetadata`
    fn nlp_document_creation_from_html() {
        let empty = Document::from_html("<html></html>").unwrap();
        assert_eq!(empty.number_of_terms, 0);

        let other_empty = Document::from_html("<html><body><p></p></body></html>").unwrap();
        assert_eq!(other_empty.number_of_terms, 0);

        let third_empty =
            Document::from_html("<!DOCTYPE html><html><!DOCTYPE html><p></p></html>").unwrap();
        assert_eq!(third_empty.number_of_terms, 0);

        // p tag for is_text check and comment for is_comment
        let doc = Document::from_html(
            "<html><body><p>The air quality in Singapore.</p><!--got worse on Wednesday--></body></html>",
        ).unwrap();

        let expected_terms = ["air", "quality", "singapore", "worse", "wednesday"];

        for expected in expected_terms {
            let term = Term::new(expected);
            assert_eq!(doc.number_of_terms, 5);
            assert!(doc.terms().contains_key(&term));
            assert_eq!(doc.terms().get(&term).unwrap().count(), 1);

            // since term frequencies aren't calculated on `new`, document frequency is zero in
            // addition to the empty term_frequencies slice
            let empty: &[f32] = &[];
            assert_eq!(doc.terms().get(&term).unwrap().term_frequencies(), empty);
            assert_eq!(doc.terms().get(&term).unwrap().document_frequency(), 0);
        }
    }

    #[test]
    /// simple check of the `term_frequency` function's return value
    fn term_frequency_validation() {
        let doc = Document::new("The air quality in Singapore got worse on Wednesday. Air Jordan.");

        let air_freq = doc.term_frequency(&Term::new("air"));

        let abs_diff = (air_freq - 0.2857143).abs();
        assert!(abs_diff <= f32::EPSILON);

        let non_existent = doc.term_frequency(&Term::new("derpatronic"));
        assert_eq!(non_existent, 0.0);
    }

    #[test]
    /// test accessors for correctness
    fn document_accessor_test() {
        let doc = Document::new("The air quality in Singapore got worse on Wednesday.");
        let keys = doc.terms().keys().map(|key| key.raw()).collect::<Vec<_>>();

        let expected = ["air", "quality", "singapore", "worse", "wednesday"];

        assert_eq!(doc.number_of_terms(), 5);

        for key in keys {
            assert!(expected.contains(&key));
        }
    }

    #[test]
    /// ensure words in script/style tags aren't processed
    fn document_creation_skips_script_and_style_tags() {
        let html = "<body><script>The air quality</script><style>in Singapore</style><p>got worse on Wednesday.</p></body>";
        let doc = Document::from_html(html).unwrap();
        let keys = doc.terms().keys().map(|key| key.raw()).collect::<Vec<_>>();

        let expected = ["worse", "wednesday"];

        assert_eq!(doc.number_of_terms(), 2);

        for key in keys {
            assert!(expected.contains(&key));
        }
    }
}
