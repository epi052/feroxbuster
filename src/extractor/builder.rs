use super::*;
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

    /// Whether or not to try recursion
    config: Option<&'a Configuration>,

    /// transmitter to the mpsc that handles statistics gathering
    tx_stats: Option<UnboundedSender<StatCommand>>,

    /// transmitter to the mpsc that handles recursive scan calls
    tx_recursion: Option<UnboundedSender<String>>,

    /// transmitter to the mpsc that handles reporting information to the user
    tx_reporter: Option<UnboundedSender<FeroxResponse>>,

    /// list of urls that will be added to when new urls are extracted
    scanned_urls: Option<&'a FeroxScans>,

    /// depth at which the scan was started
    depth: Option<usize>,

    /// copy of Stats object
    stats: Option<Arc<Stats>>,

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
            tx_stats: None,
            tx_recursion: None,
            tx_reporter: None,
            scanned_urls: None,
            depth: None,
            stats: None,
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
            tx_stats: None,
            tx_recursion: None,
            tx_reporter: None,
            scanned_urls: None,
            depth: None,
            stats: None,
            target: ExtractionTarget::RobotsTxt,
        }
    }

    /// builder call to set `config`
    pub fn config(&mut self, config: &'a Configuration) -> &mut Self {
        self.config = Some(config);
        self
    }

    /// builder call to set `tx_recursion`
    pub fn recursion_transmitter(&mut self, tx_recursion: UnboundedSender<String>) -> &mut Self {
        self.tx_recursion = Some(tx_recursion);
        self
    }

    /// builder call to set `tx_stats`
    pub fn stats_transmitter(&mut self, tx_stats: UnboundedSender<StatCommand>) -> &mut Self {
        self.tx_stats = Some(tx_stats);
        self
    }

    /// builder call to set `tx_reporter`
    pub fn reporter_transmitter(
        &mut self,
        tx_reporter: UnboundedSender<FeroxResponse>,
    ) -> &mut Self {
        self.tx_reporter = Some(tx_reporter);
        self
    }

    /// builder call to set `scanned_urls`
    pub fn scanned_urls(&mut self, scanned_urls: &'a FeroxScans) -> &mut Self {
        self.scanned_urls = Some(scanned_urls);
        self
    }

    /// builder call to set `stats`
    pub fn stats(&mut self, stats: Arc<Stats>) -> &mut Self {
        self.stats = Some(stats);
        self
    }

    /// builder call to set `depth`
    pub fn depth(&mut self, depth: usize) -> &mut Self {
        self.depth = Some(depth);
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
            tx_stats: self.tx_stats.as_ref().unwrap().clone(),
            tx_recursion: self.tx_recursion.as_ref().unwrap().clone(),
            tx_reporter: self.tx_reporter.as_ref().unwrap().clone(),
            scanned_urls: self.scanned_urls.unwrap(),
            depth: self.depth.unwrap(),
            stats: self.stats.as_ref().unwrap().clone(),
            target: self.target,
        })
    }
}
