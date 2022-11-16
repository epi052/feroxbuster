use super::*;
use crate::event_handlers::Handles;
use anyhow::{bail, Result};

/// Regular expression used in [LinkFinder](https://github.com/GerbenJavado/LinkFinder)
///
/// Incorporates change from this [Pull Request](https://github.com/GerbenJavado/LinkFinder/pull/66/files)
pub(super) const LINKFINDER_REGEX: &str = r#"(?:"|')(((?:[a-zA-Z]{1,10}://|//)[^"'/]{1,}\.[a-zA-Z]{2,}[^"']{0,})|((?:/|\.\./|\./)[^"'><,;| *()(%%$^/\\\[\]][^"'><,;|()]{1,})|([a-zA-Z0-9_\-/]{1,}/[a-zA-Z0-9_\-/]{1,}\.(?:[a-zA-Z]{1,4}|action)(?:[\?|#][^"|']{0,}|))|([a-zA-Z0-9_\-/]{1,}/[a-zA-Z0-9_\-/]{3,}(?:[\?|#][^"|']{0,}|))|([a-zA-Z0-9_\-.]{1,}\.(?:php|asp|aspx|jsp|json|action|html|js|txt|xml)(?:[\?|#][^"|']{0,}|)))(?:"|')"#;

/// Regular expression to pull url paths from robots.txt
///
/// ref: https://developers.google.com/search/reference/robots_txt
pub(super) const ROBOTS_TXT_REGEX: &str =
    r#"(?m)^ *(Allow|Disallow): *(?P<url_path>[a-zA-Z0-9._/?#@!&'()+,;%=-]+?)$"#; // multi-line (?m)

/// Regular expression to filter bad characters from extracted url paths
///
/// ref: https://www.rfc-editor.org/rfc/rfc3986#section-2
pub(super) const URL_CHARS_REGEX: &str = r#"["<>\\^`{|} ]"#;

/// Which type of extraction should be performed
#[derive(Debug, Copy, Clone)]
pub enum ExtractionTarget {
    /// Examine a response body and extract javascript and html links (multiple tags)
    ResponseBody,

    /// Examine robots.txt (specifically) and extract links
    RobotsTxt,

    /// Extract all <a> tags from a page
    DirectoryListing,
}

/// responsible for building an `Extractor`
pub struct ExtractorBuilder<'a> {
    /// Response from which to extract links
    response: Option<&'a FeroxResponse>,

    /// URL of where to extract links
    url: String,

    /// Handles object to house the underlying mpsc transmitters
    handles: Option<Arc<Handles>>,

    /// type of extraction to be performed
    target: ExtractionTarget,
}

/// ExtractorBuilder implementation
impl<'a> Default for ExtractorBuilder<'a> {
    fn default() -> Self {
        Self {
            response: None,
            url: "".to_string(),
            handles: None,
            target: ExtractionTarget::ResponseBody,
        }
    }
}

/// ExtractorBuilder implementation
impl<'a> ExtractorBuilder<'a> {
    /// builder call to set `handles`
    pub fn handles(&mut self, handles: Arc<Handles>) -> &mut Self {
        self.handles = Some(handles);
        self
    }

    /// builder call to set `url`
    pub fn url(&mut self, url: &str) -> &mut Self {
        self.url = url.to_string();
        self
    }

    /// builder call to set `target`
    pub fn target(&mut self, target: ExtractionTarget) -> &mut Self {
        self.target = target;
        self
    }

    /// builder call to set `response`
    pub fn response(&mut self, response: &'a FeroxResponse) -> &mut Self {
        self.response = Some(response);
        self
    }

    /// finalize configuration of `ExtractorBuilder` and return an `Extractor`
    ///
    /// requires either `with_url` or `with_response` to have been used in the build process
    pub fn build(&self) -> Result<Extractor<'a>> {
        if (self.url.is_empty() && self.response.is_none()) || self.handles.is_none() {
            bail!("Extractor requires a URL or a FeroxResponse be specified as well as a Handles object")
        }

        Ok(Extractor {
            links_regex: Regex::new(LINKFINDER_REGEX).unwrap(),
            robots_regex: Regex::new(ROBOTS_TXT_REGEX).unwrap(),
            url_regex: Regex::new(URL_CHARS_REGEX).unwrap(),
            response: if self.response.is_some() {
                Some(self.response.unwrap())
            } else {
                None
            },
            url: self.url.to_owned(),
            handles: self.handles.as_ref().unwrap().clone(),
            target: self.target,
        })
    }
}
