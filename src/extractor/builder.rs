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

/// Which type of extraction should be performed
#[derive(Debug, Copy, Clone)]
pub enum ExtractionTarget {
    /// Examine a response body and extract links
    ResponseBody,

    /// Examine robots.txt (specifically) and extract links
    RobotsTxt,
}

/// responsible for building an `Extractor`
pub struct ExtractorBuilder<'a> {
    /// Response from which to extract links
    response: Option<&'a FeroxResponse>,

    /// Response from which to extract links
    url: String,

    /// current configuration
    config: Option<&'a Configuration>,

    /// Handles object to house the underlying mpsc transmitters
    handles: Option<Arc<Handles>>,

    /// type of extraction to be performed
    target: ExtractionTarget,
}

/// ExtractorBuilder implementation
impl<'a> ExtractorBuilder<'a> {
    /// Given a FeroxResponse, create new ExtractorBuilder
    ///
    /// Once built, Extractor::target is ExtractionTarget::ResponseBody
    pub fn with_response(response: &'a FeroxResponse) -> Self {
        Self {
            response: Some(response),
            url: "".to_string(),
            config: None,
            handles: None,
            target: ExtractionTarget::ResponseBody,
        }
    }

    /// Given a url and Stats transmitter, create new ExtractorBuilder
    ///
    /// Once built, Extractor::target is ExtractionTarget::ResponseBody
    pub fn with_url(url: &str) -> Self {
        Self {
            response: None,
            url: url.to_string(),
            config: None,
            handles: None,
            target: ExtractionTarget::RobotsTxt,
        }
    }

    /// builder call to set `config`
    pub fn config(&mut self, config: &'a Configuration) -> &mut Self {
        self.config = Some(config);
        self
    }

    /// builder call to set `handles`
    pub fn handles(&mut self, handles: Arc<Handles>) -> &mut Self {
        self.handles = Some(handles);
        self
    }

    /// finalize configuration of ExtratorBuilder and return an Extractor
    ///
    /// requires either with_url or with_response to have been used in the build process
    pub fn build(&self) -> Result<Extractor<'a>> {
        if self.url.is_empty() && self.response.is_none() {
            bail!("Extractor requires either a URL or a FeroxResponse be specified")
        }

        Ok(Extractor {
            links_regex: Regex::new(LINKFINDER_REGEX).unwrap(),
            robots_regex: Regex::new(ROBOTS_TXT_REGEX).unwrap(),
            response: if self.response.is_some() {
                Some(self.response.unwrap())
            } else {
                None
            },
            url: self.url.to_owned(),
            config: self.config.unwrap(),
            handles: self.handles.as_ref().unwrap().clone(),
            target: self.target,
        })
    }
}
