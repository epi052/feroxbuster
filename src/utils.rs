use reqwest::Url;

/// Helper function that determines the current depth of a given url
///
/// Essentially looks at the Url path and determines how many directories are present in the
/// given Url
///
/// http://localhost -> 1
/// http://localhost/ -> 1
/// http://localhost/stuff -> 2
/// ...
///
/// returns 0 on error and relative urls
pub fn get_current_depth(target: &str) -> usize {
    log::trace!("enter: get_current_depth({})", target);

    let target = if !target.ends_with('/') {
        // target url doesn't end with a /, for the purposes of determining depth, we'll normalize
        // all urls to end in a / and then calculate accordingly
        format!("{}/", target)
    } else {
        String::from(target)
    };

    match Url::parse(&target) {
        Ok(url) => {
            if let Some(parts) = url.path_segments() {
                // at least an empty string returned by the Split, meaning top-level urls
                let mut depth = 0;

                for _ in parts {
                    depth += 1;
                }

                let return_val = depth;

                log::trace!("exit: get_current_depth -> {}", return_val);
                return return_val;
            };

            log::debug!("get_current_depth called on a Url that cannot be a base: {}", url);
            log::trace!("exit: get_current_depth -> 0");

            0
        }
        Err(e) => {
            log::error!("could not parse to url: {}", e);
            log::trace!("exit: get_current_depth -> 0");
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_returns_1() {
        let depth = get_current_depth("http://localhost");
        assert_eq!(depth, 1);
    }

    #[test]
    fn base_url_with_slash_returns_1() {
        let depth = get_current_depth("http://localhost/");
        assert_eq!(depth, 1);
    }

    #[test]
    fn one_dir_returns_2() {
        let depth = get_current_depth("http://localhost/src");
        assert_eq!(depth, 2);
    }

    #[test]
    fn one_dir_with_slash_returns_2() {
        let depth = get_current_depth("http://localhost/src/");
        assert_eq!(depth, 2);
    }
}