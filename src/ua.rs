pub fn classify(user_agent: &str) -> (&'static str, &'static str) {
    let ua = user_agent.to_lowercase();
    (browser(&ua), device(&ua))
}

fn browser(ua: &str) -> &'static str {
    if ua.contains("edg/") || ua.contains("edga/") || ua.contains("edgios/") {
        "Edge"
    } else if ua.contains("firefox") || ua.contains("fxios") {
        "Firefox"
    } else if ua.contains("chrome") || ua.contains("crios") || ua.contains("chromium") {
        "Chrome"
    } else if ua.contains("safari") {
        "Safari"
    } else {
        "Other"
    }
}

fn device(ua: &str) -> &'static str {
    if ua.contains("mobi") || ua.contains("android") || ua.contains("iphone") {
        "Mobile"
    } else {
        "Desktop"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHROME: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
    const SAFARI: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Safari/605.1.15";
    const FIREFOX: &str =
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:127.0) Gecko/20100101 Firefox/127.0";
    const EDGE: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36 Edg/126.0.0.0";
    const IPHONE: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1";
    const ANDROID_CHROME: &str = "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36";

    #[test]
    fn browser_picks_family_in_priority_order() {
        // Edge and Chrome UAs both contain "safari"/"chrome"; order must disambiguate.
        assert_eq!(browser(&EDGE.to_lowercase()), "Edge");
        assert_eq!(browser(&CHROME.to_lowercase()), "Chrome");
        assert_eq!(browser(&FIREFOX.to_lowercase()), "Firefox");
        assert_eq!(browser(&SAFARI.to_lowercase()), "Safari");
        assert_eq!(browser("some-random-agent"), "Other");
    }

    #[test]
    fn device_splits_mobile_from_desktop() {
        assert_eq!(device(&CHROME.to_lowercase()), "Desktop");
        assert_eq!(device(&IPHONE.to_lowercase()), "Mobile");
        assert_eq!(device(&ANDROID_CHROME.to_lowercase()), "Mobile");
    }

    #[test]
    fn classify_returns_both_dimensions() {
        assert_eq!(classify(ANDROID_CHROME), ("Chrome", "Mobile"));
        assert_eq!(classify(SAFARI), ("Safari", "Desktop"));
    }
}
