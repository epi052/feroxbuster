use super::constants::{BOUNDED_WORD_REGEX, STOP_WORDS};
use regex::Captures;
use std::borrow::Cow;

/// pre-processing pipeline wrapper that removes punctuation, normalizes word case (utf-8 included)
/// to lowercase, and remove stop words
pub(crate) fn preprocess(text: &str) -> Vec<String> {
    let text = remove_punctuation(text);
    let text = normalize_case(text);
    let text = remove_stop_words(&text);

    text.split_whitespace()
        .map(|word| word.to_string())
        .collect::<Vec<_>>()
}

/// optimized version of `str::to_lowercase`
fn normalize_case<'a, S: Into<Cow<'a, str>>>(input: S) -> Cow<'a, str> {
    let input = input.into();

    let first = input.find(char::is_uppercase);

    if let Some(first_idx) = first {
        let mut output = String::from(&input[..first_idx]);
        output.reserve(input.len() - first_idx);

        for c in input[first_idx..].chars() {
            if c.is_uppercase() {
                output.push(c.to_lowercase().next().unwrap())
            } else {
                output.push(c)
            }
        }

        Cow::Owned(output)
    } else {
        input
    }
}

/// replace ascii and some utf-8 punctuation characters with ' ' (space) in the given string
fn remove_punctuation(text: &str) -> String {
    text.replace(
        [
            '!', '\\', '"', '#', '$', '%', '&', '(', ')', '*', '+', ':', ';', '<', '=', '>', '?',
            '@', '[', ']', '^', '{', '}', '|', '~', ',', '\'', '“', '”', '’', '‘', '’', '‘', '/',
            '–', '—', '.',
        ],
        " ",
    )
}

/// remove stop words from the given string
fn remove_stop_words(text: &str) -> String {
    BOUNDED_WORD_REGEX
        .replace_all(text, |caps: &Captures| {
            let word = &caps[0];
            if !STOP_WORDS.contains(&word) {
                word.to_owned()
            } else {
                String::new()
            }
        })
        .into()
}

/// calculate inverse document frequency
pub(super) fn inverse_document_frequency(num_docs: f32, doc_frequency: f32) -> f32 {
    f32::log10(num_docs / doc_frequency)
}

/// calculate term frequency-inverse document frequency (tf-idf)
pub(super) fn tf_idf_score(term_frequency: f32, idf: f32) -> f32 {
    term_frequency * idf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// ensure all expected punctuation characters are removed
    fn test_remove_punctuation() {
        let tester = "!\\\"#$%&()*+/:;<=>?@[]^{}|~,.'“”’‘–—\n‘’";
        // the `"    \n"` is because of the things like / getting replaced with a space
        assert_eq!(
            remove_punctuation(tester),
            "                                   \n  "
        );
    }

    #[test]
    /// ensure uppercase characters are swapped to lowercase
    fn test_normalize_case() {
        let tester = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
        assert_eq!(normalize_case(tester), "abcdefghijklmnopqrstuvwxyz");
    }

    #[test]
    /// ensure all stop words are removed from the list of stopwords ... intestuous
    fn test_remove_stopwords() {
        let all_words = STOP_WORDS
            .iter()
            .map(|&word| word.to_string())
            .collect::<Vec<_>>()
            .join(" ");

        let removed = remove_stop_words(&all_words).replace(' ', "");

        // the remaining chars are from the contraction-based stop words
        assert_eq!(removed, "'d'll'm''s'ven'tn‘tn’t‘d‘ll‘m‘‘s‘ve’d’ll’m’’s’ve");
    }

    #[test]
    /// ensure preprocess
    fn test_preprocess_results() {
        let tester = "WHY are Y'all YELLing?";
        assert_eq!(&preprocess(tester), &["y", "all", "yelling"]);
    }

    #[test]
    /// ensure our calculations conform to the example provided at the link below
    ///
    /// https://www.kaggle.com/paulrohan2020/tf-idf-tutorial/notebook#TF-IDF-Model
    ///
    /// Consider a document containing 100 words wherein the word cat appears 3 times.
    /// The term frequency (i.e., tf) for cat is then (3 / 100) = 0.03. Now, assume we have 10
    /// million documents and the word cat appears in one thousand of these. Then, the inverse
    /// document frequency (i.e., idf) is calculated as log(10,000,000 / 1,000) = 4. Thus, the
    /// Tf-idf weight is the product of these quantities: 0.03 * 4 = 0.12.
    fn idf_returns_expected_value() {
        let num_docs = 10_000_000_f32;
        let num_occurrences = 1_000_f32;
        let abs_diff = (inverse_document_frequency(num_docs, num_occurrences) - 4.0).abs();

        assert!(abs_diff <= f32::EPSILON);
    }

    #[test]
    /// ensure our calculations conform to the example provided at the link below
    ///
    /// https://www.kaggle.com/paulrohan2020/tf-idf-tutorial/notebook#TF-IDF-Model
    ///
    /// Consider a document containing 100 words wherein the word cat appears 3 times.
    /// The term frequency (i.e., tf) for cat is then (3 / 100) = 0.03. Now, assume we have 10
    /// million documents and the word cat appears in one thousand of these. Then, the inverse
    /// document frequency (i.e., idf) is calculated as log(10,000,000 / 1,000) = 4. Thus, the
    /// Tf-idf weight is the product of these quantities: 0.03 * 4 = 0.12.
    fn tf_idf_returns_expected_value() {
        let term_freq = 0.03_f32;
        let num_docs = 10_000_000_f32;
        let num_occurrences = 1_000_f32;
        let idf = inverse_document_frequency(num_docs, num_occurrences);
        let abs_diff = (tf_idf_score(term_freq, idf) - 0.12).abs();

        assert!(abs_diff <= f32::EPSILON);
    }
}
