use std::collections::HashSet;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::api::Api;

const USAGE: &str = "/api/execution-usage";
const PRICES: &str = "/api/execution-usage/prices";
const QUEUE: &str = "/api/execution-queue";
const CONFIG: &str = "/api/execution-queue/config";

#[derive(Deserialize)]
pub(super) struct UsageTotal {
    pub executor: String,
    pub model: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub cost_usd: Option<f64>,
    pub unpriced_samples: i64,
}

#[derive(Clone, PartialEq, Deserialize, Serialize)]
pub(super) struct Price {
    pub executor: String,
    pub model: String,
    pub input_usd_per_million: f64,
    pub output_usd_per_million: f64,
    pub cache_read_usd_per_million: f64,
    pub cache_write_usd_per_million: f64,
}

#[derive(Deserialize)]
pub(super) struct QueuedExecution {
    pub execution_process_id: String,
    pub session_id: String,
    pub workspace_id: String,
    pub priority: i32,
    pub queued_at: String,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub(super) struct Quota {
    pub max_concurrent: u32,
}

#[derive(Serialize)]
struct Priority {
    priority: i32,
}

pub(super) async fn usage(api: &Api, session_id: Option<&str>) -> Result<Vec<UsageTotal>, String> {
    if let Some(id) = session_id {
        // Only local session UUIDs may reach the query string.
        if !is_uuid(id) {
            return Err("会话 ID 必须为 UUID。".into());
        }
        return api
            .request("GET", &format!("{USAGE}?session_id={id}"), None)
            .await;
    }
    api.request("GET", USAGE, None).await
}

fn is_uuid(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}

pub(super) async fn queue(api: &Api) -> Result<Vec<QueuedExecution>, String> {
    api.request("GET", QUEUE, None).await
}

pub(super) async fn quota(api: &Api) -> Result<Quota, String> {
    api.request("GET", CONFIG, None).await
}

pub(super) async fn prices(api: &Api) -> Result<Vec<Price>, String> {
    api.request("GET", PRICES, None).await
}

pub(super) fn parse_quota(value: &str) -> Result<Quota, String> {
    let max_concurrent = value
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| "并发上限必须是 1 至 4294967295 的整数。".to_owned())?;
    Ok(Quota { max_concurrent })
}

pub(super) fn parse_priority(value: &str) -> Result<i32, String> {
    value
        .trim()
        .parse()
        .map_err(|_| "优先级必须是 -2147483648 至 2147483647 的整数。".into())
}

pub(super) async fn save_quota(api: &Api, value: Quota) -> Result<Quota, String> {
    send(api, "PUT", CONFIG, &value).await
}

pub(super) async fn save_priority(api: &Api, id: &str, priority: i32) -> Result<(), String> {
    send(
        api,
        "PATCH",
        &format!("{QUEUE}/{id}"),
        &Priority { priority },
    )
    .await
}

pub(super) async fn save_prices(api: &Api, value: &[Price]) -> Result<Vec<Price>, String> {
    send(api, "PUT", PRICES, value).await
}

async fn send<Response: DeserializeOwned>(
    api: &Api,
    method: &str,
    path: &str,
    data: &(impl Serialize + ?Sized),
) -> Result<Response, String> {
    let body = serde_json::to_vec(data).map_err(|error| error.to_string())?;
    api.request(method, path, Some(body)).await
}

impl Price {
    pub fn from_fields(fields: &[String; 6]) -> Result<Self, String> {
        let [executor, model, input, output, cache_read, cache_write] =
            fields.each_ref().map(String::as_str);
        if executor.trim().is_empty() || model.trim().is_empty() {
            return Err("请填写执行器和模型 ID。".into());
        }
        let rate = |value: &str, label: &str| {
            value
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|rate| rate.is_finite() && *rate >= 0.0)
                .ok_or_else(|| format!("{label}费率必须是有限的非负数。"))
        };
        Ok(Self {
            executor: executor.into(),
            model: model.into(),
            input_usd_per_million: rate(input, "输入")?,
            output_usd_per_million: rate(output, "输出")?,
            cache_read_usd_per_million: rate(cache_read, "缓存读取")?,
            cache_write_usd_per_million: rate(cache_write, "缓存写入")?,
        })
    }

    pub fn fields(&self) -> [String; 6] {
        let [input, output, cache_read, cache_write] = self.rates().map(|rate| rate.to_string());
        [
            self.executor.clone(),
            self.model.clone(),
            input,
            output,
            cache_read,
            cache_write,
        ]
    }

    pub fn rates(&self) -> [f64; 4] {
        [
            self.input_usd_per_million,
            self.output_usd_per_million,
            self.cache_read_usd_per_million,
            self.cache_write_usd_per_million,
        ]
    }
}

pub(super) fn distinct_prices(prices: &[Price]) -> Result<(), String> {
    let mut seen = HashSet::with_capacity(prices.len());
    if prices
        .iter()
        .all(|price| seen.insert((price.executor.as_str(), price.model.as_str())))
    {
        Ok(())
    } else {
        Err("执行器和模型 ID 的组合不能重复。".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_filter_admits_only_a_uuid() {
        assert!(is_uuid("00000000-0000-4000-8000-000000000001"));
        assert!(!is_uuid("00000000-0000-4000-8000-000000000001&"));
        assert!(!is_uuid("00000000-0000-4000-8000-00000000000g"));
    }

    #[test]
    fn writes_follow_server_ranges_and_price_keys() {
        assert!(parse_quota("0").is_err());
        assert!(parse_quota("4294967296").is_err());
        assert_eq!(parse_quota("4294967295").unwrap().max_concurrent, u32::MAX);
        assert_eq!(parse_priority("-2147483648").unwrap(), i32::MIN);
        assert!(parse_priority("2147483648").is_err());

        let mut fields = ["COPILOT", "*", "2", "8", "0.5", "0"].map(str::to_owned);
        let price = Price::from_fields(&fields).unwrap();
        assert_eq!(price.rates(), [2.0, 8.0, 0.5, 0.0]);
        assert!(distinct_prices(&[price.clone(), price]).is_err());
        for invalid in ["", "-0.01", "NaN", "inf"] {
            fields[5] = invalid.into();
            assert!(Price::from_fields(&fields).is_err());
        }
    }
}
