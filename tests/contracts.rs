use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use hnbot::article::Article;
use hnbot::content::html_to_markdown;
use hnbot::http::retry_after_duration;
use hnbot::rss::HnEntry;
use hnbot::telegram::build_message;
use hnbot::telegraph::sanitize_telegraph_html;
use serde_json::Value;

#[test]
fn shared_contract_fixtures_match_rust_behavior() {
    let contract: Value = serde_json::from_str(include_str!("contracts/parity.json")).unwrap();

    assert_eq!(
        html_to_markdown(contract["html_to_markdown"]["input"].as_str().unwrap()),
        contract["html_to_markdown"]["expected"].as_str().unwrap()
    );

    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    for case in contract["retry_after"].as_array().unwrap() {
        let actual = retry_after_duration(case["value"].as_str().unwrap(), now)
            .map(|duration| duration.as_secs_f64());
        assert_eq!(actual, case["expected_seconds"].as_f64());
    }

    let article: Article = serde_json::from_value(contract["article"]["value"].clone()).unwrap();
    assert_eq!(
        article.render_content_text(),
        contract["article"]["rendered"].as_str().unwrap()
    );

    assert_eq!(
        sanitize_telegraph_html(contract["sanitizer"]["input"].as_str().unwrap()),
        contract["sanitizer"]["expected"].as_str().unwrap()
    );

    let message = &contract["message"];
    let entry = HnEntry {
        title: message["entry"]["title"].as_str().unwrap().to_owned(),
        link: message["entry"]["link"].as_str().unwrap().to_owned(),
        comment_url: message["entry"]["comment_url"].as_str().unwrap().to_owned(),
        id: message["entry"]["id"].as_str().unwrap().to_owned(),
        published_at: message["entry"]["published_at"]
            .as_str()
            .unwrap()
            .parse::<DateTime<Utc>>()
            .unwrap(),
        points: message["entry"]["points"]
            .as_u64()
            .map(|value| value as u32),
        num_comments: message["entry"]["num_comments"]
            .as_u64()
            .map(|value| value as u32),
    };
    let article: Article = serde_json::from_value(message["article"].clone()).unwrap();
    assert_eq!(
        build_message(&entry, &article, message["page_url"].as_str().unwrap()),
        message["expected"].as_str().unwrap()
    );
}
